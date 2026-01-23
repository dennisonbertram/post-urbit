// API Response Types for Post-Urbit HTTP API

export interface HealthCheck {
  status: 'healthy' | 'unhealthy';
  version: string;
  uptime_seconds: number;
  checks: {
    identity: {
      status: 'healthy' | 'unhealthy';
      iid: string;
      last_published?: string;
    };
    transport: {
      status: 'healthy' | 'unhealthy';
      connections: number;
      relays_connected: number;
    };
    messaging: {
      status: 'healthy' | 'unhealthy';
      queue_depth: number;
      sessions_active: number;
    };
    storage: {
      status: 'healthy' | 'unhealthy';
      disk_used_bytes: number;
      disk_free_bytes: number;
    };
    apps: {
      status: 'healthy' | 'unhealthy';
      installed: number;
      running: number;
    };
  };
}

export interface NodeStatus {
  version: string;
  uptime_seconds: number;
  status: 'healthy' | 'unhealthy';
  identity: {
    iid: string;
    last_published?: string;
    device_count: number;
  };
  network: {
    connections_active: number;
    connections_direct: number;
    connections_relay: number;
    relays_connected: number;
    bytes_sent: number;
    bytes_received: number;
    external_addr_detected?: string;
  };
  storage: {
    data_used_bytes: number;
    data_free_bytes: number;
    messages_count: number;
    documents_count: number;
  };
  apps: {
    installed: number;
    running: number;
    total_storage_used: number;
  };
}

export interface Identity {
  iid: string;
  genesis_key_fingerprint: string;
  current_signing_key_fingerprint: string;
  current_encryption_key_fingerprint: string;
  created_at: string;
  last_key_rotation?: string;
  recovery_method: string;
  endpoints: Array<{
    type: string;
    url: string;
    priority: number;
  }>;
  profile: {
    display_name: string;
    avatar?: string;
    bio?: string;
  };
}

export type AppStatus = 'installed' | 'running' | 'disabled' | 'error';

export interface App {
  id: string;
  name: string;
  version: string;
  author_iid: string;
  author_name: string;
  description: string;
  icon?: string;
  installed_at: string;
  last_opened?: string;
  update_available?: string;
  status: AppStatus;
  permissions: {
    granted: string[];
    denied: string[];
    pending: string[];
  };
  storage_used: number;
  storage_quota: number;
}

export interface AppRuntime {
  app_id: string;
  installed: boolean;
  running: boolean;
  version: string;
  capabilities: string[];
  secrets_configured: string[];
  secrets_missing: string[];
}

export interface ApiError {
  error: {
    code: string;
    message: string;
    details?: unknown;
  };
}

export interface LoginRequest {
  password: string;
  remember_device?: boolean;
}

export interface LoginResponse {
  session: {
    id: string;
    created_at: string;
    expires_at: string;
    last_activity: string;
    user_agent?: string;
    ip_address?: string;
    device_id?: string;
    requires_fresh_auth: boolean;
  };
  csrf_token: string;
}

export type TrustLevel = 'unknown' | 'unverified' | 'verified' | 'trusted';

export interface Contact {
  iid: string;
  display_name: string;
  avatar?: string;
  trust_level: TrustLevel;
  is_blocked: boolean;
  is_online: boolean;
  last_seen?: string;
  added_at: string;
  added_by: string;
  notes?: string;
  tags: string[];
  shared_groups: string[];
}

export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
}

// App secrets from /admin/v1/apps/{id}/secrets
export interface AppSecret {
  name: string;
  description: string;
  required: boolean;
  configured: boolean;
  domains?: string[];
  inject_method?: string;
}

export interface AppSecretsResponse {
  secrets: AppSecret[];
}

// App action responses
export interface AppActionResponse {
  stopped?: boolean;
  restarted?: boolean;
}

export interface AppUpdateResponse {
  app: App;
  previous_version: string;
  new_permissions?: string[];
}

export interface AppPermissions {
  granted: string[];
  denied: string[];
  pending: string[];
}

export interface AppPermissionsUpdate {
  grant?: string[];
  deny?: string[];
  reset?: string[];
}

// Log entry from /admin/v1/logs
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  timestamp: string; // ISO-8601
  level: LogLevel;
  target: string;
  message: string;
  fields?: Record<string, unknown>;
}

export interface LogsResponse {
  entries: LogEntry[];
  cursor: string | null;
  has_more: boolean;
}

export interface LogsQueryParams {
  limit?: number;
  cursor?: string;
  level?: LogLevel;
  target?: string;
  search?: string;
  since?: string;
  until?: string;
}

// ============================================================================
// Messaging Types
// ============================================================================

export type MessageFolder = 'inbox' | 'sent' | 'drafts' | 'trash';

export interface Message {
  id: string;
  sender_iid: string;
  recipient_iid: string;
  subject: string;
  body: string;
  sent_at: string;
  read: boolean;
  folder: MessageFolder;
}

export interface SendMessageRequest {
  recipient_iid: string;
  subject: string;
  body: string;
}

export interface SendMessageResponse {
  message_id: string;
  sent_at: string;
}

export interface MessageUpdate {
  read?: boolean;
  folder?: MessageFolder;
}

export interface MessageStats {
  inbox_count: number;
  unread_count: number;
  sent_count: number;
}
