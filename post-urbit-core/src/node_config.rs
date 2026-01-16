use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{PostUrbitError, Result};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeSettings {
    pub port: u16,
    pub data_dir: String,
    pub metrics_enabled: bool,
    pub admin_token: Option<String>,
}

impl Default for NodeSettings {
    fn default() -> Self {
        Self {
            port: 4433,
            data_dir: "./data".to_string(),
            metrics_enabled: true,
            admin_token: None,
        }
    }
}

pub fn load_config(path: Option<&str>, overrides: HashMap<String, String>) -> Result<NodeSettings> {
    let mut builder = config::Config::builder();
    builder = builder.add_source(config::Config::try_from(&NodeSettings::default())
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
}
