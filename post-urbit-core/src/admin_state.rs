use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::admin_types::{
    ApiKey, AppSource, BackupListEntry, Contact, Device, InstalledApp, LogEntry, Message,
    MessageFolder, NodeSettings,
};
use crate::error::{PostUrbitError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub created_at: String,
    pub expires_at: String,
    pub last_activity: String,
    pub user_agent: String,
    pub ip_address: String,
    pub device_id: Option<String>,
    pub csrf_token: String,
    pub fresh_auth_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub key: ApiKey,
    pub key_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminData {
    pub contacts: Vec<Contact>,
    pub apps: Vec<InstalledApp>,
    pub settings: NodeSettings,
    #[serde(default)]
    pub app_settings: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub app_sources: HashMap<String, AppSource>,
    #[serde(default)]
    pub repo_cache: HashMap<String, CachedRepository>,
    pub api_keys: Vec<ApiKeyRecord>,
    pub sessions: HashMap<String, SessionRecord>,
    pub backups: Vec<BackupListEntry>,
    pub devices: Vec<Device>,
    pub last_key_rotation: Option<String>,
    #[serde(default)]
    pub logs: Vec<LogEntry>,
    #[serde(default)]
    pub messages: Vec<Message>,
}

impl AdminData {
    pub fn new(settings: NodeSettings) -> Self {
        Self {
            contacts: Vec::new(),
            apps: Vec::new(),
            settings,
            app_settings: HashMap::new(),
            app_sources: HashMap::new(),
            repo_cache: HashMap::new(),
            api_keys: Vec::new(),
            sessions: HashMap::new(),
            backups: Vec::new(),
            devices: Vec::new(),
            last_key_rotation: None,
            logs: Vec::new(),
            messages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRepository {
    pub fetched_at: String,
    pub manifest: serde_json::Value,
}

#[derive(Clone)]
pub struct AdminState {
    pub data_dir: PathBuf,
    pub data: Arc<Mutex<AdminData>>,
}

impl AdminState {
    pub async fn load(data_dir: impl AsRef<Path>, settings: NodeSettings) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let admin_dir = data_dir.join("admin");
        tokio::fs::create_dir_all(&admin_dir).await?;
        let path = admin_dir.join("state.json");

        let data = if path.exists() {
            let raw = tokio::fs::read_to_string(&path).await?;
            serde_json::from_str(&raw)
                .map_err(|_| PostUrbitError::InvalidInput("admin state json"))?
        } else {
            AdminData::new(settings)
        };

        Ok(Self {
            data_dir,
            data: Arc::new(Mutex::new(data)),
        })
    }

    pub async fn persist(&self) -> Result<()> {
        let admin_dir = self.data_dir.join("admin");
        tokio::fs::create_dir_all(&admin_dir).await?;
        let path = admin_dir.join("state.json");
        let tmp = admin_dir.join("state.json.tmp");
        let data = self.data.lock().await;
        let payload = serde_json::to_vec_pretty(&*data)
            .map_err(|_| PostUrbitError::InvalidInput("admin state serialize"))?;
        tokio::fs::write(&tmp, payload).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }

    pub async fn touch_session(&self, session_id: &str) {
        let mut data = self.data.lock().await;
        if let Some(session) = data.sessions.get_mut(session_id) {
            session.last_activity = Utc::now().to_rfc3339();
        }
    }

    pub async fn set_fresh_auth(&self, session_id: &str) {
        let mut data = self.data.lock().await;
        if let Some(session) = data.sessions.get_mut(session_id) {
            session.fresh_auth_at = Some(Utc::now().to_rfc3339());
        }
    }

    pub async fn remove_session(&self, session_id: &str) {
        let mut data = self.data.lock().await;
        data.sessions.remove(session_id);
    }

    pub async fn prune_sessions(&self) {
        let now = Utc::now();
        let mut data = self.data.lock().await;
        data.sessions.retain(|_, session| {
            DateTime::parse_from_rfc3339(&session.expires_at)
                .map(|ts| ts.with_timezone(&Utc) > now)
                .unwrap_or(false)
        });
    }

    pub async fn prune_repo_cache(&self, max_age: Duration) {
        let now = Utc::now();
        let mut data = self.data.lock().await;
        data.repo_cache.retain(|_, cached| {
            DateTime::parse_from_rfc3339(&cached.fetched_at)
                .map(|ts| ts.with_timezone(&Utc) + max_age > now)
                .unwrap_or(false)
        });
    }

    pub async fn append_log(&self, entry: LogEntry, max_entries: usize) {
        let mut data = self.data.lock().await;
        data.logs.push(entry);
        if data.logs.len() > max_entries {
            let excess = data.logs.len() - max_entries;
            data.logs.drain(0..excess);
        }
    }

    // Message operations
    pub async fn add_message(&self, message: Message) {
        let mut data = self.data.lock().await;
        data.messages.push(message);
    }

    pub async fn get_messages_by_folder(
        &self,
        folder: MessageFolder,
        owner_iid: &str,
    ) -> Vec<Message> {
        let data = self.data.lock().await;
        data.messages
            .iter()
            .filter(|m| {
                m.folder == folder
                    && match folder {
                        MessageFolder::Inbox => m.recipient_iid == owner_iid,
                        MessageFolder::Sent => m.sender_iid == owner_iid,
                        MessageFolder::Drafts => m.sender_iid == owner_iid,
                        MessageFolder::Trash => {
                            m.sender_iid == owner_iid || m.recipient_iid == owner_iid
                        }
                    }
            })
            .cloned()
            .collect()
    }

    pub async fn get_message_by_id(&self, message_id: &str) -> Option<Message> {
        let data = self.data.lock().await;
        data.messages.iter().find(|m| m.id == message_id).cloned()
    }

    pub async fn update_message(
        &self,
        message_id: &str,
        read: Option<bool>,
        folder: Option<MessageFolder>,
    ) -> bool {
        let mut data = self.data.lock().await;
        if let Some(msg) = data.messages.iter_mut().find(|m| m.id == message_id) {
            if let Some(r) = read {
                msg.read = r;
            }
            if let Some(f) = folder {
                msg.folder = f;
            }
            true
        } else {
            false
        }
    }

    pub async fn delete_message(&self, message_id: &str) -> bool {
        let mut data = self.data.lock().await;
        let len_before = data.messages.len();
        data.messages.retain(|m| m.id != message_id);
        data.messages.len() < len_before
    }

    pub async fn get_message_stats(&self, owner_iid: &str) -> (u64, u64, u64) {
        let data = self.data.lock().await;
        let inbox_count = data
            .messages
            .iter()
            .filter(|m| m.folder == MessageFolder::Inbox && m.recipient_iid == owner_iid)
            .count() as u64;
        let unread_count = data
            .messages
            .iter()
            .filter(|m| {
                m.folder == MessageFolder::Inbox && m.recipient_iid == owner_iid && !m.read
            })
            .count() as u64;
        let sent_count = data
            .messages
            .iter()
            .filter(|m| m.folder == MessageFolder::Sent && m.sender_iid == owner_iid)
            .count() as u64;
        (inbox_count, unread_count, sent_count)
    }
}
