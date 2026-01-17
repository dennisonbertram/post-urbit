use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSocketMessage {
    pub id: String,
    pub r#type: String,
    pub timestamp: String,
    pub data: Value,
}

#[derive(Clone)]
pub struct EventBus {
    next_id: Arc<AtomicU64>,
    buffer: Arc<Mutex<VecDeque<WebSocketMessage>>>,
    sender: broadcast::Sender<WebSocketMessage>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self {
            next_id: Arc::new(AtomicU64::new(1)),
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(1000))),
            sender,
        }
    }

    pub async fn emit(&self, event_type: &str, data: Value) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = WebSocketMessage {
            id: id.to_string(),
            r#type: event_type.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            data,
        };
        {
            let mut buffer = self.buffer.lock().await;
            buffer.push_back(msg.clone());
            while buffer.len() > 1000 {
                buffer.pop_front();
            }
        }
        let _ = self.sender.send(msg);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WebSocketMessage> {
        self.sender.subscribe()
    }

    pub async fn replay_since(&self, last_id: Option<u64>) -> Vec<WebSocketMessage> {
        let buffer = self.buffer.lock().await;
        match last_id {
            Some(id) => buffer
                .iter()
                .filter(|msg| msg.id.parse::<u64>().unwrap_or(0) > id)
                .cloned()
                .collect(),
            None => buffer.iter().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn event_bus_replay_filters() {
        let bus = EventBus::new();
        bus.emit("status_change", json!({"status": "ok"})).await;
        bus.emit("app_installed", json!({"app_id": "a"})).await;

        let replay = bus.replay_since(Some(1)).await;
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].r#type, "app_installed");
    }
}
