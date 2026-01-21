## 1) Critical decision: multi-webview vs iframe (recommendation)

**Recommend: multi-webview for all untrusted / third‑party apps.**  
Use iframes only for **fully trusted, same-team** “micro-UI” that can share a renderer process (or for prototyping).

**Why (security boundary reality):**
- **Iframes share the same renderer process** → a renderer compromise, memory corruption, or side-channel class issue is “game over” for shell + all apps. CSP/sandbox helps but it’s not a comparable boundary.
- **Multi-webview maps to native multi-process isolation** on all target engines (WebView2 / WKWebView / WebKitGTK). This is the only option that matches “apps are untrusted” as a core assumption.

**Trade-off:** memory and creation time. Your Phase 0 spikes already recognize this; that’s the correct gating.

**Actionable ADR framing (Domain 2 ADR-003):**
- **Default:** multi-webview per app (process isolation).
- **Resource policy:** cap concurrent “hot” webviews (e.g., 3–5) and LRU-unload the rest (Domain 2.5).
- **Fallback:** if Phase 0.1/0.2/0.3 show custom-scheme limitations on any platform, keep multi-webview but change *content origin strategy* (e.g., `tauri://localhost` assets or loopback HTTP) — do **not** revert to iframes for untrusted apps.

---

## 2) Webview creation / lifecycle API design (Rust-owned, shell-triggered)

### Core principles
- **Only the shell** can create/destroy app webviews (capability restricted).
- **App identity is derived from the caller webview label** (never trust `app_id` fields passed from JS).
- **All app content must load from an app-scoped origin** (`postapp://{app_id}/...` or whatever wins Spike 0.1).

### Concrete Rust API (commands + internal manager)

**Data model (internal):**
```rust
use std::{collections::HashMap, time::Instant};

struct LoadedApp {
  app_id: String,
  label: String,            // "app-{app_id}"
  last_active: Instant,
  state: AppRunState,       // Hot/Warm/Cold
}

enum AppRunState { Hot, Warm, Cold }

struct AppWebviewManager {
  loaded: HashMap<String, LoadedApp>, // app_id -> loaded
}
```

**Command surface (shell only):**
```rust
#[derive(serde::Deserialize)]
struct LaunchOptions {
  app_id: String,
  bounds: (f64, f64, f64, f64), // x,y,w,h
  focus: bool,
}

#[tauri::command]
async fn app_launch(app: tauri::AppHandle, window: tauri::Window, opts: LaunchOptions)
  -> Result<String, String>
{
  // 1) validate app_id exists/installed
  // 2) enforce resource limits/LRU (Domain 2.5)
  // 3) create webview child with label "app-{app_id}"
  // 4) return label
}

#[tauri::command]
async fn app_show(window: tauri::Window, app_id: String) -> Result<(), String>;

#[tauri::command]
async fn app_hide(window: tauri::Window, app_id: String) -> Result<(), String>;

#[tauri::command]
async fn app_close(app: tauri::AppHandle, window: tauri::Window, app_id: String)
  -> Result<(), String>;
```

**Webview creation (implementation sketch):**
```rust
use tauri::{WebviewBuilder, WebviewUrl};

fn create_app_webview(window: &tauri::Window, app_id: &str, bounds: (f64,f64,f64,f64))
  -> Result<String, String>
{
  let label = format!("app-{}", app_id);
  let url = format!("postapp://{}/index.html", app_id)
    .parse().map_err(|e| e.to_string())?;

  let webview = WebviewBuilder::new(&label, WebviewUrl::External(url))
    // hardening (see sections below)
    .build()
    .map_err(|e| e.to_string())?;

  window.add_child(
    webview,
    tauri::LogicalPosition::new(bounds.0, bounds.1),
    tauri::LogicalSize::new(bounds.2, bounds.3),
  ).map_err(|e| e.to_string())?;

  Ok(label)
}
```

**Lifecycle events (needed for LRU + crash detection):**
- Emit to shell: `app://lifecycle/created`, `.../shown`, `.../hidden`, `.../closed`
- Track `last_active` on focus / pointer enter / “app heartbeat” pings.
- For crashes: treat “webview destroyed unexpectedly” as an event and allow relaunch.

**Platform delta to plan for (important for Phase 0.2):**
- If host-based origin partitioning is inconsistent anywhere, you will need **per-app engine storage separation** (e.g., separate WebView2 user-data folders). If Tauri doesn’t expose it cleanly, that becomes an explicit engineering constraint/ADR (and may require upstreaming/patching).

---

## 3) CSP header injection implementation (custom protocol)

### Implement in the `postapp://` protocol handler
You already have the correct pattern: `register_uri_scheme_protocol("postapp", ...)` and attach headers in the `ResponseBuilder`.

**Security requirements for the handler (do not skip):**
1. **Canonicalize and validate path** (block `..`, `%2e%2e`, backslashes, NUL, etc.).
2. **Enforce host/app_id mapping**: only serve files that belong to that app.
3. **Correct MIME types** (`nosniff` depends on it).

### Concrete header set (baseline)
Apply to **HTML and JS at minimum**; applying to all responses is fine.

Recommended baseline CSP for untrusted apps (tight network, no framing):
```
default-src 'none';
base-uri 'none';
form-action 'none';
frame-ancestors 'none';
object-src 'none';

script-src 'self';
style-src 'self';                /* avoid 'unsafe-inline' if possible */
img-src 'self' data: blob:;
font-src 'self';
media-src 'self' blob:;

connect-src 'none';              /* key exfiltration control */
```

Additional strongly recommended headers:
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: no-referrer`
- `Permissions-Policy: geolocation=(), microphone=(), camera=(), payment=(), usb=()`
- `Cross-Origin-Opener-Policy: same-origin` *(test carefully; can break some integrations)*
- `Cross-Origin-Resource-Policy: same-origin`

**Development mode exception:** you’ll likely need a dev-only CSP variant (e.g., allow `connect-src http://127.0.0.1:* ws://127.0.0.1:*` for HMR). Treat that as a build/profile switch, never in production packages.

**Reality check vs your Spike 0.3:** some webviews have quirks around CSP on custom schemes. Keep Spike 0.3 as written, but be prepared to fall back to:
- `<meta http-equiv="Content-Security-Policy" ...>` for HTML, and/or
- abandoning `postapp://` in favor of `tauri://localhost` assets or loopback HTTP **while retaining multi-webview**.

---

## 4) Navigation blocking strategies (top-level + popups + external intents)

You need **two layers**:

### Layer A — Webview navigation policy (top-level)
Enforce in Rust using the webview’s navigation hooks:
- Allow only:
  - `postapp://{app_id}/...` (or your chosen origin)
  - optionally `about:blank` (some flows need it; test and restrict)
- Block everything else: `http(s)`, `file`, `data`, `javascript`, custom schemes.

Implementation intent (based on your spike text):
```rust
webview.on_navigation(move |url| {
  url.scheme() == "postapp" && url.host_str() == Some(app_id)
});
```

### Layer B — New window / popup policy
Block *all* new-window creation from untrusted apps:
```rust
webview.on_new_window_request(|_url| false);
```

### Handling allowed “external intents” (mailto/tel/http link-outs)
Do **not** allow direct navigation. Instead:
1. Block the navigation.
2. Emit an event to the shell with the requested URL + app_id.
3. Shell decides (permission-gated + user gesture) whether to open via system handler.

This keeps the boundary in Rust/shell and avoids “open redirect → exfil” style tricks.

---

## 5) Tauri IPC lockdown specifics (what to do concretely)

### The invariant you actually need
Even if you manage to hide `__TAURI__`, the real security requirement is:

- **App webviews must not be able to invoke privileged commands/plugins directly.**
- **They may only call a single, minimal “bridge” command**, and *that* command enforces:
  - caller binding (webview label → app_id),
  - session/auth (Domain 3),
  - permissions (Domain 4),
  - rate limits/replay protection (Domain 3).

### Use Tauri 2.x Capabilities as the outer gate
Create per-webview capability files that allow only your bridge command for that webview label, and **no plugin permissions**.

Example shape (illustrative):
```json
{
  "identifier": "app-todo",
  "windows": ["app-todo"],              // must match the webview label
  "permissions": [
    "core:default",
    "postbridge:invoke"                 // your single command permission
  ]
}
```

Then ensure:
- Shell webview has broad permissions.
- App webviews do **not** have `fs`, `shell`, `dialog`, etc.

### Bind identity in Rust (do not trust payload)
Design the bridge command to derive identity from the invoking webview context:

```rust
#[tauri::command]
async fn postbridge_invoke(
  webview: tauri::WebviewWindow,
  state: tauri::State<'_, BridgeState>,
  cbor_request: Vec<u8>,
) -> Result<Vec<u8>, BridgeError> {
  let label = webview.label().to_string();
  let app_id = state.map_label_to_app_id(&label)?; // your registry mapping

  // ignore any app_id in request; enforce label binding
  state.handle_request(app_id, label, cbor_request).await
}
```

### About “disable tauri api injection”
Your Spike 0.4 assumes you can make `__TAURI__` undefined. In practice, depending on Tauri/webview internals, **you may not be able to guarantee the global is absent** on all platforms/versions.

What you *can* and *should* guarantee:
- **No capabilities → no useful IPC access**, even if internals exist.
- **Only the bridge command is callable**, and it is hardened.

So I would adjust Spike 0.4’s pass criteria from “globals are absent” to:
- “No privileged command can be successfully invoked”
- “Only `postbridge_invoke` works, and it enforces identity + permissions”
- “External navigation and popups are blocked”

(You can still keep “globals absent” as a *nice-to-have* check.)

---

### Bottom line implementation plan for Domain 2
- Commit to **multi-webview** for untrusted apps.
- Implement a **Rust-owned webview manager** + shell-only lifecycle commands.
- Serve all app assets through a **custom protocol** with **strict CSP + security headers** and **path/app scoping**.
- Enforce **navigation + popup blocking** in Rust.
- Lock IPC with **capabilities** so apps can reach **only** a single hardened bridge command, with identity derived from the **webview label**.

If you want, I can turn the above into an ADR-003/004 draft plus a “minimum viable capability set” table (shell vs app webviews) that maps directly onto Tauri capability JSON.
