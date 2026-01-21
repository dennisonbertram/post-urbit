# Secure Bridge Protocol Specification

## Overview

The Secure Bridge Protocol is the **ONLY** IPC channel between sandboxed app webviews and the Rust backend. All app communication flows through a single Tauri command (`postbridge_invoke`), ensuring a minimal attack surface with centralized security enforcement.

### Design Principles

1. **Identity from Infrastructure** - App identity derived from webview label (`app-{app_id}`), never from payload
2. **Fail Closed** - Unknown requests denied by default
3. **Defense in Depth** - Multiple security layers (CBOR validation, session binding, capability checks, rate limiting)
4. **Minimal Attack Surface** - Single command entry point for all app IPC
5. **Auditability** - All operations logged with correlation IDs

### Threat Model

- **Trusted**: Tauri framework, Rust backend, shell webview, OS primitives
- **Untrusted**: All third-party app webviews, any data from apps
- **Partially Trusted**: Marketplace-signed apps (verified authorship, not behavior)

### Related Documents

- [Domain 2: App Sandbox & Isolation](./02-APP_SANDBOX_ISOLATION.md) - Webview isolation and identity
- [Domain 3: Resource Constraints](./03-RESOURCE_CONSTRAINTS.md) - Bridge rate limits
- [ADR-003: Multi-webview Architecture](../adrs/ADR-003-multiwebview-isolation.md)

---

## Transport Binding

### Single Entry Point

```rust
/// The ONLY Tauri command available to app webviews
/// Registered in capabilities/app-default.json with "postbridge:allow-invoke"
#[tauri::command]
pub async fn postbridge_invoke(
    webview: Webview,
    state: State<'_, AppState>,
    request_bytes: Vec<u8>,
) -> Result<Vec<u8>, BridgeError> {
    // CRITICAL: Derive identity from webview label
    let label = webview.label();

    if !label.starts_with("app-") {
        return Err(BridgeError::unauthorized());
    }

    let app_id = label.strip_prefix("app-")
        .ok_or_else(|| BridgeError::unauthorized())?;

    // Validate and process request
    state.bridge.handle_request(app_id, label, request_bytes).await
}
```

### Transport Properties

| Property | Value |
|----------|-------|
| Encoding | CBOR (RFC 8949) |
| Max request size | 256 KB |
| Max response size | 256 KB |
| Max concurrent requests | 16 per session |
| Timeout | 30 seconds (global) |

---

## CBOR Profile

### Strict Validation Requirements

The bridge enforces a strict CBOR profile to prevent parser attacks and ensure deterministic behavior.

**MUST reject:**
- Indefinite-length items (arrays, maps, byte strings, text strings)
- Duplicate map keys
- Non-UTF8 text strings
- Floating point NaN/Infinity (unless explicitly needed)
- Unknown CBOR tags (unless whitelisted)

**Limits enforced during decode:**
| Limit | Value |
|-------|-------|
| Max nesting depth | 32 |
| Max collection length | 1000 |
| Max string length | 65536 bytes |
| Max total payload | 262144 bytes (256 KB) |

### CBOR Validator

```rust
pub struct CborValidator {
    max_depth: usize,
    max_collection_len: usize,
    max_string_len: usize,
    max_payload_bytes: usize,
}

impl CborValidator {
    pub fn validate(&self, bytes: &[u8]) -> Result<(), CborValidationError> {
        if bytes.len() > self.max_payload_bytes {
            return Err(CborValidationError::PayloadTooLarge);
        }

        self.validate_recursive(bytes, 0)?;
        Ok(())
    }

    fn validate_recursive(&self, bytes: &[u8], depth: usize) -> Result<(), CborValidationError> {
        if depth > self.max_depth {
            return Err(CborValidationError::NestingTooDeep);
        }
        // ... recursive validation
    }
}
```

---

## Envelope Schema

### CDDL Schema

```cddl
; ============================
; Bridge Protocol v1 Envelope
; ============================

; Request envelope (app → backend)
bridge-request = {
    v: 1,                              ; Protocol version (fixed)
    id: request-id,                    ; Unique request ID (UUID)
    ts: uint,                          ; Client timestamp (ms since epoch)
    session: session-id,               ; Session ID from handshake
    token: text,                       ; HMAC authentication token
    method: method-name,               ; API method name
    params: any,                       ; Method-specific parameters
    ? deadline_ms: uint,               ; Request deadline (client hint)
    ? trace_id: text,                  ; Distributed tracing correlation
    ? idempotency_key: text,           ; Explicit idempotency key
}

; Response envelope (backend → app)
bridge-response = {
    v: 1,                              ; Protocol version
    id: request-id,                    ; Echoed request ID
    ts: uint,                          ; Response timestamp
    ok: bool,                          ; Success flag
    ? result: any,                     ; Method result (if ok=true)
    ? error: bridge-error,             ; Error details (if ok=false)
    server_ts: uint,                   ; Server processing start time
    processing_ms: uint,               ; Processing duration
}

; Error structure
bridge-error = {
    code: error-code,                  ; Machine-readable code
    message: text,                     ; Human-readable message
    ? error_id: text,                  ; Log correlation ID
    ? details: { * text => any },      ; Safe additional context
    retryable: bool,                   ; Can client retry?
    ? retry_after_ms: uint,            ; Wait time before retry
}

; Type definitions
request-id = text .size (1..64)        ; UUID format preferred
session-id = text .size (1..64)        ; UUID format
method-name = text .regexp "^[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*)+$"

error-code = "INVALID_REQUEST" /
             "UNAUTHORIZED" /
             "PERMISSION_DENIED" /
             "NOT_FOUND" /
             "CONFLICT" /
             "RATE_LIMITED" /
             "PAYLOAD_TOO_LARGE" /
             "TIMEOUT" /
             "INTERNAL_ERROR"
```

### Rust Structures

```rust
use serde::{Deserialize, Serialize};

/// Bridge request from app to backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRequest {
    /// Protocol version (must be 1)
    pub v: u8,

    /// Unique request ID (UUID)
    pub id: String,

    /// Client timestamp (milliseconds since epoch)
    pub ts: u64,

    /// Session ID from handshake
    pub session: String,

    /// HMAC authentication token
    pub token: String,

    /// API method name (e.g., "storage.get")
    pub method: String,

    /// Method-specific parameters
    pub params: ciborium::Value,

    /// Request deadline (client hint, optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,

    /// Distributed tracing ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,

    /// Explicit idempotency key (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// Bridge response from backend to app
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    /// Protocol version
    pub v: u8,

    /// Echoed request ID
    pub id: String,

    /// Response timestamp
    pub ts: u64,

    /// Success flag
    pub ok: bool,

    /// Method result (if ok=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ciborium::Value>,

    /// Error details (if ok=false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BridgeError>,

    /// Server processing start time
    pub server_ts: u64,

    /// Processing duration in milliseconds
    pub processing_ms: u64,
}

/// Error details structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeError {
    /// Machine-readable error code
    pub code: BridgeErrorCode,

    /// Human-readable message (safe for display)
    pub message: String,

    /// Log correlation ID (for support)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_id: Option<String>,

    /// Safe additional context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<std::collections::HashMap<String, ciborium::Value>>,

    /// Whether the request can be retried
    pub retryable: bool,

    /// Suggested wait time before retry (for rate limits)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BridgeErrorCode {
    InvalidRequest,
    Unauthorized,
    PermissionDenied,
    NotFound,
    Conflict,
    RateLimited,
    PayloadTooLarge,
    Timeout,
    InternalError,
}
```

---

## Session Lifecycle

### State Machine

```
           create_session (shell-only)
    ┌────────────────────────────────────────┐
    │                                        │
    ▼                                        │
┌──────────────────────────────────────────────────┐
│                     Active                        │
│  - Accepting requests                             │
│  - Token validation                               │
│  - Bound to webview label                         │
└───────────────────────┬──────────────────────────┘
                        │
    ┌───────────────────┼───────────────────┐
    │                   │                   │
  expire          invalidate            evict
  (TTL)          (explicit)           (Cold)
    │                   │                   │
    ▼                   ▼                   ▼
┌──────────────────────────────────────────────────┐
│                   Terminated                      │
│  - All requests rejected                          │
│  - Resources released                             │
│  - Cached responses purged                        │
└──────────────────────────────────────────────────┘
```

### Session Structure

```rust
use std::sync::atomic::{AtomicI64, AtomicU64};
use chrono::{DateTime, Utc};

/// Active session for an app webview
#[derive(Debug)]
pub struct AppSession {
    /// Unique session identifier
    pub session_id: String,

    /// App identifier (from webview label)
    pub app_id: String,

    /// Bound webview label (for validation)
    pub webview_label: String,

    /// Granted capabilities
    pub capabilities: Vec<String>,

    /// Session creation time
    pub created_at: DateTime<Utc>,

    /// Session expiration time
    pub expires_at: DateTime<Utc>,

    /// Random nonce for token generation
    pub nonce: String,

    /// Key ID for token (supports rotation)
    pub token_kid: String,

    /// Request counter (for metrics)
    pub request_count: AtomicU64,

    /// Last activity timestamp (ms since epoch)
    pub last_activity: AtomicI64,
}

/// Session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Session TTL (default: 24 hours)
    pub session_ttl: std::time::Duration,

    /// Max sessions per app
    pub max_sessions_per_app: usize,

    /// Nonce length (bytes)
    pub nonce_length: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_ttl: std::time::Duration::from_secs(24 * 60 * 60),
            max_sessions_per_app: 1,
            nonce_length: 32,
        }
    }
}
```

### Session Binding

Sessions are bound to the specific webview label, not just app_id:

```rust
impl SessionManager {
    pub fn validate_request(
        &self,
        request: &BridgeRequest,
        webview_label: &str,
        derived_app_id: &str,
    ) -> Result<&AppSession, BridgeError> {
        let session = self.sessions.get(&request.session)
            .ok_or_else(|| BridgeError::unauthorized())?;

        // Verify session not expired
        if Utc::now() > session.expires_at {
            return Err(BridgeError::unauthorized());
        }

        // Verify session bound to this webview
        if session.webview_label != webview_label {
            return Err(BridgeError::unauthorized());
        }

        // Verify app_id matches
        if session.app_id != derived_app_id {
            return Err(BridgeError::unauthorized());
        }

        Ok(session)
    }
}
```

---

## Token Format and Key Management

### Token Structure

```
{kid}.{signature_base64url}

Example:
k1.dGhpcyBpcyBhIHNhbXBsZSBzaWduYXR1cmU
```

### Token Generation

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

type HmacSha256 = Hmac<Sha256>;

/// Token configuration with key rotation support
pub struct TokenConfig {
    /// Current HMAC secret (from OS keystore)
    pub current_secret: [u8; 32],

    /// Current key ID
    pub current_kid: String,

    /// Previous secret (during rotation window)
    pub previous_secret: Option<[u8; 32]>,

    /// Previous key ID
    pub previous_kid: Option<String>,

    /// Rotation grace period
    pub rotation_grace_period: std::time::Duration,
}

impl SessionManager {
    /// Generate authentication token for session
    pub fn generate_token(&self, session: &AppSession) -> Result<String, TokenError> {
        let payload = format!(
            "post-urbit:bridge-token:v1:{}:{}:{}:{}:{}",
            session.session_id,
            session.app_id,
            session.webview_label,
            session.created_at.timestamp(),
            session.nonce
        );

        let mut mac = HmacSha256::new_from_slice(&self.config.current_secret)
            .map_err(|_| TokenError::InvalidKey)?;
        mac.update(payload.as_bytes());
        let signature = mac.finalize().into_bytes();

        let encoded = URL_SAFE_NO_PAD.encode(signature);
        Ok(format!("{}.{}", self.config.current_kid, encoded))
    }

    /// Validate token with constant-time comparison
    pub fn validate_token(&self, session: &AppSession, token: &str) -> Result<(), BridgeError> {
        let (kid, _signature) = token.split_once('.')
            .ok_or_else(|| BridgeError::unauthorized())?;

        // Determine which key to use
        let secret = if kid == self.config.current_kid {
            &self.config.current_secret
        } else if self.config.previous_kid.as_ref() == Some(&kid.to_string()) {
            self.config.previous_secret.as_ref()
                .ok_or_else(|| BridgeError::unauthorized())?
        } else {
            return Err(BridgeError::unauthorized());
        };

        let expected = self.generate_token_with_key(session, kid, secret)?;

        // Constant-time comparison
        if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
            return Err(BridgeError::unauthorized());
        }

        Ok(())
    }
}

/// Constant-time byte comparison
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}
```

### Key Storage

| Platform | Storage Location |
|----------|-----------------|
| Windows | DPAPI / Windows Credential Manager |
| macOS | Keychain |
| Linux | libsecret / gnome-keyring |

---

## Anti-Replay Protection

### Cached Response Semantics

When a duplicate `(session_id, request_id)` is detected within the replay window, return the **cached response** instead of re-executing. This provides:
- Replay protection (attackers can't force re-execution)
- Safe retry semantics (clients can retry without double-execution)
- Deterministic behavior (same request = same response)

```rust
use dashmap::DashMap;
use std::time::{Duration, Instant};

/// Replay cache with cached response return
pub struct ReplayCache {
    cache: DashMap<(String, String), CachedResponse>,
    window: Duration,
    max_entries: usize,
}

struct CachedResponse {
    response_bytes: Vec<u8>,
    cached_at: Instant,
}

pub enum ReplayCacheResult {
    /// Request is new, proceed with execution
    New,
    /// Request was seen before, return cached response
    Cached(Vec<u8>),
}

impl ReplayCache {
    pub fn new(window: Duration, max_entries: usize) -> Self {
        Self {
            cache: DashMap::new(),
            window,
            max_entries,
        }
    }

    /// Check if request is a replay; return cached response if so
    pub fn check(&self, session_id: &str, request_id: &str) -> ReplayCacheResult {
        let key = (session_id.to_string(), request_id.to_string());

        if let Some(entry) = self.cache.get(&key) {
            if entry.cached_at.elapsed() < self.window {
                return ReplayCacheResult::Cached(entry.response_bytes.clone());
            }
        }

        ReplayCacheResult::New
    }

    /// Store response for future replay detection
    pub fn store(&self, session_id: &str, request_id: &str, response_bytes: Vec<u8>) {
        let key = (session_id.to_string(), request_id.to_string());

        self.cache.insert(key, CachedResponse {
            response_bytes,
            cached_at: Instant::now(),
        });

        // Evict expired entries
        self.evict_expired();
    }

    fn evict_expired(&self) {
        self.cache.retain(|_, v| v.cached_at.elapsed() < self.window);

        // If still over limit, evict oldest
        while self.cache.len() > self.max_entries {
            let oldest = self.cache.iter()
                .min_by_key(|e| e.cached_at)
                .map(|e| e.key().clone());

            if let Some(key) = oldest {
                self.cache.remove(&key);
            } else {
                break;
            }
        }
    }
}
```

### Timestamp Validation

Requests with timestamps outside the allowed window are rejected:

```rust
const TIMESTAMP_SKEW_MS: i64 = 5 * 60 * 1000; // 5 minutes

fn validate_timestamp(request_ts: u64) -> Result<(), BridgeError> {
    let now = Utc::now().timestamp_millis();
    let diff = (now - request_ts as i64).abs();

    if diff > TIMESTAMP_SKEW_MS {
        Err(BridgeError::invalid_request("Request timestamp out of range"))
    } else {
        Ok(())
    }
}
```

### Replay Protection Configuration

| Parameter | Value |
|-----------|-------|
| Replay window | 5 minutes |
| Max cached entries per session | 1000 |
| Timestamp skew tolerance | +/- 5 minutes |

---

## Method Namespace and Authorization

### Reserved Namespaces

| Prefix | Owner | Description |
|--------|-------|-------------|
| `bridge.*` | Platform | Bridge lifecycle (ping, close) |
| `events.*` | Platform | Event subscription system |
| `system.*` | Platform | System information |
| `storage.*` | Platform | App-scoped storage |
| `resource.*` | Platform | Resource budget queries |
| `permission.*` | Platform | Permission requests |
| `shell.*` | Shell only | Shell-privileged operations |

### Authorization Table

```rust
pub struct MethodSpec {
    pub name: String,
    pub required_capabilities: Vec<String>,
    pub tier: PermissionTier,
    pub timeout_ms: u64,
    pub idempotent: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum PermissionTier {
    /// Always granted to all apps
    AlwaysGranted,
    /// Granted once at install, remembered
    GrantOnce,
    /// Prompt every time
    PromptAlways,
    /// Shell-only, never granted to apps
    ShellOnly,
}

pub struct MethodRegistry {
    methods: HashMap<String, MethodSpec>,
}

impl MethodRegistry {
    pub fn new() -> Self {
        let mut registry = Self { methods: HashMap::new() };

        // Bridge lifecycle
        registry.register("bridge.ping", &[], PermissionTier::AlwaysGranted, 100, true);

        // Storage (always granted, per-app scoped)
        registry.register("storage.get", &["storage:app"], PermissionTier::AlwaysGranted, 2000, true);
        registry.register("storage.set", &["storage:app"], PermissionTier::AlwaysGranted, 2000, true);
        registry.register("storage.delete", &["storage:app"], PermissionTier::AlwaysGranted, 2000, true);
        registry.register("storage.list", &["storage:app"], PermissionTier::AlwaysGranted, 2000, true);

        // System
        registry.register("system.get_time", &[], PermissionTier::AlwaysGranted, 100, true);
        registry.register("system.get_identity", &["system:identity:read"], PermissionTier::AlwaysGranted, 100, true);

        // Resource
        registry.register("resource.get_budget", &[], PermissionTier::AlwaysGranted, 100, true);
        registry.register("resource.get_storage_usage", &[], PermissionTier::AlwaysGranted, 100, true);
        registry.register("resource.request_quota_increase", &[], PermissionTier::PromptAlways, 5000, false);

        // Events
        registry.register("events.subscribe", &[], PermissionTier::AlwaysGranted, 500, true);
        registry.register("events.poll", &[], PermissionTier::AlwaysGranted, 30000, true);
        registry.register("events.unsubscribe", &[], PermissionTier::AlwaysGranted, 500, true);

        // Permission
        registry.register("permission.prepare_action", &[], PermissionTier::PromptAlways, 1000, true);
        registry.register("permission.execute_action", &[], PermissionTier::PromptAlways, 5000, false);

        // External (require prompt)
        registry.register("external.open_url", &["external:open_url"], PermissionTier::PromptAlways, 1000, false);
        registry.register("clipboard.write", &["clipboard:write"], PermissionTier::PromptAlways, 1000, false);

        registry
    }

    fn register(&mut self, name: &str, caps: &[&str], tier: PermissionTier, timeout_ms: u64, idempotent: bool) {
        self.methods.insert(name.to_string(), MethodSpec {
            name: name.to_string(),
            required_capabilities: caps.iter().map(|s| s.to_string()).collect(),
            tier,
            timeout_ms,
            idempotent,
        });
    }

    pub fn authorize(&self, method: &str, session: &AppSession) -> Result<&MethodSpec, BridgeError> {
        let spec = self.methods.get(method)
            .ok_or_else(|| BridgeError::invalid_request("Unknown method"))?;

        // Shell-only methods are never authorized for apps
        if matches!(spec.tier, PermissionTier::ShellOnly) {
            return Err(BridgeError::unauthorized());
        }

        // Check required capabilities
        for cap in &spec.required_capabilities {
            if !session.capabilities.contains(cap) {
                return Err(BridgeError::permission_denied(cap));
            }
        }

        Ok(spec)
    }
}
```

### Capability Naming Convention

Capabilities use a hierarchical naming scheme:

```
{domain}:{action}
{domain}:{subdomain}:{action}

Examples:
- storage:app          (app-scoped storage)
- clipboard:write      (write to clipboard)
- external:open_url    (open external URLs)
- contacts:read        (read contacts - if ever added)
```

**Non-wildcard rule**: Third-party apps must request specific capabilities, never wildcards.

---

## Error Taxonomy

### Error Codes

| Code | HTTP Equiv | Description | Retryable | When to Use |
|------|-----------|-------------|-----------|-------------|
| `INVALID_REQUEST` | 400 | Malformed CBOR, invalid envelope, unknown method | No | Parse errors, schema violations |
| `UNAUTHORIZED` | 401 | Session/token invalid (collapsed) | No | All auth failures |
| `PERMISSION_DENIED` | 403 | Valid session, missing capability | No | Capability check failures |
| `NOT_FOUND` | 404 | Resource not found | No | Missing data (generic) |
| `CONFLICT` | 409 | Version/state conflict | Yes | Optimistic concurrency |
| `RATE_LIMITED` | 429 | Too many requests | Yes | Rate limit exceeded |
| `PAYLOAD_TOO_LARGE` | 413 | Exceeds 256KB limit | No | Oversize request |
| `TIMEOUT` | 504 | Request timed out | Yes | Handler timeout |
| `INTERNAL_ERROR` | 500 | Unexpected backend error | Yes | Catchall for bugs |

### Security-Sensitive Error Handling

```rust
impl BridgeError {
    /// Collapse all auth failures into generic UNAUTHORIZED
    /// Prevents oracle attacks (session guessing, token probing)
    pub fn unauthorized() -> Self {
        Self {
            code: BridgeErrorCode::Unauthorized,
            message: "Unauthorized".to_string(),
            error_id: Some(generate_error_id()),
            details: None, // NEVER expose details
            retryable: false,
            retry_after_ms: None,
        }
    }

    pub fn rate_limited(retry_after_ms: u64) -> Self {
        Self {
            code: BridgeErrorCode::RateLimited,
            message: "Rate limit exceeded".to_string(),
            error_id: Some(generate_error_id()),
            details: None,
            retryable: true,
            retry_after_ms: Some(retry_after_ms),
        }
    }

    pub fn permission_denied(capability: &str) -> Self {
        Self {
            code: BridgeErrorCode::PermissionDenied,
            message: format!("Permission denied: {}", capability),
            error_id: Some(generate_error_id()),
            details: None,
            retryable: false,
            retry_after_ms: None,
        }
    }
}

/// Never expose in error messages:
/// - Filesystem paths
/// - Internal object IDs
/// - Stack traces
/// - Database details
/// - Other session IDs
```

---

## Timeout and Idempotency

### Timeout Configuration

```rust
pub struct TimeoutConfig {
    /// Global maximum timeout (30s)
    pub global_timeout: Duration,

    /// Per-method timeouts
    pub method_timeouts: HashMap<String, Duration>,
}

impl TimeoutConfig {
    pub fn get_timeout(&self, method: &str) -> Duration {
        self.method_timeouts.get(method)
            .copied()
            .unwrap_or(self.global_timeout)
            .min(self.global_timeout)
    }
}

// Default per-method timeouts
storage.* → 2 seconds
messaging.* → 10 seconds
events.poll → 30 seconds (long-poll)
bridge.ping → 100 milliseconds
```

### Request Processing with Timeout

```rust
impl Bridge {
    pub async fn handle_request(
        &self,
        app_id: &str,
        webview_label: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, BridgeError> {
        let start = Instant::now();

        // Parse and validate request
        let request: BridgeRequest = self.parse_request(&request_bytes)?;

        // Check replay cache
        match self.replay_cache.check(&request.session, &request.id) {
            ReplayCacheResult::Cached(response) => {
                return Ok(response);
            }
            ReplayCacheResult::New => {}
        }

        // Get method timeout
        let timeout = self.timeout_config.get_timeout(&request.method);

        // Execute with timeout
        let result = tokio::time::timeout(
            timeout,
            self.execute_request(&request, app_id, webview_label)
        ).await;

        let response = match result {
            Ok(Ok(result)) => BridgeResponse::success(request.id, result, start),
            Ok(Err(e)) => BridgeResponse::error(request.id, e, start),
            Err(_) => BridgeResponse::error(request.id, BridgeError::timeout(), start),
        };

        let response_bytes = self.encode_response(&response)?;

        // Cache response for replay protection
        self.replay_cache.store(&request.session, &request.id, response_bytes.clone());

        Ok(response_bytes)
    }
}
```

### Idempotency Semantics

- `response.id == request.id` always
- `request.id` serves as the default idempotency key
- Within the replay window, duplicate requests return cached response
- Optional explicit `idempotency_key` for client-controlled deduplication

---

## Event Subscriptions (Long-Poll)

Since the bridge uses only `postbridge_invoke`, backend-to-app push happens via **app-initiated long-polling**.

### Subscription Methods

```rust
// events.subscribe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeParams {
    pub topic: String,
    pub filter: Option<ciborium::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeResult {
    pub subscription_id: String,
}

// events.poll (long-poll)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollParams {
    pub subscription_id: String,
    pub after_seq: Option<u64>,
    pub timeout_ms: Option<u64>,  // Clamped to max 30000
    pub max_events: Option<usize>, // Default 100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollResult {
    pub events: Vec<SubscriptionEvent>,
    pub last_seq: u64,
    pub dropped: bool,  // Events were dropped due to buffer overflow
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionEvent {
    pub seq: u64,
    pub topic: String,
    pub payload: ciborium::Value,
    pub timestamp: u64,
}

// events.unsubscribe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeParams {
    pub subscription_id: String,
}
```

### Subscription Limits

| Limit | Value |
|-------|-------|
| Max subscriptions per session | 10 |
| Max pending events per subscription | 100 |
| Event TTL | 5 minutes |
| Poll timeout max | 30 seconds |
| Poll timeout min | 100 milliseconds |

### Subscription Management

```rust
pub struct SubscriptionManager {
    subscriptions: DashMap<String, Subscription>,
    config: SubscriptionConfig,
}

pub struct Subscription {
    pub id: String,
    pub session_id: String,
    pub app_id: String,
    pub topic: String,
    pub filter: Option<ciborium::Value>,
    pub created_at: Instant,
    pub pending_events: VecDeque<SubscriptionEvent>,
    pub next_seq: AtomicU64,
    pub dropped_count: AtomicU64,
}

impl SubscriptionManager {
    pub async fn poll(
        &self,
        subscription_id: &str,
        session_id: &str,
        after_seq: Option<u64>,
        timeout: Duration,
        max_events: usize,
    ) -> Result<PollResult, BridgeError> {
        let sub = self.subscriptions.get(subscription_id)
            .ok_or_else(|| BridgeError::not_found("Subscription not found"))?;

        // Verify session owns subscription
        if sub.session_id != session_id {
            return Err(BridgeError::unauthorized());
        }

        // Wait for events or timeout
        let deadline = Instant::now() + timeout;

        loop {
            let events = self.get_events_after(&sub, after_seq, max_events);

            if !events.is_empty() || Instant::now() >= deadline {
                let last_seq = events.last().map(|e| e.seq).unwrap_or(after_seq.unwrap_or(0));
                let dropped = sub.dropped_count.load(Ordering::Relaxed) > 0;

                return Ok(PollResult { events, last_seq, dropped });
            }

            // Short sleep before checking again
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
```

---

## Chunked Transfers

For payloads exceeding 256KB, use chunked transfer protocol.

### Transfer Methods

```rust
// Start large upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobPutStartParams {
    pub total_bytes: u64,
    pub sha256: Option<String>,  // Expected hash
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobPutStartResult {
    pub transfer_id: String,
    pub chunk_size: u64,  // Server-specified chunk size
}

// Upload chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobPutChunkParams {
    pub transfer_id: String,
    pub offset: u64,
    pub chunk: Vec<u8>,  // Raw bytes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobPutChunkResult {
    pub next_offset: u64,
    pub bytes_received: u64,
}

// Finish upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobPutFinishParams {
    pub transfer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobPutFinishResult {
    pub blob_id: String,
    pub total_bytes: u64,
    pub sha256: String,
}

// Download chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobGetChunkParams {
    pub blob_id: String,
    pub offset: u64,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobGetChunkResult {
    pub chunk: Vec<u8>,
    pub next_offset: u64,
    pub done: bool,
}
```

### Transfer Limits

| Limit | Value |
|-------|-------|
| Max total transfer size | 10 MB |
| Chunk size | 64 KB |
| Transfer timeout | 5 minutes |
| Max concurrent transfers per session | 3 |

---

## Security Considerations

### Threat Mitigations

#### Replay Attacks
- **Mitigation**: Request cache returns cached response for duplicate `(session, id)`
- **Additional**: Timestamp validation rejects requests outside +/- 5 minute window

#### TOCTOU (Time-of-Check-to-Time-of-Use)
- **Mitigation**: Two-step flow for prompted actions

```rust
// Step 1: Prepare action (creates pending action with exact params)
let pending = permission.prepare_action("clipboard:write", { text: "..." });
// Shell displays confirmation dialog with exact parameters
// User confirms

// Step 2: Execute with action token (only exact prepared params executed)
let result = permission.execute_action(pending.action_token);
```

#### Amplification
- **Mitigation**: Cap response sizes, require pagination
- List APIs return max 100 items per call
- Large payloads require chunked transfer

#### Enumeration
- **Mitigation**: Uniform `UNAUTHORIZED` for all auth failures
- Generic `NOT_FOUND` without resource type hints
- Avoid timing differences before auth checks

#### Parser Bombs
- **Mitigation**: CBOR limits enforced before parsing
- Reject indefinite lengths
- Max nesting depth, collection size, payload bytes

#### Flooding / DoS
- **Mitigation**: Token bucket rate limiting
- 50 rps sustained, 200 burst per session
- Max 16 concurrent in-flight requests

#### Confused Deputy
- **Mitigation**: Never accept `app_id` in request params
- Identity always derived from webview label
- Session bound to specific webview label

### Security Invariants

1. **App identity MUST come from infrastructure** (webview label), never from payload
2. **Token validation MUST use constant-time comparison**
3. **Error messages MUST NOT expose internal state** (paths, IDs, traces)
4. **Unknown methods MUST be rejected** (default deny)
5. **Shell-only methods MUST always reject app webviews**
6. **Rate limiting MUST apply before expensive operations**

---

## Acceptance Criteria

### Performance

- [ ] Handshake completes in < 100ms
- [ ] Bridge request latency < 50ms p95 (for simple methods)
- [ ] CBOR parse/validate < 5ms for 256KB payload

### Security

- [ ] Replay attacks return cached response (not re-execute)
- [ ] Token from wrong webview rejected
- [ ] Expired session rejected
- [ ] Timestamp outside window rejected
- [ ] Unknown method rejected
- [ ] Shell method from app rejected
- [ ] Rate limit returns `retry_after_ms`

### Correctness

- [ ] CBOR with invalid encoding rejected
- [ ] Indefinite-length CBOR rejected
- [ ] Payloads > 256KB rejected
- [ ] Duplicate map keys rejected
- [ ] Missing capability returns `PERMISSION_DENIED`
- [ ] Subscription long-poll respects timeout

---

## Test Cases

| Test | Setup | Expected Result |
|------|-------|-----------------|
| Valid request | Well-formed CBOR | Success response |
| Replay attack | Same `(session, id)` twice | Second returns cached response |
| Invalid CBOR | Malformed bytes | `INVALID_REQUEST` |
| Indefinite CBOR | Indefinite-length array | `INVALID_REQUEST` |
| Wrong webview | Token for `app-A` used from `app-B` | `UNAUTHORIZED` |
| Expired session | Request after session TTL | `UNAUTHORIZED` |
| Invalid token | Modified signature | `UNAUTHORIZED` |
| Old key ID | Token with rotated-out `kid` | `UNAUTHORIZED` |
| Missing capability | Call `clipboard.write` without permission | `PERMISSION_DENIED` |
| Rate limited | 300 requests in 1 second | `RATE_LIMITED` with `retry_after_ms` |
| Concurrent overflow | 20 in-flight requests | 4 queued/rejected |
| Payload too large | 512KB request | `PAYLOAD_TOO_LARGE` |
| Unknown method | `foo.bar` | `INVALID_REQUEST` |
| Shell method from app | `shell.launch_app` | `UNAUTHORIZED` |
| Long-poll timeout | Poll with 10s timeout, no events | Empty response after 10s |
| Long-poll with events | Events arrive during poll | Events returned immediately |
| TOCTOU prepare/execute | Prepare action, confirm, execute | Success |
| TOCTOU skip confirm | Execute without prepare | `UNAUTHORIZED` |
| Timestamp skew | Request ts = now - 10 minutes | `INVALID_REQUEST` |

---

## Implementation Checklist

### Phase 1: Core Protocol
- [ ] Implement strict CBOR validator with limits
- [ ] Define `BridgeRequest` / `BridgeResponse` structs
- [ ] Create `postbridge_invoke` Tauri command
- [ ] Wire webview label extraction

### Phase 2: Session Management
- [ ] Implement `SessionManager`
- [ ] Token generation with `kid` support
- [ ] Constant-time token validation
- [ ] Session binding to webview label
- [ ] Session expiration cleanup

### Phase 3: Anti-Replay
- [ ] Implement `ReplayCache` with cached response return
- [ ] Timestamp validation
- [ ] Bounded LRU eviction

### Phase 4: Method Registry
- [ ] Define method specifications
- [ ] Implement capability checking
- [ ] Per-method timeouts
- [ ] Default deny for unknown methods

### Phase 5: Rate Limiting
- [ ] Token bucket per session
- [ ] Concurrent request limiting
- [ ] `retry_after_ms` in responses

### Phase 6: Error Handling
- [ ] Implement error taxonomy
- [ ] Error ID generation
- [ ] Sensitive error collapsing

### Phase 7: Events System
- [ ] Subscription management
- [ ] Long-poll `events.poll`
- [ ] Buffer limits and drop policy

### Phase 8: Chunked Transfers
- [ ] Upload flow (`blob.put_*`)
- [ ] Download flow (`blob.get_chunk`)
- [ ] Transfer session management

### Phase 9: Security Hardening
- [ ] TOCTOU two-step flow
- [ ] Audit logging
- [ ] Key rotation support
- [ ] Security test suite
