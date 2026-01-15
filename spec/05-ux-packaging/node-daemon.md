# Node Daemon

## Overview

The node daemon (`postnode`) is the long-running process that provides identity, messaging, sync, and app runtime services. It manages all persistent state and exposes APIs for user interfaces.

## Daemon Lifecycle

### Startup Sequence

```
┌─────────────────────────────────────────────────────────────────┐
│                      Startup Sequence                            │
├─────────────────────────────────────────────────────────────────┤
│ 1. Load configuration                                            │
│    - Parse config file (TOML)                                   │
│    - Apply environment overrides                                │
│    - Validate configuration                                     │
├─────────────────────────────────────────────────────────────────┤
│ 2. Initialize storage                                            │
│    - Open/create data directory                                 │
│    - Run database migrations if needed                          │
│    - Verify data integrity                                      │
├─────────────────────────────────────────────────────────────────┤
│ 3. Load identity                                                 │
│    - Load or create identity keys                               │
│    - Derive IID from genesis key                                │
│    - Load device keys for this device                           │
├─────────────────────────────────────────────────────────────────┤
│ 4. Start subsystems (in order)                                   │
│    a. Key storage (TPM/Keychain/file)                           │
│    b. Transport layer (QUIC listener)                           │
│    c. DHT client                                                │
│    d. Messaging service                                         │
│    e. Sync service                                              │
│    f. App runtime                                               │
├─────────────────────────────────────────────────────────────────┤
│ 5. Start HTTP server                                             │
│    - Admin API                                                  │
│    - App serving                                                │
│    - Health/metrics endpoints                                   │
├─────────────────────────────────────────────────────────────────┤
│ 6. Publish identity                                              │
│    - Announce to DHT                                            │
│    - Connect to known relays                                    │
│    - Send presence to contacts (if configured)                  │
├─────────────────────────────────────────────────────────────────┤
│ 7. Resume pending operations                                     │
│    - Process queued outgoing messages                           │
│    - Sync pending documents                                     │
│    - Wake apps with pending invocations                         │
└─────────────────────────────────────────────────────────────────┘
```

### Shutdown Sequence

```
┌─────────────────────────────────────────────────────────────────┐
│                      Shutdown Sequence                           │
├─────────────────────────────────────────────────────────────────┤
│ 1. Signal received (SIGTERM, SIGINT, or API call)                │
├─────────────────────────────────────────────────────────────────┤
│ 2. Stop accepting new connections                                │
│    - Close QUIC listener                                        │
│    - Stop HTTP server (graceful drain)                          │
├─────────────────────────────────────────────────────────────────┤
│ 3. Complete in-flight operations (timeout: 30s)                  │
│    - Finish sending messages                                    │
│    - Complete sync operations                                   │
│    - Flush app state                                            │
├─────────────────────────────────────────────────────────────────┤
│ 4. Stop subsystems (reverse order)                               │
│    - Unload apps                                                │
│    - Close sync connections                                     │
│    - Close messaging sessions                                   │
│    - Leave DHT                                                  │
│    - Close transport                                            │
├─────────────────────────────────────────────────────────────────┤
│ 5. Persist final state                                           │
│    - Flush database                                             │
│    - Write checkpoint                                           │
├─────────────────────────────────────────────────────────────────┤
│ 6. Exit                                                          │
│    - Exit code 0 (clean) or 1 (forced)                          │
└─────────────────────────────────────────────────────────────────┘
```

## Process Model

### Single Process Architecture

The daemon runs as a single process with internal concurrency via async runtime (Tokio for Rust, goroutines for Go).

```
postnode (single process)
├── Main thread: HTTP server, signal handling
├── Async tasks:
│   ├── QUIC connection handlers
│   ├── DHT operations
│   ├── Message processing
│   ├── Sync operations
│   └── Background jobs
└── WASM instances (isolated memory spaces)
```

### Resource Limits

| Resource | Default Limit | Configurable |
|----------|---------------|--------------|
| Max open connections | 1000 | Yes |
| Max concurrent apps | 50 | Yes |
| Memory per app | 64 MB | Yes (per-app) |
| Total memory | 2 GB | Yes |
| Open file descriptors | 10000 | Yes |
| Background task threads | 4 | Yes |

### Process Supervision

The daemon is designed to be supervised by system process managers:

| Platform | Supervisor | Config Location |
|----------|------------|-----------------|
| Linux (systemd) | systemd | `/etc/systemd/system/postnode.service` |
| macOS | launchd | `~/Library/LaunchAgents/com.postnode.plist` |
| Docker | Docker daemon | `docker-compose.yml` or `Dockerfile` |
| Windows | Windows Service | `sc.exe` or NSSM |

## Data Directory Structure

```
~/.postnode/
├── config.toml              # User configuration
├── data/
│   ├── identity/
│   │   ├── identity.db      # SQLite: identity documents, contacts
│   │   └── keys/            # Encrypted key material
│   ├── messages/
│   │   ├── messages.db      # SQLite: message history
│   │   └── attachments/     # Binary attachments
│   ├── sync/
│   │   ├── documents/       # CRDT documents
│   │   └── sync.db          # SQLite: sync metadata
│   ├── apps/
│   │   ├── installed/       # Installed app packages
│   │   │   └── com.example.app/
│   │   │       ├── manifest.json
│   │   │       ├── main.wasm
│   │   │       └── assets/
│   │   └── storage/         # Per-app persistent storage
│   │       └── com.example.app/
│   │           └── app.db
│   └── runtime/
│       ├── sessions.db      # Active sessions
│       └── cache/           # Ephemeral cache
├── logs/
│   ├── postnode.log         # Main daemon log
│   └── apps/                # Per-app logs
└── run/
    └── postnode.sock        # Unix socket (if enabled)
```

### Data Integrity

| Protection | Mechanism |
|------------|-----------|
| Corruption detection | SQLite checksums, WAL mode |
| Atomic writes | Write-ahead logging |
| Backup support | Online backup via SQLite API |
| Encryption at rest | Optional, per-database |

## Key Storage

### Storage Backends

| Backend | Platform | Security Level |
|---------|----------|----------------|
| **TPM 2.0** | Linux with TPM | Hardware-backed, highest |
| **Secure Enclave** | macOS | Hardware-backed, highest |
| **Windows CNG** | Windows | OS-protected |
| **Keychain** | macOS/iOS | OS-protected |
| **Encrypted file** | Any | Password-derived key |
| **Plain file** | Any (dev only) | No protection |

### Key Hierarchy

```
Root of Trust (hardware or password-derived)
    │
    ├── Identity Master Key (IK)
    │   ├── Genesis signing key (Ed25519, never changes)
    │   ├── Current signing key (Ed25519, rotatable)
    │   └── Encryption keys (X25519, rotatable)
    │
    ├── Device Keys (per-device)
    │   ├── Device signing key (Ed25519)
    │   └── Device transport key (X25519)
    │
    └── Storage Encryption Key
        └── Used for at-rest encryption of databases
```

### Key Operations

```typescript
interface KeyStorage {
  // Identity keys
  getGenesisSigningKey(): Promise<Ed25519KeyPair>;
  getCurrentSigningKey(): Promise<Ed25519KeyPair>;
  getCurrentEncryptionKey(): Promise<X25519KeyPair>;
  rotateSigningKey(): Promise<{ newKey: Ed25519KeyPair; signature: Uint8Array }>;
  rotateEncryptionKey(): Promise<{ newKey: X25519KeyPair }>;

  // Device keys
  getDeviceSigningKey(did: DeviceIdentifier): Promise<Ed25519KeyPair>;
  // NOTE: getDeviceTransportKey is reserved for future use. v1 uses device signing key for handshake.
  // getDeviceTransportKey(did: DeviceIdentifier): Promise<X25519KeyPair>;
  createDeviceKeys(): Promise<{ did: DeviceIdentifier; keys: DeviceKeyPair }>;
  revokeDeviceKeys(did: DeviceIdentifier): Promise<void>;

  // Signing operations (key material never leaves storage)
  sign(keyType: KeyType, data: Uint8Array): Promise<Uint8Array>;
  decrypt(keyType: KeyType, ciphertext: Uint8Array): Promise<Uint8Array>;

  // Export (for backup/migration)
  exportEncrypted(password: string): Promise<Uint8Array>;
  importEncrypted(data: Uint8Array, password: string): Promise<void>;
}

type KeyType =
  | 'identity:signing:genesis'
  | 'identity:signing:current'
  | 'identity:encryption:current'
  | `device:signing:${string}`;
  // NOTE: device:transport reserved for future use (v1 uses device signing key only)
```

## HTTP API

### Server Configuration

```toml
[http]
# Local HTTP server (admin + apps) - HTTP allowed on localhost only
listen_addr = "127.0.0.1:8080"
# Production/external - ALWAYS requires TLS
external_listen_addr = "0.0.0.0:8443"
external_tls_cert = "/path/to/cert.pem"
external_tls_key = "/path/to/key.pem"

# Timeouts
read_timeout_seconds = 30
write_timeout_seconds = 30
idle_timeout_seconds = 120

# Limits
max_request_body_bytes = 104857600  # 100 MB (matches max package size)
max_concurrent_requests = 1000

# Streaming upload support for large files
enable_chunked_upload = true
max_upload_chunk_bytes = 10485760    # 10 MB chunks
```

### Network Mode

The daemon operates in one of two modes:

| Mode | Listen Address | TLS | Session Cookie | Use Case |
|------|----------------|-----|----------------|----------|
| **Local** | `127.0.0.1:8080` | No (HTTP) | `Secure=false` | Local development, same-machine access |
| **Production** | `0.0.0.0:8443` | Required | `Secure=true` | Remote access, reverse proxy |

**Important:** When `external_listen_addr` is configured with TLS, the local `listen_addr` is disabled unless `allow_local_http = true` is explicitly set.

```toml
[http]
# Explicit local mode for development
allow_local_http = true  # Default: false when external is configured
```

### REST API Reference

Complete Admin API endpoint specification. All endpoints under `/admin/v1/` require authentication.

#### Authentication Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| POST | `/admin/v1/auth/login` | `LoginRequest` | `LoginResponse` | Sets session cookie |
| POST | `/admin/v1/auth/logout` | - | `204 No Content` | Clears session |
| POST | `/admin/v1/auth/refresh` | - | `Session` | Extend session |
| POST | `/admin/v1/auth/reauth` | `{ password: string }` | `Session` | Fresh auth for sensitive ops |

#### Identity Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| GET | `/admin/v1/identity` | - | `IdentityInfo` | Current identity |
| PUT | `/admin/v1/identity/profile` | `Partial<PublicProfile>` | `IdentityInfo` | Update profile |
| POST | `/admin/v1/identity/rotate/signing` | - | `KeyRotationResult` | Rotate signing key (fresh auth) |
| POST | `/admin/v1/identity/rotate/encryption` | - | `KeyRotationResult` | Rotate encryption key (fresh auth) |
| GET | `/admin/v1/identity/export` | - | `application/octet-stream` | Encrypted identity export |
| GET | `/admin/v1/identity/recovery` | - | `RecoveryConfig` | Recovery configuration |
| PUT | `/admin/v1/identity/recovery` | `RecoveryConfig` | `RecoveryConfig` | Update recovery (fresh auth) |

#### Device Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| GET | `/admin/v1/devices` | - | `Device[]` | List all devices |
| POST | `/admin/v1/devices` | `{ name: string }` | `DeviceAddResult` | Add new device (fresh auth) |
| DELETE | `/admin/v1/devices/{did}` | - | `204 No Content` | Remove device (fresh auth) |

#### Contact Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| GET | `/admin/v1/contacts` | Query: `limit`, `offset`, `sort_by`, `sort_order` | `PaginatedResult<Contact>` | List contacts |
| GET | `/admin/v1/contacts/{iid}` | - | `Contact` | Get single contact |
| POST | `/admin/v1/contacts` | `AddContactRequest` | `Contact` | Add contact |
| PUT | `/admin/v1/contacts/{iid}` | `ContactUpdate` | `Contact` | Update contact |
| DELETE | `/admin/v1/contacts/{iid}` | - | `204 No Content` | Remove contact |
| POST | `/admin/v1/contacts/{iid}/block` | - | `204 No Content` | Block contact |
| DELETE | `/admin/v1/contacts/{iid}/block` | - | `204 No Content` | Unblock contact |

**Allowed sort fields for contacts:** `display_name`, `added_at`, `last_seen`, `trust_level`

**Note:** All query parameter names use snake_case for consistency with JSON field naming (see interfaces.md).

#### App Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| GET | `/admin/v1/apps` | - | `InstalledApp[]` | List installed apps |
| GET | `/admin/v1/apps/{app_id}` | - | `InstalledApp` | Get app details |
| POST | `/admin/v1/apps/install` | `InstallRequest` | `InstallResult` | Install app (see below) |
| POST | `/admin/v1/apps/install/upload` | `multipart/form-data` | `InstallResult` | Upload and install `.postapp` (see multipart spec below) |
| POST | `/admin/v1/apps/{app_id}/update` | - | `UpdateResult` | Update to latest |
| DELETE | `/admin/v1/apps/{app_id}` | Query: `keepData?` | `204 No Content` | Uninstall app |
| GET | `/admin/v1/apps/{app_id}/permissions` | - | `AppPermissions` | Get permissions |
| PATCH | `/admin/v1/apps/{app_id}/permissions` | `PermissionPatch` | `AppPermissions` | Modify permissions |
| POST | `/admin/v1/apps/{app_id}/clear-data` | - | `204 No Content` | Clear app data (fresh auth) |
| GET | `/admin/v1/apps/{app_id}/settings` | - | `PerAppSettings` | Per-app settings |
| PUT | `/admin/v1/apps/{app_id}/settings` | `PerAppSettings` | `PerAppSettings` | Update per-app settings |

**Install Request Types:**
```typescript
// Install by URL
{ source: { type: 'url', value: 'https://...' } }

// Install from repository
{ source: { type: 'repository', value: 'repo_id:app_id' } }

// For file upload, use /apps/install/upload with multipart/form-data
```

**Permission Patch:**
```typescript
interface PermissionPatch {
  grant?: Capability[];     // Move to granted
  deny?: Capability[];      // Move to denied
  reset?: Capability[];     // Reset to app default
}
```

**Multipart Upload Specification:**

For file upload endpoints (`/apps/install/upload` and `/backups/upload`), the following normative requirements apply:

| Field | Requirement | Description |
|-------|-------------|-------------|
| Part name | **REQUIRED**: `file` | The form field name MUST be `file` |
| Content-Type | `application/octet-stream` or `application/zip` | MIME type of the uploaded file |
| Filename | Recommended | Original filename for logging/display |

**Example multipart request:**
```http
POST /admin/v1/apps/install/upload HTTP/1.1
Content-Type: multipart/form-data; boundary=----WebKitFormBoundary7MA4YWxkTrZu0gW

------WebKitFormBoundary7MA4YWxkTrZu0gW
Content-Disposition: form-data; name="file"; filename="notes-app.postapp"
Content-Type: application/octet-stream

<binary data>
------WebKitFormBoundary7MA4YWxkTrZu0gW--
```

**JavaScript example:**
```typescript
const formData = new FormData();
formData.append('file', fileBlob, 'app.postapp');
await fetch('/admin/v1/apps/install/upload', {
  method: 'POST',
  body: formData,
  credentials: 'same-origin',
});
```

#### Settings Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| GET | `/admin/v1/settings` | - | `NodeSettings` | All settings |
| GET | `/admin/v1/settings/{section}` | - | Settings section | e.g., `/settings/network` |
| PATCH | `/admin/v1/settings` | `Partial<NodeSettings>` | `NodeSettings` | Update settings (fresh auth) |
| POST | `/admin/v1/settings/reset` | `{ section?: string }` | `NodeSettings` | Reset to defaults |

#### Backup Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| GET | `/admin/v1/backups` | - | `BackupListEntry[]` | List backups |
| POST | `/admin/v1/backups` | `{ type?: 'full' \| 'identity' \| 'data' }` | `BackupResult` | Create backup |
| GET | `/admin/v1/backups/{id}` | - | `application/octet-stream` | Download backup file |
| POST | `/admin/v1/backups/upload` | `multipart/form-data` | `BackupListEntry` | Upload backup file (see multipart spec below) |
| POST | `/admin/v1/backups/{id}/restore` | `{ password?: string }` | `RestoreResult` | Restore backup (fresh auth) |
| DELETE | `/admin/v1/backups/{id}` | - | `204 No Content` | Delete backup file |

#### API Key Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| GET | `/admin/v1/api-keys` | - | `ApiKey[]` | List API keys |
| POST | `/admin/v1/api-keys` | `CreateApiKeyRequest` | `CreateApiKeyResponse` | Create key (fresh auth) |
| DELETE | `/admin/v1/api-keys/{id}` | - | `204 No Content` | Revoke key |

#### System Endpoints

| Method | Path | Request | Response | Notes |
|--------|------|---------|----------|-------|
| GET | `/admin/v1/status` | - | `NodeStatus` | System status |
| GET | `/admin/v1/logs` | Query: `level`, `target`, `since`, `until`, `limit`, `cursor` | `LogsResponse` | Query logs |
| POST | `/admin/v1/restart` | - | `202 Accepted` | Restart node |
| POST | `/admin/v1/shutdown` | - | `202 Accepted` | Shutdown node |

**Logs Response:**
```typescript
interface LogsResponse {
  entries: LogEntry[];
  cursor?: string;         // For pagination
  hasMore: boolean;
}
```

#### WebSocket Endpoint

| Path | Auth | Notes |
|------|------|-------|
| `/admin/v1/events` | Session cookie OR `?token=<api_key>` | Real-time events |

See [WebSocket Events](#websocket-events) section for event format.

#### Public Endpoints (No Auth)

| Method | Path | Response | Notes |
|--------|------|----------|-------|
| GET | `/health/live` | `{ status: 'alive' }` | Liveness probe |
| GET | `/health/ready` | `{ status: 'ready' }` | Readiness probe |
| GET | `/health` | `NodeHealthStatus` | Detailed health |
| GET | `/metrics` | Prometheus format | Metrics endpoint |

#### App Serving Endpoints

| Method | Path | Response | Notes |
|--------|------|----------|-------|
| GET | `/apps/{app_id}/` | HTML | App's index.html |
| GET | `/apps/{app_id}/assets/*` | Static files | App assets |
| ALL | `/apps/{app_id}/api/*` | Proxied | App backend API |

### WebSocket Events

The `/admin/v1/events` WebSocket provides real-time updates.

**Authentication:**
- If session cookie present: uses session auth
- Otherwise: requires `?token=<api_key>` query parameter

**Connection URL:**
- Local mode: `ws://localhost:8080/admin/v1/events`
- Production mode: `wss://hostname:8443/admin/v1/events`

**Reconnection:**
- Client should implement exponential backoff (1s, 2s, 4s, 8s, max 30s)
- Pass `?lastEventId=<id>` on reconnect to receive missed events
- Server buffers last 1000 events for replay

**Server→Client Messages (wrapped):**

Server messages are wrapped in `WebSocketMessage` with metadata for replay support:

```typescript
interface WebSocketMessage {
  id: string;               // Monotonic event ID for replay
  type: AdminEventType;     // Event type discriminator
  timestamp: Timestamp;     // ISO 8601 timestamp
  data: unknown;            // Event-specific payload
}

type AdminEventType =
  | 'status_change'
  | 'contact_online'
  | 'message_received'
  | 'app_installed'
  | 'app_updated'
  | 'app_error'
  | 'sync_progress'
  | 'log_entry'            // Optional, if subscribed
  | 'error';
```

**Client→Server Messages (NOT wrapped):**

Client messages are simple command objects without wrapper metadata:

```typescript
// Subscribe to specific event types
interface SubscribeMessage {
  type: 'subscribe';
  events: AdminEventType[];  // Event types to subscribe to
}

// Unsubscribe from event types
interface UnsubscribeMessage {
  type: 'unsubscribe';
  events: AdminEventType[];  // Event types to unsubscribe from
}

// Union of all client→server message types
type ClientWebSocketMessage = SubscribeMessage | UnsubscribeMessage;
```

**Default subscription:** All event types except `log_entry` are subscribed by default on connection.

**Example client usage:**
```typescript
const ws = new WebSocket('/admin/v1/events');

// Subscribe to log entries (not included by default)
ws.send(JSON.stringify({ type: 'subscribe', events: ['log_entry'] }));

// Unsubscribe from sync progress events
ws.send(JSON.stringify({ type: 'unsubscribe', events: ['sync_progress'] }));

// Handle server messages (always wrapped)
ws.onmessage = (e) => {
  const msg: WebSocketMessage = JSON.parse(e.data);
  console.log(`Event ${msg.id} at ${msg.timestamp}: ${msg.type}`, msg.data);
};
```

### Authentication

The Admin API uses a tiered authentication model with clear separation between credential types.

#### Credential Types

| Type | Format | Use Case | Storage |
|------|--------|----------|---------|
| **Admin Password** | User-chosen password | Interactive setup, browser login | Never stored (hashed) |
| **Admin Token** | 64 hex chars | Headless/automation access | Config file (hashed) |
| **API Key** | 64 hex chars | Third-party integrations | Database (hashed) |
| **Session Cookie** | Signed token | Browser sessions after login | HttpOnly cookie |

#### Authentication Flows

**Browser Authentication (Admin UI):**
1. User submits password via login form
2. Server validates against stored password hash (argon2id)
3. Server creates session, returns session metadata (no token in body)
4. Server sets HttpOnly session cookie
5. All subsequent requests use cookie automatically
6. CSRF protection required for state-changing requests

**Headless/CLI Authentication:**
1. Admin token set via environment variable or config
2. Requests include `Authorization: Bearer <admin_token>`
3. No session/cookie involved
4. Full admin access

**API Key Authentication:**
1. API key created via Admin UI or CLI
2. Requests include `Authorization: Bearer <api_key>`
3. Access scoped to key permissions
4. No session/cookie involved

```typescript
interface AuthConfig {
  // Password authentication (interactive)
  passwordHash: string;             // argon2id hash of admin password

  // Token authentication (headless)
  adminTokenHash?: string;          // SHA256 of admin token

  // Session management (browser)
  sessionSecret: string;            // HMAC key for signing session cookies
  sessionTimeoutHours: number;      // Default: 24

  // API keys (third-party)
  apiKeys: ApiKey[];

  // App tokens (per-app delegated)
  appTokens: AppToken[];
}

interface ApiKey {
  id: string;
  keyHash: string;                  // SHA256 of key
  name: string;
  permissions: Permission[];
  createdAt: string;
  expiresAt?: string;
  lastUsed?: string;
}
```

#### CSRF Protection

For browser sessions using HttpOnly cookies, CSRF protection is required on all state-changing endpoints.

**Mechanism: Double-Submit Cookie Pattern with Body Token**

On successful login, the server MUST:
1. Return `csrfToken` in the `LoginResponse` body (see `interfaces.md`)
2. Set `postnode_csrf` cookie (NOT HttpOnly, readable by JS) to the SAME value
3. Use `SameSite=Strict` for both session and CSRF cookies

On subsequent requests:
1. Client includes token in `X-CSRF-Token` header on POST/PUT/DELETE
2. Server validates header matches cookie value

The dual delivery (body + cookie) allows clients to initialize CSRF state immediately after login without parsing cookies.

```typescript
// CSRF configuration
interface CsrfConfig {
  cookieName: 'postnode_csrf';
  headerName: 'X-CSRF-Token';
  tokenLength: 32;  // bytes, hex encoded = 64 chars
  sameSite: 'strict';
}

// Endpoints exempt from CSRF (read-only)
const CSRF_EXEMPT = [
  'GET *',
  'HEAD *',
  'OPTIONS *',
];

// CSRF validation errors
interface CsrfError {
  error: {
    code: 'CSRF_INVALID';
    message: 'CSRF token missing or invalid';
  };
}
// HTTP Status: 403 Forbidden
```

#### Session Cookie Configuration

```typescript
// Cookie settings vary by network mode
interface SessionCookieConfig {
  name: 'postnode_session';
  httpOnly: true;                   // Always
  sameSite: 'strict';               // Always
  secure: boolean;                  // true for TLS, false for localhost
  maxAge: number;                   // sessionTimeoutHours * 3600
  path: '/admin';                   // Scoped to admin API
}
```

#### Re-authentication for Sensitive Operations

Certain operations require fresh authentication (password re-entry within last 5 minutes):

| Operation | Requires Fresh Auth | Rationale |
|-----------|---------------------|-----------|
| View settings | No | Read-only |
| Change password | Yes | Credential change |
| Rotate keys | Yes | Critical security |
| Add/remove device | Yes | Device management |
| Delete all data | Yes | Destructive |
| Create backup | No | Data export |
| Restore backup | Yes | Destructive |
| Uninstall app | No | Recoverable |
| Clear app data | Yes | Destructive |
| Create API key | Yes | Credential creation |
| Revoke API key | No | Reducing access |

Fresh auth is validated via `X-Fresh-Auth-At` timestamp in session, compared against 5-minute window.

### Error Responses

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
```

**Forward Compatibility:** Clients MUST accept unknown error codes gracefully. New error codes may be added in future versions without a major version bump. Unknown codes SHOULD be treated as `INTERNAL_ERROR` for error handling purposes.

**Canonical Error Registry:** The authoritative `ApiErrorCode` definition and HTTP status code mappings are specified in `interfaces.md`. This file mirrors that definition for reference.

## Background Services

### Scheduled Tasks

| Task | Interval | Purpose |
|------|----------|---------|
| Identity publish | 24 hours | Refresh DHT record |
| Key rotation check | 1 hour | Auto-rotate if policy requires |
| Session cleanup | 1 hour | Remove expired sessions |
| Log rotation | Daily | Compress and archive logs |
| Cache cleanup | 1 hour | Remove expired cache entries |
| App background | Per-app | App-specific background work |
| Sync poll | 5 minutes | Check for sync updates |
| Health self-check | 1 minute | Internal health validation |

### Task Scheduler

```typescript
interface Scheduler {
  // Register a recurring task
  schedule(
    name: string,
    interval: Duration,
    handler: () => Promise<void>,
    options?: {
      runImmediately?: boolean;
      maxRetries?: number;
      timeout?: Duration;
    }
  ): TaskHandle;

  // Run a one-time task
  runOnce(
    name: string,
    handler: () => Promise<void>,
    options?: {
      delay?: Duration;
      timeout?: Duration;
    }
  ): TaskHandle;

  // Task control
  cancel(handle: TaskHandle): void;
  pause(handle: TaskHandle): void;
  resume(handle: TaskHandle): void;

  // Introspection
  listTasks(): TaskInfo[];
  getTaskStatus(handle: TaskHandle): TaskStatus;
}

interface TaskInfo {
  name: string;
  interval?: Duration;
  lastRun?: Timestamp;
  nextRun?: Timestamp;
  status: 'running' | 'scheduled' | 'paused' | 'cancelled';
  errorCount: number;
  lastError?: string;
}
```

## CLI Interface

### Command Structure

```
postnode - Personal node daemon

USAGE:
    postnode [OPTIONS] <COMMAND>

COMMANDS:
    start       Start the daemon
    stop        Stop a running daemon
    status      Show daemon status
    config      Manage configuration
    identity    Manage identity
    apps        Manage installed apps
    backup      Create/restore backups
    logs        View logs
    version     Show version information
    help        Show help

OPTIONS:
    -c, --config <PATH>    Config file path
    -d, --data-dir <PATH>  Data directory path
    -v, --verbose          Increase verbosity
    -q, --quiet            Decrease verbosity
    --json                 Output as JSON

EXAMPLES:
    postnode start                 # Start daemon with default config
    postnode start --config /etc/postnode/config.toml
    postnode status                # Check if daemon is running
    postnode apps list             # List installed apps
    postnode backup create         # Create encrypted backup
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error |
| 3 | Data directory error |
| 4 | Already running |
| 5 | Not running (for stop/status) |
| 6 | Permission denied |
| 130 | Interrupted (Ctrl+C) |

## Signals

| Signal | Behavior |
|--------|----------|
| SIGTERM | Graceful shutdown (30s timeout) |
| SIGINT | Same as SIGTERM |
| SIGHUP | Reload configuration (where safe) |
| SIGUSR1 | Dump diagnostics to log |
| SIGUSR2 | Force log rotation |
