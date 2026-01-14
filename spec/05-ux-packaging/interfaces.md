# UX & Packaging Interfaces

## Overview

TypeScript interface definitions for the UX & Packaging layer. These define the Admin API contract between the node daemon and UI clients.

## Naming Convention

| Context | Convention | Example |
|---------|------------|---------|
| **On-wire JSON** | snake_case | `admin_token`, `app_id`, `created_at` |
| **TypeScript interfaces** | camelCase | `adminToken`, `appId`, `createdAt` |

When serializing to JSON, convert camelCase to snake_case.

## Admin API Types

### Authentication

```typescript
// Login request/response
interface LoginRequest {
  password: string;
  rememberDevice?: boolean;
}

interface LoginResponse {
  session: Session;
  token: string;  // Bearer token for API calls
}

interface Session {
  id: string;
  createdAt: Timestamp;
  expiresAt: Timestamp;
  lastActivity: Timestamp;
  userAgent: string;
  ipAddress: string;
  deviceId?: string;
}

// API key management
interface ApiKey {
  id: string;
  name: string;
  permissions: Permission[];
  createdAt: Timestamp;
  expiresAt?: Timestamp;
  lastUsed?: Timestamp;
}

interface CreateApiKeyRequest {
  name: string;
  permissions: Permission[];
  expiresInDays?: number;
}

interface CreateApiKeyResponse {
  key: ApiKey;
  secret: string;  // Only returned once!
}

type Permission =
  | 'read:identity'
  | 'write:identity'
  | 'read:contacts'
  | 'write:contacts'
  | 'read:messages'
  | 'send:messages'
  | 'read:apps'
  | 'manage:apps'
  | 'read:settings'
  | 'write:settings'
  | 'admin:full';
```

### Identity Management

```typescript
interface IdentityInfo {
  iid: IdentityIdentifier;
  genesisKeyFingerprint: string;
  currentSigningKeyFingerprint: string;
  currentEncryptionKeyFingerprint: string;
  createdAt: Timestamp;
  lastKeyRotation?: Timestamp;
  recoveryMethod: 'none' | 'social' | 'device-escrow' | 'threshold' | 'provider';
  endpoints: Endpoint[];
  profile?: PublicProfile;
}

interface PublicProfile {
  displayName?: string;
  avatar?: string;  // Content hash or URL
  bio?: string;
}

interface Device {
  did: DeviceIdentifier;
  name: string;
  createdAt: Timestamp;
  lastSeen: Timestamp;
  isCurrent: boolean;
  platform?: string;
}

interface KeyRotationResult {
  success: boolean;
  newKeyFingerprint: string;
  previousKeyFingerprint: string;
  rotatedAt: Timestamp;
}

interface DeviceAddResult {
  did: DeviceIdentifier;
  name: string;
  activationCode: string;  // For activating on new device
  expiresAt: Timestamp;
}
```

### Contact Management

```typescript
interface Contact {
  iid: IdentityIdentifier;
  displayName?: string;
  avatar?: string;
  trustLevel: TrustLevel;
  isBlocked: boolean;
  isOnline: boolean;
  lastSeen?: Timestamp;
  addedAt: Timestamp;
  addedBy: 'manual' | 'invite' | 'sync';
  notes?: string;
  tags: string[];
  sharedGroups: string[];
}

type TrustLevel =
  | 'unknown'       // Never verified
  | 'unverified'    // Added but not verified
  | 'verified'      // Out-of-band verification done
  | 'trusted';      // Fully trusted (e.g., close contact)

interface ContactMetadata {
  displayName?: string;
  notes?: string;
  tags?: string[];
  trustLevel?: TrustLevel;
}

interface ContactUpdate {
  displayName?: string;
  notes?: string;
  tags?: string[];
  trustLevel?: TrustLevel;
}

interface AddContactRequest {
  iid: IdentityIdentifier;
  displayName?: string;
  trustLevel?: TrustLevel;
}
```

### App Management

```typescript
interface InstalledApp {
  id: AppId;
  name: string;
  version: AppVersion;
  authorIid: IdentityIdentifier;
  authorName?: string;
  description: string;
  icon?: string;
  installedAt: Timestamp;
  lastOpened?: Timestamp;
  updateAvailable?: AppVersion;
  status: AppStatus;
  permissions: AppPermissions;
  storageUsed: number;  // bytes
  storageQuota: number; // bytes
}

type AppStatus =
  | 'installed'
  | 'running'
  | 'disabled'
  | 'error';

interface AppPermissions {
  granted: Capability[];
  denied: Capability[];
  pending: Capability[];  // Optional capabilities not yet decided
}

interface AppSource {
  type: 'url' | 'file' | 'repository';
  value: string;  // URL, file path, or "repo_id:app_id"
}

interface InstallResult {
  app: InstalledApp;
  permissionsRequested: Capability[];
  permissionsGranted: Capability[];
}

interface UpdateResult {
  app: InstalledApp;
  previousVersion: AppVersion;
  newPermissions: Capability[];  // Any new permissions in update
}

interface UninstallOptions {
  keepData?: boolean;  // Default: false
  keepSettings?: boolean;
}
```

### Settings

```typescript
interface NodeSettings {
  network: NetworkSettings;
  admin: AdminSettings;
  apps: AppSettings;
  privacy: PrivacySettings;
  storage: StorageSettings;
  notifications: NotificationSettings;
}

interface NetworkSettings {
  listenAddr: string;
  adminListenAddr: string;
  enableUpnp: boolean;
  externalAddr?: string;
  relayServers: string[];
  bandwidthLimitMbps?: number;
}

interface AdminSettings {
  enabled: boolean;
  requireTls: boolean;
  sessionTimeoutHours: number;
  ipAllowlist: string[];
}

interface AppSettings {
  autoUpdate: boolean;
  allowSideload: boolean;
  defaultStorageQuota: string;
  trustedRepositories: TrustedRepository[];
}

interface TrustedRepository {
  id: string;
  operatorIid: IdentityIdentifier;
  url: string;
  trustLevel: 'full' | 'prompt' | 'disabled';
  autoUpdate: boolean;
  addedAt: Timestamp;
}

interface PrivacySettings {
  publishIdentityHours: number;
  showOnlineStatus: boolean;
  sendReadReceipts: boolean;
  shareAnalytics: boolean;
}

interface StorageSettings {
  dataDir: string;
  logDir: string;
  backupEnabled: boolean;
  backupSchedule?: string;  // Cron expression
  backupRetentionDays: number;
}

interface NotificationSettings {
  enabled: boolean;
  soundEnabled: boolean;
  quietHoursStart?: string;  // HH:MM
  quietHoursEnd?: string;
}
```

### System Status

```typescript
interface NodeStatus {
  version: string;
  uptimeSeconds: number;
  status: 'healthy' | 'degraded' | 'unhealthy';
  identity: IdentityStatus;
  network: NetworkStatus;
  storage: StorageStatus;
  apps: AppsStatus;
}

interface IdentityStatus {
  iid: IdentityIdentifier;
  lastPublished?: Timestamp;
  deviceCount: number;
}

interface NetworkStatus {
  connectionsActive: number;
  connectionsDirect: number;
  connectionsRelay: number;
  relaysConnected: number;
  bytesSent: number;
  bytesReceived: number;
  externalAddrDetected?: string;
}

interface StorageStatus {
  dataUsedBytes: number;
  dataFreeBytes: number;
  messagesCount: number;
  documentsCount: number;
}

interface AppsStatus {
  installed: number;
  running: number;
  totalStorageUsed: number;
}
```

### Logging

```typescript
interface LogEntry {
  timestamp: Timestamp;
  level: LogLevel;
  target: string;
  message: string;
  fields?: Record<string, unknown>;
}

type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace';

interface LogOptions {
  level?: LogLevel;
  target?: string;
  since?: Timestamp;
  until?: Timestamp;
  limit?: number;
  search?: string;
}
```

### Backup

```typescript
interface BackupResult {
  id: string;
  createdAt: Timestamp;
  size: number;
  path: string;
  encrypted: boolean;
}

interface RestoreResult {
  success: boolean;
  restoredAt: Timestamp;
  identity: IdentityIdentifier;
  contactsRestored: number;
  messagesRestored: number;
  appsRestored: number;
  warnings: string[];
}

interface BackupListEntry {
  id: string;
  createdAt: Timestamp;
  size: number;
  path: string;
  type: 'full' | 'identity' | 'data';
}
```

## Admin API Client Interface

```typescript
interface AdminApiClient {
  // Authentication
  login(request: LoginRequest): Promise<LoginResponse>;
  logout(): Promise<void>;
  refreshSession(): Promise<Session>;

  // Identity
  getIdentity(): Promise<IdentityInfo>;
  updateProfile(profile: Partial<PublicProfile>): Promise<IdentityInfo>;
  rotateSigningKey(): Promise<KeyRotationResult>;
  rotateEncryptionKey(): Promise<KeyRotationResult>;
  getDevices(): Promise<Device[]>;
  addDevice(name: string): Promise<DeviceAddResult>;
  removeDevice(did: DeviceIdentifier): Promise<void>;
  getRecoveryConfig(): Promise<RecoveryConfig>;
  updateRecoveryConfig(config: RecoveryConfig): Promise<void>;

  // Contacts
  listContacts(options?: PaginationOptions): Promise<PaginatedResult<Contact>>;
  getContact(iid: IdentityIdentifier): Promise<Contact>;
  addContact(request: AddContactRequest): Promise<Contact>;
  updateContact(iid: IdentityIdentifier, update: ContactUpdate): Promise<Contact>;
  removeContact(iid: IdentityIdentifier): Promise<void>;
  blockContact(iid: IdentityIdentifier): Promise<void>;
  unblockContact(iid: IdentityIdentifier): Promise<void>;

  // Apps
  listApps(): Promise<InstalledApp[]>;
  getApp(appId: AppId): Promise<InstalledApp>;
  installApp(source: AppSource): Promise<InstallResult>;
  updateApp(appId: AppId): Promise<UpdateResult>;
  uninstallApp(appId: AppId, options?: UninstallOptions): Promise<void>;
  getAppPermissions(appId: AppId): Promise<AppPermissions>;
  updateAppPermissions(appId: AppId, permissions: Partial<AppPermissions>): Promise<void>;
  clearAppData(appId: AppId): Promise<void>;

  // Settings
  getSettings(): Promise<NodeSettings>;
  updateSettings(settings: Partial<NodeSettings>): Promise<NodeSettings>;
  resetSettings(section?: keyof NodeSettings): Promise<NodeSettings>;

  // System
  getStatus(): Promise<NodeStatus>;
  getLogs(options?: LogOptions): Promise<LogEntry[]>;
  createBackup(): Promise<BackupResult>;
  listBackups(): Promise<BackupListEntry[]>;
  restoreBackup(backupId: string, password?: string): Promise<RestoreResult>;
  restart(): Promise<void>;
  shutdown(): Promise<void>;

  // API Keys
  listApiKeys(): Promise<ApiKey[]>;
  createApiKey(request: CreateApiKeyRequest): Promise<CreateApiKeyResponse>;
  revokeApiKey(keyId: string): Promise<void>;
}
```

## Event Types (WebSocket)

```typescript
// WebSocket event stream
type AdminEvent =
  | StatusChangeEvent
  | ContactOnlineEvent
  | MessageReceivedEvent
  | AppInstalledEvent
  | AppUpdatedEvent
  | AppErrorEvent
  | SyncProgressEvent
  | ErrorEvent;

interface StatusChangeEvent {
  type: 'status_change';
  data: NodeStatus;
}

interface ContactOnlineEvent {
  type: 'contact_online';
  data: {
    iid: IdentityIdentifier;
    online: boolean;
    lastSeen?: Timestamp;
  };
}

interface MessageReceivedEvent {
  type: 'message_received';
  data: {
    messageId: string;
    senderIid: IdentityIdentifier;
    preview: string;
    receivedAt: Timestamp;
  };
}

interface AppInstalledEvent {
  type: 'app_installed';
  data: InstalledApp;
}

interface AppUpdatedEvent {
  type: 'app_updated';
  data: {
    app: InstalledApp;
    previousVersion: AppVersion;
  };
}

interface AppErrorEvent {
  type: 'app_error';
  data: {
    appId: AppId;
    error: string;
    timestamp: Timestamp;
  };
}

interface SyncProgressEvent {
  type: 'sync_progress';
  data: {
    documentId: string;
    progress: number;  // 0-100
    status: 'syncing' | 'complete' | 'error';
  };
}

interface ErrorEvent {
  type: 'error';
  data: {
    code: string;
    message: string;
    details?: Record<string, unknown>;
  };
}
```

## Common Types

```typescript
type Timestamp = string;  // RFC3339 format
type IdentityIdentifier = string;  // 32-char Base32
type DeviceIdentifier = string;  // 32-char Base32
type AppId = string;  // Reverse domain notation
type AppVersion = string;  // Semver
type Capability = string;  // Capability string

interface PaginationOptions {
  limit?: number;
  offset?: number;
  sortBy?: string;
  sortOrder?: 'asc' | 'desc';
}

interface PaginatedResult<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  hasMore: boolean;
}
```

## Missing Types (Referenced Elsewhere)

```typescript
// Recovery configuration for identity
interface RecoveryConfig {
  method: RecoveryMethod;
  // For social recovery
  trustees?: TrusteeConfig[];
  threshold?: number;          // M-of-N threshold
  // For device escrow
  escrowDevice?: DeviceIdentifier;
  // For provider recovery
  provider?: {
    name: string;
    endpoint: string;
    publicKey: string;
  };
}

type RecoveryMethod =
  | 'none'           // No recovery configured
  | 'social'         // M-of-N trustees
  | 'device-escrow'  // Backed up to another device
  | 'threshold'      // Shamir secret sharing
  | 'provider';      // Third-party recovery service

interface TrusteeConfig {
  iid: IdentityIdentifier;
  displayName?: string;
  addedAt: Timestamp;
}

// Network endpoint for identity
interface Endpoint {
  type: 'quic' | 'relay' | 'https';
  address: string;           // host:port or relay URL
  priority: number;          // Lower = preferred
  lastVerified?: Timestamp;
}

// Message types for Admin UI
interface MessageSummary {
  id: string;
  senderIid: IdentityIdentifier;
  senderName?: string;
  preview: string;           // First 100 chars, sanitized
  receivedAt: Timestamp;
  isRead: boolean;
  conversationId: string;
}

interface MessageExport {
  format: 'json' | 'mbox';
  conversationIds?: string[];
  since?: Timestamp;
  until?: Timestamp;
}

// Per-app settings (distinct from NodeSettings.apps)
interface PerAppSettings {
  enabled: boolean;
  autoStart: boolean;
  storageQuota: number;      // bytes
  backgroundAllowed: boolean;
  notificationsEnabled: boolean;
  customConfig?: Record<string, unknown>;
}

// Verification status for authors/contacts
interface VerificationStatus {
  level: 'unknown' | 'unverified' | 'verified' | 'trusted';
  sources: VerificationSource[];
}

interface VerificationSource {
  type: 'local' | 'repository' | 'contact' | 'enterprise';
  name?: string;             // Repository name, contact name, etc.
  verifiedAt?: Timestamp;
}

// Permission patch for app permissions
interface PermissionPatch {
  grant?: Capability[];      // Move to granted
  deny?: Capability[];       // Move to denied
  reset?: Capability[];      // Reset to app default
}

// Logs response with pagination
interface LogsResponse {
  entries: LogEntry[];
  cursor?: string;           // Opaque cursor for next page
  hasMore: boolean;
}
```

## Error Types

```typescript
interface ApiError {
  error: {
    code: ApiErrorCode;
    message: string;
    details?: Record<string, unknown>;
  };
}

type ApiErrorCode =
  // Client errors (4xx)
  | 'INVALID_REQUEST'       // 400: Malformed request
  | 'UNAUTHORIZED'          // 401: Missing or invalid authentication
  | 'FORBIDDEN'             // 403: Authenticated but not allowed
  | 'NOT_FOUND'             // 404: Resource doesn't exist
  | 'CONFLICT'              // 409: Resource state conflict
  | 'RATE_LIMITED'          // 429: Too many requests
  | 'PAYLOAD_TOO_LARGE'     // 413: Request body too large
  | 'VALIDATION_ERROR'      // 422: Request validation failed
  | 'CSRF_INVALID'          // 403: CSRF token missing/invalid
  | 'FRESH_AUTH_REQUIRED'   // 403: Sensitive operation needs re-auth

  // Server errors (5xx)
  | 'INTERNAL_ERROR'        // 500: Unexpected server error
  | 'SERVICE_UNAVAILABLE'   // 503: Service temporarily unavailable
  | 'TIMEOUT';              // 504: Operation timed out

// Error mapping to HTTP status codes
const ERROR_STATUS_CODES: Record<ApiErrorCode, number> = {
  INVALID_REQUEST: 400,
  UNAUTHORIZED: 401,
  FORBIDDEN: 403,
  NOT_FOUND: 404,
  CONFLICT: 409,
  RATE_LIMITED: 429,
  PAYLOAD_TOO_LARGE: 413,
  VALIDATION_ERROR: 422,
  CSRF_INVALID: 403,
  FRESH_AUTH_REQUIRED: 403,
  INTERNAL_ERROR: 500,
  SERVICE_UNAVAILABLE: 503,
  TIMEOUT: 504,
};

// Response semantics
// - All error responses are JSON (even 5xx)
// - 204 No Content: Used for successful DELETE, logout, etc.
// - 202 Accepted: Used for async operations (restart, shutdown)
// - Success responses: JSON body with resource, or 204 if no content
```
