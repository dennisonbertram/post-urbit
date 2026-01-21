# App Sandbox & Isolation Specification

## Overview

This specification defines how third-party apps are isolated from the shell and each other. The sandbox architecture uses **multi-webview isolation** for all untrusted apps, providing OS-level process isolation on all platforms.

### Security Principles

1. **Defense in Depth**: Multiple layers (CSP, capability files, navigation hooks, IPC lockdown)
2. **Identity from Infrastructure**: App identity derived from webview label, never from payload
3. **Least Privilege**: Apps get NO Tauri commands except the single bridge command
4. **Fail Closed**: Unknown requests are denied by default
5. **Audit Trail**: All permission-sensitive operations logged

### Related Documents

- ADR-003: Multi-webview Architecture Decision
- Domain 1: Shell Architecture (security hardening)
- Domain 3: Secure Bridge Protocol (IPC)
- Phase 0: Gating Spikes (validation)

---

## Critical Decision: Multi-webview

**Decision**: Use multi-webview for ALL untrusted/third-party apps.

See [ADR-003](../adrs/ADR-003-multiwebview-isolation.md) for full rationale.

**Summary**:
- Iframes share renderer process → renderer compromise = game over
- Multi-webview maps to native OS process isolation
- Higher memory (~50-350MB per webview) mitigated by LRU management
- Cap concurrent hot webviews to 3-5

---

## Webview Lifecycle API

### Data Models

```rust
/// Current run state of an app webview
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppRunState {
    Hot,   // Actively visible and rendered
    Warm,  // Hidden but webview still in memory
    Cold,  // Webview destroyed, state persisted
}

/// Represents a loaded app instance
#[derive(Debug, Clone)]
pub struct LoadedApp {
    pub app_id: String,
    pub label: String,         // Always "app-{app_id}"
    pub session_id: String,
    pub capabilities: Vec<String>,
    pub state: AppRunState,
    pub last_active: Instant,
    pub created_at: DateTime<Utc>,
}

/// Configuration for webview lifecycle
#[derive(Debug, Clone)]
pub struct WebviewLifecycleConfig {
    pub max_hot_webviews: usize,      // Default: 3
    pub max_warm_webviews: usize,     // Default: 5
    pub warm_timeout_secs: u64,       // Default: 300
    pub memory_pressure_threshold: f64, // Default: 0.85
}
```

### Tauri Commands (Shell-Only)

```rust
/// Launch an app, creating its webview
#[tauri::command]
pub async fn app_launch(
    app: AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
) -> Result<LaunchResult, String>;

/// Show an existing app (bring to front)
#[tauri::command]
pub async fn app_show(
    app: AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
) -> Result<(), String>;

/// Hide an app (keep in memory as warm)
#[tauri::command]
pub async fn app_hide(
    app: AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
) -> Result<(), String>;

/// Close an app (destroy webview)
#[tauri::command]
pub async fn app_close(
    app: AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
) -> Result<(), String>;
```

All commands verify `webview.label() == "shell"` before proceeding.

### Lifecycle Events

| Event | Payload | When Emitted |
|-------|---------|--------------|
| `app://lifecycle/created` | `{}` | Webview created |
| `app://lifecycle/shown` | `{}` | App brought to foreground |
| `app://lifecycle/hidden` | `{}` | App sent to background |
| `app://lifecycle/closed` | `{}` | Webview about to be destroyed |

---

## Custom Protocol Handler

### Protocol Format

```
postapp://{app_id}/{path}

Examples:
postapp://com.example.notes/index.html
postapp://com.example.notes/assets/icon.png
```

### Rust Implementation

```rust
pub fn register_postapp_protocol(
    builder: tauri::Builder<tauri::Wry>,
    apps_dir: PathBuf,
) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("postapp", move |_app, request| {
        handle_postapp_request(&apps_dir, request)
    })
}

fn handle_postapp_request(
    apps_dir: &PathBuf,
    request: &Request,
) -> Result<Response, Box<dyn std::error::Error>> {
    let uri = request.uri();
    let app_id = uri.host().unwrap_or_default();
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // Validate app_id format
    if !is_valid_app_id(app_id) {
        return error_response(400, "Invalid app ID");
    }

    // Construct and validate file path
    let file_path = apps_dir.join(app_id).join("ui").join(path);
    let canonical = file_path.canonicalize()?;
    let app_dir = apps_dir.join(app_id).canonicalize()?;

    // SECURITY: Prevent path traversal
    if !canonical.starts_with(&app_dir) {
        return error_response(403, "Path traversal blocked");
    }

    let content = std::fs::read(&canonical)?;
    let mime_type = mime_guess::from_path(&canonical)
        .first_or_octet_stream()
        .to_string();

    ResponseBuilder::new()
        .status(200)
        .header("Content-Type", &mime_type)
        .header("Content-Security-Policy", &build_csp(app_id))
        .header("X-Content-Type-Options", "nosniff")
        .header("Referrer-Policy", "no-referrer")
        .header("Permissions-Policy", PERMISSIONS_POLICY)
        .header("Cross-Origin-Opener-Policy", "same-origin")
        .header("Cross-Origin-Resource-Policy", "same-origin")
        .body(content)
}

fn is_valid_app_id(id: &str) -> bool {
    // Reverse DNS format: com.example.app
    let re = regex::Regex::new(r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$").unwrap();
    re.is_match(id) && id.len() <= 128
}
```

---

## CSP Header Specification

### Baseline CSP for All Apps

```
default-src 'none';
base-uri 'none';
form-action 'none';
frame-ancestors 'none';
object-src 'none';
script-src 'self' 'wasm-unsafe-eval';
style-src 'self' 'unsafe-inline';
img-src 'self' data: blob:;
font-src 'self';
media-src 'self' blob:;
connect-src 'none';
```

### CSP Builder

```rust
fn build_csp(app_id: &str) -> String {
    format!(
        "default-src 'none'; \
         base-uri 'none'; \
         form-action 'none'; \
         frame-ancestors 'none'; \
         object-src 'none'; \
         script-src 'self' 'wasm-unsafe-eval' postapp://{app_id}; \
         style-src 'self' 'unsafe-inline' postapp://{app_id}; \
         img-src 'self' data: blob: postapp://{app_id}; \
         font-src 'self' postapp://{app_id}; \
         media-src 'self' blob: postapp://{app_id}; \
         connect-src 'none'"
    )
}
```

### Additional Security Headers

| Header | Value | Purpose |
|--------|-------|---------|
| `X-Content-Type-Options` | `nosniff` | Prevent MIME sniffing |
| `Referrer-Policy` | `no-referrer` | Prevent referrer leakage |
| `Permissions-Policy` | `camera=(), microphone=(), ...` | Disable device APIs |
| `Cross-Origin-Opener-Policy` | `same-origin` | Prevent window.opener attacks |
| `Cross-Origin-Resource-Policy` | `same-origin` | Prevent cross-origin embedding |

---

## Navigation Policy

### Layer A: Webview Navigation Hook

```rust
pub fn configure_navigation_policy(webview: &Webview, app_id: &str) {
    let app_id = app_id.to_string();

    webview.on_navigation(move |url| {
        let allowed = url.scheme() == "postapp"
            && url.host_str() == Some(&app_id);

        if !allowed {
            log::warn!("Blocked navigation: app={}, url={}", app_id, url);
        }

        allowed
    });
}
```

### Layer B: Popup Policy

```rust
pub fn configure_popup_policy(webview: &Webview, app_id: &str) {
    let app_id = app_id.to_string();

    webview.on_new_window_request(move |url| {
        log::warn!("Blocked popup: app={}, url={}", app_id, url);
        false  // Always block
    });
}
```

### External Intent Handling

1. Navigation blocked at Layer A
2. Event emitted to shell: `app://navigation/external_requested`
3. Shell shows confirmation dialog
4. If confirmed, shell calls `shell_open_external_url`

---

## Tauri Capability Configuration

### Shell Capability File

```json
// src-tauri/capabilities/shell.json
{
  "identifier": "shell-capabilities",
  "description": "Full platform access for shell",
  "windows": ["shell"],
  "permissions": [
    "core:default",
    "shell:allow-open",
    "postbridge:allow-app-launch",
    "postbridge:allow-app-show",
    "postbridge:allow-app-hide",
    "postbridge:allow-app-close",
    "postbridge:allow-shell-*"
  ]
}
```

### App Capability File

```json
// src-tauri/capabilities/app-default.json
{
  "identifier": "app-default-capabilities",
  "description": "Minimal capabilities - bridge only",
  "windows": ["app-*"],
  "permissions": [
    "postbridge:allow-invoke"
  ]
}
```

### Bridge Command (Apps' ONLY IPC)

```rust
/// The ONLY command apps can call
#[tauri::command]
pub async fn postbridge_invoke(
    webview: Webview,
    state: State<'_, AppState>,
    request_bytes: Vec<u8>,
) -> Result<Vec<u8>, String> {
    // CRITICAL: Derive identity from webview label
    let label = webview.label();

    if !label.starts_with("app-") {
        return Err("Unauthorized: app webviews only".to_string());
    }

    let app_id = label.strip_prefix("app-")
        .ok_or("Invalid webview label")?;

    let request: BridgeRequest = serde_cbor::from_slice(&request_bytes)
        .map_err(|_| "Invalid request format")?;

    // Validate session matches webview-derived app_id
    let session = state.session_manager
        .validate_request(&request.session, &request.token, &request.id, request.ts)
        .await?;

    if session.app_id != app_id {
        return Err("Session/webview mismatch".to_string());
    }

    state.bridge_handler.handle_request(&request, &session).await
}
```

---

## External Interaction Policy Matrix

| Interaction | Policy | Implementation |
|-------------|--------|----------------|
| External HTTP fetch | BLOCKED | CSP `connect-src 'none'` |
| External WebSocket | BLOCKED | CSP `connect-src 'none'` |
| External script load | BLOCKED | CSP `script-src 'self'` |
| External navigation | BLOCKED + Event | Rust `on_navigation` |
| window.open() | BLOCKED | Rust `on_new_window_request` |
| Tauri invoke() | BLOCKED except bridge | Capability file |
| postMessage to parent | ALLOWED | Bridge communication |
| localStorage | ALLOWED (per-origin) | Origin isolation |
| IndexedDB | ALLOWED (per-origin) | Origin isolation |
| Clipboard read | BLOCKED | Permissions-Policy |
| Clipboard write | PROMPT via bridge | Permission required |
| Camera/Mic/Geo | BLOCKED | Permissions-Policy |

---

## Platform Considerations

| Feature | Windows (WebView2) | macOS (WKWebView) | Linux (WebKitGTK) |
|---------|-------------------|-------------------|-------------------|
| Process isolation | Per user data folder | Native multi-process | Native multi-process |
| CSP via headers | Full support | Full support | Full support |
| Origin isolation | Per custom scheme | Per custom scheme | Per custom scheme |
| Memory per webview | ~50-350MB | ~100-400MB | ~50-200MB |
| Creation latency | 500ms-2s | 200ms-500ms | 300ms-800ms |
| Crash containment | Yes | Yes | Yes |

---

## Acceptance Criteria

### Spike 0.4 Revised Pass Criteria

1. **No privileged command can be successfully invoked** from app webview
2. **Only `postbridge_invoke` works** with identity enforcement
3. **External navigation and popups blocked**

(Globals absence is nice-to-have, not required)

### Test Cases

| Test | Expected Result |
|------|-----------------|
| App calls `app_launch` | Error: "Unauthorized: shell only" |
| App calls `postbridge_invoke` with valid session | Success |
| App calls `postbridge_invoke` with spoofed app_id | Error: "Session/webview mismatch" |
| App navigates to external URL | Blocked |
| App calls window.open() | Returns null |
| App uses fetch() to external | CSP violation |
| Shell calls lifecycle commands | All succeed |
| App webview crashes | Shell remains responsive |

---

## Implementation Checklist

- [ ] Register `postapp://` protocol handler
- [ ] Implement CSP header injection
- [ ] Create capability files (shell + app-default)
- [ ] Implement `WebviewLifecycleManager`
- [ ] Create lifecycle commands
- [ ] Add LRU management
- [ ] Implement `postbridge_invoke`
- [ ] Add navigation hooks
- [ ] Add popup blocking
- [ ] Run Spike 0.4 tests
- [ ] Platform testing (Win/macOS/Linux)
