use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone)]
pub struct NetworkAuditEntry {
    pub timestamp: DateTime<Utc>,
    pub app_id: String,
    pub method: String,
    pub url: String,
    pub request_size: u64,
    pub status: Option<u16>,
    pub response_size: Option<u64>,
    pub duration_ms: u64,
    pub outcome: NetworkOutcome,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone)]
pub enum NetworkOutcome {
    Success,
    Error,
    Blocked,
    RateLimited,
}

#[derive(Debug)]
pub struct NetworkAuditLog {
    entries: Mutex<Vec<NetworkAuditEntry>>,
    retention: Duration,
    max_entries: usize,
}

impl NetworkAuditLog {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            retention: Duration::days(30),
            max_entries: 50_000,
        }
    }

    pub fn record(&self, entry: NetworkAuditEntry) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.push(entry);
            self.prune_locked(&mut entries);
        }
    }

    pub fn list(&self, app_id: Option<&str>) -> Vec<NetworkAuditEntry> {
        if let Ok(entries) = self.entries.lock() {
            return entries
                .iter()
                .filter(|entry| app_id.map(|id| id == entry.app_id).unwrap_or(true))
                .cloned()
                .collect();
        }
        Vec::new()
    }

    pub fn list_paginated(
        &self,
        app_id: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Vec<NetworkAuditEntry> {
        if let Ok(entries) = self.entries.lock() {
            return entries
                .iter()
                .filter(|entry| app_id.map(|id| id == entry.app_id).unwrap_or(true))
                .skip(offset)
                .take(limit)
                .cloned()
                .collect();
        }
        Vec::new()
    }

    fn prune_locked(&self, entries: &mut Vec<NetworkAuditEntry>) {
        let cutoff = Utc::now() - self.retention;
        entries.retain(|entry| entry.timestamp >= cutoff);
        if entries.len() > self.max_entries {
            let excess = entries.len() - self.max_entries;
            entries.drain(0..excess);
        }
    }
}
