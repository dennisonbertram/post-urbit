use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;
use serde_json::json;
use tar::Builder;
use uuid::Uuid;

use crate::admin_state::AdminState;
use crate::admin_types::{LogEntry, NodeStatus};
use crate::error::{PostUrbitError, Result};
use crate::health::HealthState;
use crate::identity::IdentityManager;
use crate::metrics;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiagnosticsSnapshot {
    pub version: String,
    pub captured_at: String,
    pub uptime_seconds: u64,
    pub readiness: Option<crate::health::ReadinessDetails>,
    pub status: NodeStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub data_dir: String,
    pub disk_used_bytes: u64,
    pub disk_free_bytes: u64,
}

pub async fn collect_snapshot(
    admin: &AdminState,
    identity: &IdentityManager,
    health: Option<&HealthState>,
    started_at: Instant,
) -> Result<DiagnosticsSnapshot> {
    let readiness = if let Some(health) = health {
        Some(health.readiness_details().await)
    } else {
        None
    };
    let status = build_status(admin, identity, health, started_at).await;
    Ok(DiagnosticsSnapshot {
        version: env!("CARGO_PKG_VERSION").to_string(),
        captured_at: Utc::now().to_rfc3339(),
        uptime_seconds: started_at.elapsed().as_secs(),
        readiness,
        status,
    })
}

pub async fn write_bundle(
    admin: &AdminState,
    identity: &IdentityManager,
    health: Option<&HealthState>,
    started_at: Instant,
    output: &Path,
) -> Result<()> {
    let bundle_dir = temp_bundle_dir()?;
    let snapshot = collect_snapshot(admin, identity, health, started_at).await?;
    write_json(&bundle_dir.join("snapshot.json"), &snapshot)?;
    write_json(&bundle_dir.join("settings.json"), &settings_snapshot(admin).await)?;
    write_json(&bundle_dir.join("connections.json"), &connections_snapshot())?;
    write_json(&bundle_dir.join("apps.json"), &apps_snapshot(admin).await)?;
    write_json(&bundle_dir.join("system.json"), &system_snapshot(admin).await)?;
    write_json(&bundle_dir.join("health.json"), &health_snapshot(admin, identity, health, started_at).await)?;
    write_json(&bundle_dir.join("logs.json"), &logs_snapshot(admin).await)?;

    let metrics_text = metrics::render_metrics(admin, identity, started_at).await;
    std::fs::write(bundle_dir.join("metrics.prom"), metrics_text)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;

    archive_bundle(&bundle_dir, output)?;
    std::fs::remove_dir_all(&bundle_dir)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

pub async fn write_snapshot_log(
    admin: &AdminState,
    identity: &IdentityManager,
    health: Option<&HealthState>,
    started_at: Instant,
) {
    let snapshot = match collect_snapshot(admin, identity, health, started_at).await {
        Ok(value) => value,
        Err(_) => return,
    };
    let payload = json!({
        "event": "diagnostics_snapshot",
        "snapshot": snapshot,
    });
    let _ = crate::logging::log_entry(
        admin,
        None,
        "info",
        "postnode::diagnostics",
        "diagnostics snapshot",
        Some(payload),
    )
    .await;
}

fn temp_bundle_dir() -> Result<PathBuf> {
    let id = Uuid::new_v4().to_string();
    let dir = std::env::temp_dir().join(format!("postnode-diag-{id}"));
    std::fs::create_dir_all(&dir).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(dir)
}

fn archive_bundle(bundle_dir: &Path, output: &Path) -> Result<()> {
    let file = std::fs::File::create(output).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    builder
        .append_dir_all(".", bundle_dir)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    let encoder = builder.into_inner().map_err(|err| PostUrbitError::Io(err.to_string()))?;
    encoder.finish().map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

fn write_json(path: &Path, payload: &impl Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(payload)
        .map_err(|_| PostUrbitError::InvalidInput("diagnostic json"))?;
    std::fs::write(path, data).map_err(|err| PostUrbitError::Io(err.to_string()))?;
    Ok(())
}

async fn settings_snapshot(admin: &AdminState) -> serde_json::Value {
    let data = admin.data.lock().await;
    json!({
        "settings": data.settings,
    })
}

async fn logs_snapshot(admin: &AdminState) -> Vec<LogEntry> {
    let data = admin.data.lock().await;
    data.logs.clone()
}

async fn apps_snapshot(admin: &AdminState) -> serde_json::Value {
    let data = admin.data.lock().await;
    json!({
        "installed": data.apps,
    })
}

fn connections_snapshot() -> serde_json::Value {
    json!({
        "connections_active": 0,
        "relays_connected": 0,
    })
}

async fn system_snapshot(admin: &AdminState) -> SystemInfo {
    let disk_used = directory_size(&admin.data_dir);
    let disk_free = fs2::available_space(&admin.data_dir).unwrap_or(0);
    SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        data_dir: admin.data_dir.to_string_lossy().to_string(),
        disk_used_bytes: disk_used,
        disk_free_bytes: disk_free,
    }
}

async fn health_snapshot(
    admin: &AdminState,
    identity: &IdentityManager,
    health: Option<&HealthState>,
    started_at: Instant,
) -> serde_json::Value {
    let status = build_status(admin, identity, health, started_at).await;
    json!({
        "status": status.status,
        "uptime_seconds": status.uptime_seconds,
        "checks": {
            "identity": {
                "status": "healthy",
                "iid": status.identity.iid,
                "last_published": status.identity.last_published,
            },
            "transport": {
                "status": "healthy",
                "connections": status.network.connections_active,
                "relays_connected": status.network.relays_connected,
            },
            "messaging": {
                "status": "healthy",
                "queue_depth": 0,
                "sessions_active": 0,
            },
            "storage": {
                "status": "healthy",
                "disk_used_bytes": status.storage.data_used_bytes,
                "disk_free_bytes": status.storage.data_free_bytes,
            },
            "apps": {
                "status": "healthy",
                "installed": status.apps.installed,
                "running": status.apps.running,
            }
        }
    })
}

async fn build_status(
    admin: &AdminState,
    identity: &IdentityManager,
    health: Option<&HealthState>,
    started_at: Instant,
) -> NodeStatus {
    let uptime = started_at.elapsed().as_secs();
    let data = admin.data.lock().await;
    let disk_free_bytes = fs2::available_space(&admin.data_dir).unwrap_or(0);
    let disk_ok = disk_free_bytes >= data.settings.health.disk_free_min_bytes;
    let mut status = match health {
        Some(health) if health.is_shutting_down() => "unhealthy",
        Some(health) if health.is_ready() => "healthy",
        Some(_) => "degraded",
        None => "unknown",
    };
    if !disk_ok {
        status = "unhealthy";
    }
    NodeStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
        status: status.to_string(),
        identity: crate::admin_types::IdentityStatus {
            iid: identity.iid().await,
            last_published: None,
            device_count: data.devices.len() as u32,
        },
        network: crate::admin_types::NetworkStatus {
            connections_active: 0,
            connections_direct: 0,
            connections_relay: 0,
            relays_connected: 0,
            bytes_sent: 0,
            bytes_received: 0,
            external_addr_detected: None,
        },
        storage: crate::admin_types::StorageStatus {
            data_used_bytes: directory_size(&admin.data_dir),
            data_free_bytes: disk_free_bytes,
            messages_count: 0,
            documents_count: 0,
        },
        apps: crate::admin_types::AppsStatus {
            installed: data.apps.len() as u32,
            running: 0,
            total_storage_used: data.apps.iter().map(|app| app.storage_used).sum(),
        },
    }
}

fn directory_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total = total.saturating_add(meta.len());
                } else if meta.is_dir() {
                    total = total.saturating_add(directory_size(&entry.path()));
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::HealthState;
    use crate::node_config::default_node_settings;

    #[tokio::test]
    async fn diagnostics_bundle_writes_tarball() {
        let temp = tempfile::tempdir().unwrap();
        let settings = default_node_settings(
            temp.path().to_str().unwrap(),
            temp.path().join("logs").to_str().unwrap(),
        );
        let admin = AdminState::load(temp.path(), settings).await.unwrap();
        let identity = IdentityManager::new(temp.path().to_str().unwrap()).await.unwrap();
        let health = HealthState::new();

        let output = temp.path().join("diag.tar.gz");
        write_bundle(&admin, &identity, Some(&health), Instant::now(), &output)
            .await
            .unwrap();
        assert!(output.exists());
    }
}
