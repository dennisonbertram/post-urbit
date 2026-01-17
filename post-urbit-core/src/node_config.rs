use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::admin_types::NodeSettings;
use crate::error::{PostUrbitError, Result};
use crate::node::NodeConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DaemonConfig {
    pub port: u16,
    pub data_dir: String,
    pub metrics_enabled: bool,
    pub admin_password_hash: Option<String>,
    pub admin_token_hash: Option<String>,
    pub session_secret: Option<String>,
    pub session_timeout_hours: u32,
    pub http_addr: Option<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            port: 4433,
            data_dir: "./data".to_string(),
            metrics_enabled: true,
            admin_password_hash: None,
            admin_token_hash: None,
            session_secret: None,
            session_timeout_hours: 24,
            http_addr: None,
        }
    }
}

pub fn load_config(path: Option<&str>, overrides: HashMap<String, String>) -> Result<DaemonConfig> {
    let mut builder = config::Config::builder();
    builder = builder.add_source(config::Config::try_from(&DaemonConfig::default())
        .map_err(|_| PostUrbitError::InvalidInput("config defaults"))?);

    if let Some(path) = path {
        builder = builder.add_source(config::File::with_name(path).required(false));
    }

    builder = builder.add_source(
        config::Environment::with_prefix("POST_URBIT")
            .separator("__")
            .try_parsing(true),
    );

    for (key, value) in overrides {
        builder = builder.set_override(key, value)
            .map_err(|_| PostUrbitError::InvalidInput("config override"))?;
    }

    let settings = builder
        .build()
        .map_err(|_| PostUrbitError::InvalidInput("config build"))?;
    settings
        .try_deserialize()
        .map_err(|_| PostUrbitError::InvalidInput("config deserialize"))
}

pub fn build_node_config(config: DaemonConfig, bootstrap_peers: Vec<String>) -> Result<NodeConfig> {
    let http_addr = config
        .http_addr
        .unwrap_or_else(|| "127.0.0.1:8080".to_string())
        .parse()
        .map_err(|_| PostUrbitError::InvalidInput("http addr"))?;
    Ok(NodeConfig {
        port: config.port,
        data_dir: config.data_dir,
        bootstrap_peers,
        http_addr,
        metrics_enabled: config.metrics_enabled,
        admin_password_hash: config.admin_password_hash,
        admin_token_hash: config.admin_token_hash,
        session_secret: config.session_secret,
        session_timeout_hours: config.session_timeout_hours,
    })
}

pub fn default_node_settings(data_dir: &str, log_dir: &str) -> NodeSettings {
    NodeSettings {
        network: crate::admin_types::NetworkSettings {
            listen_addr: "0.0.0.0:4433".to_string(),
            admin_listen_addr: "127.0.0.1:8080".to_string(),
            enable_upnp: true,
            external_addr: None,
            relay_servers: Vec::new(),
            bandwidth_limit_mbps: None,
        },
        admin: crate::admin_types::AdminSettings {
            enabled: true,
            require_tls: false,
            session_timeout_hours: 24,
            ip_allowlist: Vec::new(),
        },
        apps: crate::admin_types::AppSettings {
            auto_update: true,
            allow_sideload: true,
            default_storage_quota: "100MB".to_string(),
            trusted_repositories: Vec::new(),
        },
        privacy: crate::admin_types::PrivacySettings {
            publish_identity_hours: 24,
            show_online_status: true,
            send_read_receipts: true,
            share_analytics: false,
        },
        storage: crate::admin_types::StorageSettings {
            data_dir: data_dir.to_string(),
            log_dir: log_dir.to_string(),
            backup_enabled: true,
            backup_schedule: None,
            backup_retention_days: 30,
        },
        notifications: crate::admin_types::NotificationSettings {
            enabled: true,
            sound_enabled: true,
            quiet_hours_start: None,
            quiet_hours_end: None,
        },
        logging: crate::admin_types::LoggingSettings::default(),
        metrics: crate::admin_types::MetricsSettings::default(),
        health: crate::admin_types::HealthSettings::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn config_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"port": 1111, "data_dir": "./file", "metrics_enabled": false}"#,
        )
        .unwrap();

        env::set_var("POST_URBIT__PORT", "2222");

        let mut overrides = HashMap::new();
        overrides.insert("port".to_string(), "3333".to_string());
        overrides.insert("data_dir".to_string(), "./flags".to_string());

        let settings = load_config(path.to_str(), overrides).unwrap();
        assert_eq!(settings.port, 3333);
        assert_eq!(settings.data_dir, "./flags");
        assert_eq!(settings.metrics_enabled, false);

        env::remove_var("POST_URBIT__PORT");
    }

    #[test]
    fn build_node_config_parses_http_addr() {
        let config = DaemonConfig {
            port: 4444,
            data_dir: "./data".to_string(),
            metrics_enabled: true,
            admin_password_hash: None,
            admin_token_hash: None,
            session_secret: None,
            session_timeout_hours: 12,
            http_addr: Some("127.0.0.1:9999".to_string()),
        };
        let node = build_node_config(config, Vec::new()).unwrap();
        assert_eq!(node.port, 4444);
        assert_eq!(node.http_addr.to_string(), "127.0.0.1:9999");
        assert_eq!(node.session_timeout_hours, 12);
    }
}
