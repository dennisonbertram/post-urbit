use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ReadinessDetails {
    pub identity: String,
    pub transport: String,
    pub messaging: String,
    pub apps: String,
}

impl Default for ReadinessDetails {
    fn default() -> Self {
        Self {
            identity: "loaded".to_string(),
            transport: "starting".to_string(),
            messaging: "waiting".to_string(),
            apps: "waiting".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct HealthState {
    ready: Arc<AtomicBool>,
    shutting_down: Arc<AtomicBool>,
    details: Arc<tokio::sync::RwLock<ReadinessDetails>>,
}

impl HealthState {
    pub fn new() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
            shutting_down: Arc::new(AtomicBool::new(false)),
            details: Arc::new(tokio::sync::RwLock::new(ReadinessDetails::default())),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::SeqCst);
    }

    pub fn set_shutting_down(&self, shutting_down: bool) {
        self.shutting_down.store(shutting_down, Ordering::SeqCst);
    }

    pub async fn readiness_details(&self) -> ReadinessDetails {
        self.details.read().await.clone()
    }

    pub async fn set_readiness_details(&self, details: ReadinessDetails) {
        *self.details.write().await = details;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_starts_not_ready() {
        let state = HealthState::new();
        assert!(!state.is_ready());
        assert!(!state.is_shutting_down());
    }
}
