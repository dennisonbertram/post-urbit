use std::path::Path;
use std::sync::Arc;

use tracing::info;

use crate::dht::MemoryDht;
use crate::admin_auth::{AuthConfig, generate_token_hex};
use crate::admin_state::AdminState;
use crate::identity::{publish_genesis, publish_identity, IdentityManager};
use crate::node_config::default_node_settings;
use crate::node_http::{run_http_server, HttpServerConfig, HttpServerState};
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
}

pub struct PostUrbitNode {
    config: NodeConfig,
    identity: Arc<IdentityManager>,
    transport: Arc<QuicTransport>,
    dht: Arc<MemoryDht>,
    admin: AdminState,
    apps_dir: std::path::PathBuf,
}

impl PostUrbitNode {
    pub async fn new(config: NodeConfig) -> Result<Self> {
        info!("Initializing Post-Urbit node...");

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

        Ok(Self {
            config,
            identity,
            transport,
            dht,
            admin,
            apps_dir,
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
        let http_state = HttpServerState {
            admin: self.admin.clone(),
            auth,
            identity: self.identity.clone(),
            dht: self.dht.clone(),
            started_at: std::time::Instant::now(),
            config: HttpServerConfig {
                metrics_enabled: self.config.metrics_enabled,
                max_request_body_bytes: 100 * 1024 * 1024,
                session_cookie_secure: false,
            },
            apps_dir: self.apps_dir.clone(),
        };
        let http_addr = self.config.http_addr;
        let http_handle = tokio::spawn(async move { run_http_server(http_addr, http_state).await });

        tokio::signal::ctrl_c().await?;
        info!("Shutdown signal received");

        transport_handle.abort();
        http_handle.abort();
        Ok(())
    }
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
