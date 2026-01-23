use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Timestamp = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub password: String,
    pub remember_device: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginResponse {
    pub session: Session,
    pub csrf_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Session {
    pub id: String,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub last_activity: Timestamp,
    pub user_agent: String,
    pub ip_address: String,
    pub device_id: Option<String>,
    pub requires_fresh_auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub permissions: Vec<Permission>,
    pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub last_used: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub permissions: Vec<Permission>,
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateApiKeyResponse {
    pub key: ApiKey,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Permission {
    #[serde(rename = "read:identity")]
    ReadIdentity,
    #[serde(rename = "write:identity")]
    WriteIdentity,
    #[serde(rename = "read:contacts")]
    ReadContacts,
    #[serde(rename = "write:contacts")]
    WriteContacts,
    #[serde(rename = "read:messages")]
    ReadMessages,
    #[serde(rename = "send:messages")]
    SendMessages,
    #[serde(rename = "read:apps")]
    ReadApps,
    #[serde(rename = "manage:apps")]
    ManageApps,
    #[serde(rename = "read:settings")]
    ReadSettings,
    #[serde(rename = "write:settings")]
    WriteSettings,
    #[serde(rename = "admin:full")]
    AdminFull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityInfo {
    pub iid: String,
    pub genesis_key_fingerprint: String,
    pub current_signing_key_fingerprint: String,
    pub current_encryption_key_fingerprint: String,
    pub created_at: Timestamp,
    pub last_key_rotation: Option<Timestamp>,
    pub recovery_method: String,
    pub endpoints: Vec<crate::identity::Endpoint>,
    pub profile: Option<PublicProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PublicProfile {
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub bio: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Device {
    pub did: String,
    pub name: String,
    pub created_at: Timestamp,
    pub last_seen: Timestamp,
    pub is_current: bool,
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KeyRotationResult {
    pub success: bool,
    pub new_key_fingerprint: String,
    pub previous_key_fingerprint: String,
    pub rotated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceAddResult {
    pub did: String,
    pub name: String,
    pub activation_code: String,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Contact {
    pub iid: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub trust_level: TrustLevel,
    pub is_blocked: bool,
    pub is_online: bool,
    pub last_seen: Option<Timestamp>,
    pub added_at: Timestamp,
    pub added_by: String,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub shared_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Unknown,
    Unverified,
    Verified,
    Trusted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContactUpdate {
    pub display_name: Option<String>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub trust_level: Option<TrustLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddContactRequest {
    pub iid: String,
    pub display_name: Option<String>,
    pub trust_level: Option<TrustLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author_iid: String,
    pub author_name: Option<String>,
    pub description: String,
    pub icon: Option<String>,
    pub installed_at: Timestamp,
    pub last_opened: Option<Timestamp>,
    pub update_available: Option<String>,
    pub status: AppStatus,
    pub permissions: AppPermissions,
    pub storage_used: u64,
    pub storage_quota: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    Installed,
    Running,
    Disabled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppPermissions {
    pub granted: Vec<String>,
    pub denied: Vec<String>,
    pub pending: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppSource {
    pub r#type: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InstallRequest {
    pub source: AppSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InstallResult {
    pub app: InstalledApp,
    pub permissions_requested: Vec<String>,
    pub permissions_granted: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateResult {
    pub app: InstalledApp,
    pub previous_version: String,
    pub new_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PermissionPatch {
    pub grant: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
    pub reset: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeSettings {
    pub network: NetworkSettings,
    pub admin: AdminSettings,
    pub apps: AppSettings,
    pub privacy: PrivacySettings,
    pub storage: StorageSettings,
    pub notifications: NotificationSettings,
    #[serde(default)]
    pub logging: LoggingSettings,
    #[serde(default)]
    pub metrics: MetricsSettings,
    #[serde(default)]
    pub health: HealthSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NetworkSettings {
    pub listen_addr: String,
    pub admin_listen_addr: String,
    pub enable_upnp: bool,
    pub external_addr: Option<String>,
    pub relay_servers: Vec<String>,
    pub bandwidth_limit_mbps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdminSettings {
    pub enabled: bool,
    pub require_tls: bool,
    pub session_timeout_hours: u32,
    pub ip_allowlist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppSettings {
    pub auto_update: bool,
    pub allow_sideload: bool,
    pub default_storage_quota: String,
    pub trusted_repositories: Vec<TrustedRepository>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TrustedRepository {
    pub id: String,
    pub operator_iid: String,
    pub operator_key_fingerprint: Option<String>,
    pub url: String,
    pub trust_level: String,
    pub auto_update: bool,
    pub added_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PrivacySettings {
    pub publish_identity_hours: u32,
    pub show_online_status: bool,
    pub send_read_receipts: bool,
    pub share_analytics: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StorageSettings {
    pub data_dir: String,
    pub log_dir: String,
    pub backup_enabled: bool,
    pub backup_schedule: Option<String>,
    pub backup_retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationSettings {
    pub enabled: bool,
    pub sound_enabled: bool,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoggingSettings {
    pub redact_iids: bool,
    pub redact_ips: bool,
    pub redact_message_content: bool,
}

impl Default for LoggingSettings {
    fn default() -> Self {
        Self {
            redact_iids: true,
            redact_ips: true,
            redact_message_content: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MetricsSettings {
    pub enabled: bool,
    pub require_auth: bool,
    pub auth_token_hash: Option<String>,
}

impl Default for MetricsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            require_auth: false,
            auth_token_hash: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HealthSettings {
    pub disk_free_min_bytes: u64,
    pub memory_max_percent: u8,
    pub connection_queue_max: u32,
    pub message_queue_max: u32,
}

impl Default for HealthSettings {
    fn default() -> Self {
        Self {
            disk_free_min_bytes: 100 * 1024 * 1024,
            memory_max_percent: 90,
            connection_queue_max: 1000,
            message_queue_max: 10000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeStatus {
    pub version: String,
    pub uptime_seconds: u64,
    pub status: String,
    pub identity: IdentityStatus,
    pub network: NetworkStatus,
    pub storage: StorageStatus,
    pub apps: AppsStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityStatus {
    pub iid: String,
    pub last_published: Option<Timestamp>,
    pub device_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NetworkStatus {
    pub connections_active: u32,
    pub connections_direct: u32,
    pub connections_relay: u32,
    pub relays_connected: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub external_addr_detected: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StorageStatus {
    pub data_used_bytes: u64,
    pub data_free_bytes: u64,
    pub messages_count: u64,
    pub documents_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppsStatus {
    pub installed: u32,
    pub running: u32,
    pub total_storage_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogEntry {
    pub timestamp: Timestamp,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogsResponse {
    pub entries: Vec<LogEntry>,
    pub cursor: Option<String>,
    #[serde(rename = "hasMore", alias = "has_more")]
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogOptions {
    pub level: Option<String>,
    pub target: Option<String>,
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BackupResult {
    pub id: String,
    pub created_at: Timestamp,
    pub size: u64,
    pub path: String,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RestoreResult {
    pub success: bool,
    pub restored_at: Timestamp,
    pub identity: String,
    pub contacts_restored: u64,
    pub messages_restored: u64,
    pub apps_restored: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BackupListEntry {
    pub id: String,
    pub created_at: Timestamp,
    pub size: u64,
    pub path: String,
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiErrorBody {
    pub error: ApiError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiErrorCode {
    InvalidRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    RateLimited,
    PayloadTooLarge,
    ValidationError,
    CsrfInvalid,
    FreshAuthRequired,
    InternalError,
    ServiceUnavailable,
    Timeout,
}

pub fn api_error(code: ApiErrorCode, message: &str) -> ApiErrorBody {
    ApiErrorBody {
        error: ApiError {
            code,
            message: message.to_string(),
            details: None,
        },
    }
}

// ============================================================================
// Messaging Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageFolder {
    Inbox,
    Sent,
    Drafts,
    Trash,
}

impl Default for MessageFolder {
    fn default() -> Self {
        MessageFolder::Inbox
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Message {
    pub id: String,
    pub sender_iid: String,
    pub recipient_iid: String,
    pub subject: String,
    pub body: String,
    pub sent_at: Timestamp,
    pub read: bool,
    pub folder: MessageFolder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SendMessageRequest {
    pub recipient_iid: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SendMessageResponse {
    pub message_id: String,
    pub sent_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageUpdate {
    pub read: Option<bool>,
    pub folder: Option<MessageFolder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageStats {
    pub inbox_count: u64,
    pub unread_count: u64,
    pub sent_count: u64,
}
