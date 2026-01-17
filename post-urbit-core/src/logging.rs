use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::{Value, json};

use crate::admin_state::AdminState;
use crate::admin_types::LogEntry;
use crate::event_bus::EventBus;

const MAX_LOG_BYTES: u64 = 100 * 1024 * 1024;
const MAX_LOG_FILES: usize = 10;

fn redact_token(value: &str) -> String {
    if value.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...", &value[..8])
}

fn redact_ip(value: &str) -> String {
    if value == "unknown" || value.is_empty() {
        return value.to_string();
    }
    match value.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            let octets = ip.octets();
            format!("{}.{}.x.x", octets[0], octets[1])
        }
        Ok(std::net::IpAddr::V6(_)) => "redacted".to_string(),
        Err(_) => "redacted".to_string(),
    }
}

fn redact_value(key: Option<&str>, value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = serde_json::Map::new();
            for (k, v) in map {
                redacted.insert(k.clone(), redact_value(Some(&k), v));
            }
            Value::Object(redacted)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(|v| redact_value(key, v)).collect()),
        Value::String(value) => {
            if let Some(key) = key {
                let key = key.to_ascii_lowercase();
                if key.contains("iid") || key.contains("message_id") || key.contains("messageid") {
                    return Value::String(redact_token(&value));
                }
                if key.contains("ip") {
                    return Value::String(redact_ip(&value));
                }
            }
            Value::String(value)
        }
        other => other,
    }
}

fn redact_log_fields(fields: Option<Value>) -> Option<Value> {
    fields.map(|value| redact_value(None, value))
}

async fn ensure_log_dir(path: &Path) {
    let _ = tokio::fs::create_dir_all(path).await;
}

async fn maybe_rotate_log(log_dir: &Path, file_name: &str) {
    let path = log_dir.join(file_name);
    let size = tokio::fs::metadata(&path).await.map(|meta| meta.len()).unwrap_or(0);
    if size <= MAX_LOG_BYTES {
        return;
    }

    for idx in (1..=MAX_LOG_FILES).rev() {
        let from = if idx == 1 {
            path.clone()
        } else {
            log_dir.join(format!("{file_name}.{idx_minus}", idx_minus = idx - 1))
        };
        if !from.exists() {
            continue;
        }
        let to = log_dir.join(format!("{file_name}.{idx}"));
        let _ = tokio::fs::rename(&from, &to).await;
    }
    let overflow = log_dir.join(format!("{file_name}.{}", MAX_LOG_FILES + 1));
    if overflow.exists() {
        let _ = tokio::fs::remove_file(overflow).await;
    }
}

async fn write_log_line(log_dir: &Path, file_name: &str, payload: &Value) {
    ensure_log_dir(log_dir).await;
    maybe_rotate_log(log_dir, file_name).await;
    let path = log_dir.join(file_name);
    if let Ok(line) = serde_json::to_string(payload) {
        let line = format!("{line}\n");
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
        {
            use tokio::io::AsyncWriteExt;
            let _ = file.write_all(line.as_bytes()).await;
        }
    }
}

async fn log_dirs(admin: &AdminState) -> (PathBuf, PathBuf) {
    let data = admin.data.lock().await;
    let base = PathBuf::from(&data.settings.storage.log_dir);
    let audit = base.join("audit");
    (base, audit)
}

pub async fn log_entry(
    admin: &AdminState,
    event_bus: Option<&EventBus>,
    level: &str,
    target: &str,
    message: &str,
    fields: Option<Value>,
) {
    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        level: level.to_string(),
        target: target.to_string(),
        message: message.to_string(),
        fields: redact_log_fields(fields),
    };
    admin.append_log(entry.clone(), 1000).await;
    if let Some(bus) = event_bus {
        let _ = bus
            .emit(
                "log_entry",
                json!({
                    "timestamp": entry.timestamp,
                    "level": entry.level,
                    "target": entry.target,
                    "message": entry.message,
                    "fields": entry.fields,
                }),
            )
            .await;
    }

    let (log_dir, audit_dir) = log_dirs(admin).await;
    let payload = json!({
        "timestamp": entry.timestamp,
        "level": entry.level,
        "target": entry.target,
        "message": entry.message,
        "fields": entry.fields,
    });
    write_log_line(&log_dir, "postnode.log", &payload).await;
    if entry.target == "postnode::audit" {
        write_log_line(&audit_dir, "audit.log", &payload).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_config::default_node_settings;

    #[tokio::test]
    async fn audit_logs_write_to_audit_file() {
        let temp = tempfile::tempdir().unwrap();
        let settings = default_node_settings(
            temp.path().to_str().unwrap(),
            temp.path().join("logs").to_str().unwrap(),
        );
        let admin = AdminState::load(temp.path(), settings).await.unwrap();

        log_entry(
            &admin,
            None,
            "info",
            "postnode::audit",
            "admin_login",
            Some(json!({"actor_ip": "127.0.0.1"})),
        )
        .await;

        let audit_path = temp.path().join("logs").join("audit").join("audit.log");
        assert!(audit_path.exists());
    }
}
