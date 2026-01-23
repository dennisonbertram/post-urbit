use std::path::Path;
use std::sync::Arc;

use tracing::info;
use serde_json::json;

use crate::dht::MemoryDht;
use crate::admin_auth::{AuthConfig, generate_token_hex};
use crate::admin_state::AdminState;
use crate::diagnostics;
use crate::event_bus::EventBus;
use crate::identity::{publish_genesis, publish_identity, IdentityManager};
use crate::node_config::default_node_settings;
use crate::health::{HealthState, ReadinessDetails};
use crate::node_http::{run_http_server, HttpServerConfig, HttpServerState};
use crate::runtime_wasm::RuntimeManager;
use crate::scheduler::Scheduler;
use crate::transport::QuicTransport;
use crate::error::{PostUrbitError, Result};

#[derive(Clone)]
pub struct NodeConfig {
    pub port: u16,
    pub data_dir: String,
    pub bootstrap_peers: Vec<String>,
    pub http_addr: std::net::SocketAddr,
    pub metrics_enabled: bool,
    pub admin_password_hash: Option<String>,
    pub admin_token_hash: Option<String>,
    pub session_secret: Option<String>,
    pub session_timeout_hours: u32,
    /// Development mode - bypasses authentication (UNSAFE for production)
    pub dev_mode: bool,
}

pub struct PostUrbitNode {
    config: NodeConfig,
    identity: Arc<IdentityManager>,
    transport: Arc<QuicTransport>,
    dht: Arc<MemoryDht>,
    admin: AdminState,
    apps_dir: std::path::PathBuf,
    event_bus: Arc<EventBus>,
}

impl PostUrbitNode {
    pub async fn new(config: NodeConfig) -> Result<Self> {
        info!("Initializing Post-Urbit node...");

        ensure_data_directories(Path::new(&config.data_dir)).await?;

        let identity_dir = Path::new(&config.data_dir).join("identity");
        let identity = Arc::new(IdentityManager::new(identity_dir.to_string_lossy().as_ref()).await?);
        info!("Identity loaded: {}", identity.iid().await);

        let dht = Arc::new(MemoryDht::new());
        info!("DHT initialized (in-memory)");

        let transport = Arc::new(QuicTransport::new(config.port, identity.clone()).await?);
        info!("QUIC transport initialized on port {}", config.port);

        let idoc = identity.identity_document().await;
        if idoc.sequence == "0" {
            publish_genesis(dht.as_ref(), &idoc).await?;
        } else {
            publish_identity(dht.as_ref(), &idoc).await?;
        }
        info!("Identity published to DHT");

        let log_dir = Path::new(&config.data_dir).join("logs");
        let settings = default_node_settings(
            config.data_dir.as_str(),
            log_dir.to_string_lossy().as_ref(),
        );
        let admin = AdminState::load(&config.data_dir, settings).await?;
        let apps_dir = Path::new(&config.data_dir).join("apps").join("installed");
        let event_bus = Arc::new(EventBus::new());

        Ok(Self {
            config,
            identity,
            transport,
            dht,
            admin,
            apps_dir,
            event_bus,
        })
    }

    pub async fn run(self) -> Result<()> {
        info!("Post-Urbit node running...");
        info!("Node IID: {}", self.identity.iid().await);
        info!("Listening on port {}", self.config.port);

        let transport = self.transport.clone();
        let transport_handle = tokio::spawn(async move { transport.run().await });

        let session_secret = load_or_create_session_secret(&self.config).await?;
        let auth = AuthConfig {
            password_hash: self.config.admin_password_hash.clone(),
            admin_token_hash: self.config.admin_token_hash.clone(),
            session_secret,
            session_timeout_hours: self.config.session_timeout_hours,
        };
        let scheduler = Scheduler::new();
        let identity = self.identity.clone();
        let dht = self.dht.clone();
        let admin_sessions = self.admin.clone();
        let admin_repo = self.admin.clone();
        let event_bus = self.event_bus.clone();
        scheduler
            .schedule("identity_publish", std::time::Duration::from_secs(60 * 60 * 24), move || {
                let identity = identity.clone();
                let dht = dht.clone();
                async move {
                    let doc = identity.identity_document().await;
                    publish_identity(dht.as_ref(), &doc).await.map_err(|err| format!("{err:?}"))?;
                    Ok(())
                }
            })
            .await;
        scheduler
            .schedule("session_cleanup", std::time::Duration::from_secs(60 * 60), move || {
                let admin = admin_sessions.clone();
                async move {
                    admin.prune_sessions().await;
                    let _ = admin.persist().await;
                    Ok(())
                }
            })
            .await;
        scheduler
            .schedule("repo_cache_cleanup", std::time::Duration::from_secs(60 * 60), move || {
                let admin = admin_repo.clone();
                async move {
                    admin.prune_repo_cache(chrono::Duration::hours(1)).await;
                    let _ = admin.persist().await;
                    Ok(())
                }
            })
            .await;
        scheduler
            .schedule("health_self_check", std::time::Duration::from_secs(60), move || {
                let event_bus = event_bus.clone();
                async move {
                    event_bus.emit("status_change", json!({"status": "healthy"})).await;
                    Ok(())
                }
            })
            .await;
        let health = HealthState::new();
        let started_at = std::time::Instant::now();
        let runtime = Arc::new(tokio::sync::Mutex::new(RuntimeManager::new()));
        if self.config.dev_mode {
            tracing::warn!("⚠️  DEVELOPMENT MODE ENABLED - Authentication is bypassed! Do not use in production.");
        }
        let http_state = HttpServerState {
            admin: self.admin.clone(),
            auth,
            identity: self.identity.clone(),
            dht: self.dht.clone(),
            event_bus: self.event_bus.clone(),
            started_at,
            config: HttpServerConfig {
                metrics_enabled: self.config.metrics_enabled,
                max_request_body_bytes: 100 * 1024 * 1024,
                session_cookie_secure: false,
                dev_mode: self.config.dev_mode,
            },
            health: health.clone(),
            apps_dir: self.apps_dir.clone(),
            runtime,
        };
        let http_addr = self.config.http_addr;
        let http_handle = tokio::spawn(async move { run_http_server(http_addr, http_state).await });

        health
            .set_readiness_details(ReadinessDetails {
                identity: "loaded".to_string(),
                transport: "running".to_string(),
                messaging: "ready".to_string(),
                apps: "ready".to_string(),
            })
            .await;
        health.set_ready(true);

        #[cfg(unix)]
        let diag_handle = {
            let admin = self.admin.clone();
            let identity = self.identity.clone();
            let health = health.clone();
            tokio::spawn(async move {
                use tokio::signal::unix::{signal, SignalKind};
                if let Ok(mut stream) = signal(SignalKind::user_defined1()) {
                    while stream.recv().await.is_some() {
                        diagnostics::write_snapshot_log(&admin, &identity, Some(&health), started_at).await;
                    }
                }
            })
        };

        tokio::signal::ctrl_c().await?;
        info!("Shutdown signal received");

        health.set_ready(false);
        health.set_shutting_down(true);

        transport_handle.abort();
        http_handle.abort();
        #[cfg(unix)]
        diag_handle.abort();
        Ok(())
    }
}

async fn ensure_data_directories(base_dir: &Path) -> Result<()> {
    let paths = [
        "identity",
        "messages",
        "messages/attachments",
        "sync",
        "sync/documents",
        "apps/installed",
        "apps/storage",
        "runtime",
        "runtime/cache",
        "logs",
        "logs/apps",
        "run",
        "admin",
    ];
    for path in paths {
        tokio::fs::create_dir_all(base_dir.join(path)).await?;
    }
    Ok(())
}

async fn load_or_create_session_secret(config: &NodeConfig) -> Result<Vec<u8>> {
    if let Some(value) = config.session_secret.as_ref() {
        return hex::decode(value)
            .map_err(|_| PostUrbitError::InvalidInput("session secret"));
    }
    let admin_dir = Path::new(&config.data_dir).join("admin");
    tokio::fs::create_dir_all(&admin_dir).await?;
    let secret_path = admin_dir.join("session_secret");
    if secret_path.exists() {
        let hex_value = tokio::fs::read_to_string(&secret_path).await?;
        return hex::decode(hex_value.trim())
            .map_err(|_| PostUrbitError::InvalidInput("session secret"));
    }
    let hex_value = generate_token_hex(32);
    tokio::fs::write(&secret_path, &hex_value).await?;
    hex::decode(hex_value)
        .map_err(|_| PostUrbitError::InvalidInput("session secret"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_data_directories_creates_layout() {
        let temp = tempfile::tempdir().unwrap();
        ensure_data_directories(temp.path()).await.unwrap();

        let expected = [
            "identity",
            "messages",
            "messages/attachments",
            "sync",
            "sync/documents",
            "apps/installed",
            "apps/storage",
            "runtime",
            "runtime/cache",
            "logs",
            "logs/apps",
            "run",
            "admin",
        ];
        for path in expected {
            assert!(temp.path().join(path).exists(), "missing {path}");
        }
    }
}
