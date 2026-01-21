# Post-Urbit Tauri Frontend Integration Plan

## Overview

This document provides a comprehensive implementation plan for building a Tauri-based frontend for Post-Urbit that embeds the existing Rust core, provides a React + shadcn + Tailwind shell UI, and hosts sandboxed third-party applications.

---

## 1. Project Structure

```
post-urbit/
├── post-urbit-core/              # Existing core (library)
│   ├── src/
│   └── Cargo.toml
│
└── post-urbit-desktop/           # New Tauri project
    ├── src/                      # React frontend
    │   ├── components/
    │   │   ├── shell/           # Shell UI components
    │   │   │   ├── Sidebar.tsx
    │   │   │   ├── TitleBar.tsx
    │   │   │   ├── AppContainer.tsx
    │   │   │   └── PermissionPrompt.tsx
    │   │   └── ui/              # shadcn components
    │   ├── stores/              # Zustand stores
    │   │   ├── appsStore.ts
    │   │   ├── identityStore.ts
    │   │   └── settingsStore.ts
    │   ├── hooks/
    │   ├── lib/
    │   ├── App.tsx
    │   └── main.tsx
    │
    ├── src-tauri/               # Tauri Rust backend
    │   ├── src/
    │   │   ├── main.rs
    │   │   ├── commands/        # Tauri command modules
    │   │   │   ├── mod.rs
    │   │   │   ├── apps.rs
    │   │   │   ├── bridge.rs
    │   │   │   ├── identity.rs
    │   │   │   └── settings.rs
    │   │   ├── protocol/        # Custom protocol handler
    │   │   │   ├── mod.rs
    │   │   │   └── postapp.rs
    │   │   ├── session/         # App session management
    │   │   │   ├── mod.rs
    │   │   │   └── token.rs
    │   │   ├── permissions/     # Permission enforcement
    │   │   │   ├── mod.rs
    │   │   │   └── registry.rs
    │   │   └── state.rs         # AppState definition
    │   ├── Cargo.toml
    │   └── tauri.conf.json
    │
    ├── package.json
    ├── tsconfig.json
    ├── vite.config.ts
    └── tailwind.config.js
```

### Cargo Workspace Configuration

```toml
# post-urbit/Cargo.toml (workspace root)
[workspace]
members = [
    "post-urbit-core",
    "post-urbit-desktop/src-tauri",
]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_cbor = "0.11"
```

```toml
# post-urbit-desktop/src-tauri/Cargo.toml
[package]
name = "post-urbit-desktop"
version = "0.1.0"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
post-urbit-core = { path = "../../post-urbit-core" }
tauri = { version = "2", features = ["unstable"] }
tauri-plugin-shell = "2"
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
serde_cbor = { workspace = true }
hmac = "0.12"
sha2 = "0.10"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
base64 = "0.21"
mime_guess = "2"
```

---

## 2. Rust Backend Architecture

### AppState Definition

```rust
// src-tauri/src/state.rs

use std::sync::Arc;
use tokio::sync::RwLock;
use post_urbit_core::{PostUrbitNode, RuntimeManager};

use crate::session::AppSessionManager;
use crate::permissions::PermissionRegistry;

pub struct AppState {
    /// Post-Urbit core node
    pub node: Arc<PostUrbitNode>,

    /// WASM runtime manager
    pub runtime: Arc<RwLock<RuntimeManager>>,

    /// App session manager (tokens, validation)
    pub session_manager: Arc<AppSessionManager>,

    /// Permission registry and enforcement
    pub permissions: Arc<PermissionRegistry>,

    /// Apps directory path
    pub apps_dir: std::path::PathBuf,

    /// Data directory path
    pub data_dir: std::path::PathBuf,
}

impl AppState {
    pub async fn new(data_dir: impl AsRef<std::path::Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let apps_dir = data_dir.join("apps");

        // Initialize Post-Urbit node
        let config = post_urbit_core::NodeConfig {
            port: 0,  // Use ephemeral port
            data_dir: data_dir.to_string_lossy().to_string(),
            bootstrap_peers: vec![],
            http_addr: "127.0.0.1:0".parse().unwrap(),
            metrics_enabled: false,
            admin_password_hash: None,
            admin_token_hash: None,
            session_secret: None,
            session_timeout_hours: 24,
        };
        let node = Arc::new(PostUrbitNode::new(config).await?);

        // Initialize runtime
        let mut runtime = RuntimeManager::new();
        if let Some(iid) = node.identity_manager().get_iid() {
            runtime.set_identity_iid(iid);
        }

        // Initialize session manager
        let session_config = crate::session::SessionManagerConfig::default();
        let session_manager = Arc::new(AppSessionManager::new(session_config));

        // Initialize permission registry
        let permissions = Arc::new(PermissionRegistry::new());

        Ok(Self {
            node,
            runtime: Arc::new(RwLock::new(runtime)),
            session_manager,
            permissions,
            apps_dir,
            data_dir,
        })
    }
}
```

### Tauri Command Definitions

#### Apps Commands

```rust
// src-tauri/src/commands/apps.rs

#[tauri::command]
pub async fn apps_list(state: State<'_, AppState>) -> Result<Vec<InstalledApp>, String> {
    // List installed apps from apps directory
}

#[tauri::command]
pub async fn apps_install_from_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<InstallResult, String> {
    // Download, verify, and install package
}

#[tauri::command]
pub async fn apps_install_from_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<InstallResult, String> {
    // Install from local .postapp file
}

#[tauri::command]
pub async fn apps_uninstall(
    state: State<'_, AppState>,
    app_id: String,
    keep_data: bool,
) -> Result<(), String> {
    // Uninstall app, optionally preserving data
}

#[tauri::command]
pub async fn apps_create_session(
    state: State<'_, AppState>,
    app_id: String,
) -> Result<CreateSessionResponse, String> {
    // Create new app session with token
    state.session_manager
        .create_session(&app_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apps_invalidate_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state.session_manager.invalidate_session(&session_id).await;
    Ok(())
}

#[tauri::command]
pub async fn apps_get_permissions(
    state: State<'_, AppState>,
    app_id: String,
) -> Result<AppPermissions, String> {
    // Get app's granted/denied permissions
}

#[tauri::command]
pub async fn apps_set_permissions(
    state: State<'_, AppState>,
    app_id: String,
    patch: PermissionPatch,
) -> Result<(), String> {
    // Grant or revoke permissions
}
```

#### Bridge Commands

```rust
// src-tauri/src/commands/bridge.rs

#[tauri::command]
pub async fn bridge_request(
    state: State<'_, AppState>,
    request_bytes: Vec<u8>,
) -> Result<Vec<u8>, String> {
    // Handle CBOR-encoded bridge request
    crate::bridge::handle_bridge_request(
        &request_bytes,
        &state.session_manager,
        &state.permissions,
        &state.runtime,
    ).await
    .map_err(|e| e.to_string())
}

// Individual bridge commands for type-safe access from shell
#[tauri::command]
pub async fn bridge_storage_get(
    state: State<'_, AppState>,
    token: String,
    key: String,
) -> Result<StorageGetResult, String> {
    // Validate token, check permissions, get storage
}

#[tauri::command]
pub async fn bridge_storage_set(
    state: State<'_, AppState>,
    token: String,
    key: String,
    value: Vec<u8>,
    expected_version: Option<u64>,
) -> Result<StorageSetResult, String> {
    // Validate token, check permissions, set storage
}

#[tauri::command]
pub async fn bridge_messaging_send(
    state: State<'_, AppState>,
    token: String,
    recipient: String,
    message_type: String,
    content: Vec<u8>,
) -> Result<MessageSendResult, String> {
    // Validate token, check permissions, send message
}

// ... additional bridge commands
```

#### Identity Commands

```rust
// src-tauri/src/commands/identity.rs

#[tauri::command]
pub async fn identity_get(state: State<'_, AppState>) -> Result<IdentityInfo, String> {
    // Get current identity info
}

#[tauri::command]
pub async fn identity_get_profile(state: State<'_, AppState>) -> Result<PublicProfile, String> {
    // Get public profile
}
```

### Custom Protocol Handler

```rust
// src-tauri/src/protocol/postapp.rs

use tauri::http::{Request, Response, ResponseBuilder};
use std::path::PathBuf;

pub fn register_postapp_protocol(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    tauri::protocol::register(app, "postapp", |request| {
        handle_postapp_request(request)
    })?;
    Ok(())
}

fn handle_postapp_request(request: &Request) -> Result<Response, Box<dyn std::error::Error>> {
    let url = request.uri();
    let app_id = url.host().unwrap_or_default();
    let path = url.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // Validate app_id format
    if !is_valid_app_id(app_id) {
        return ResponseBuilder::new()
            .status(400)
            .body(b"Invalid app ID".to_vec());
    }

    // Construct file path
    let apps_dir = get_apps_dir();
    let file_path = apps_dir.join(app_id).join(path);

    // Security: Validate path is within app directory
    let canonical = file_path.canonicalize()?;
    let app_dir = apps_dir.join(app_id).canonicalize()?;
    if !canonical.starts_with(&app_dir) {
        return ResponseBuilder::new()
            .status(403)
            .body(b"Path traversal blocked".to_vec());
    }

    // Read file
    let content = std::fs::read(&canonical)?;
    let mime_type = mime_guess::from_path(&canonical)
        .first_or_octet_stream()
        .to_string();

    // Build response with security headers
    ResponseBuilder::new()
        .status(200)
        .header("Content-Type", &mime_type)
        .header("X-Content-Type-Options", "nosniff")
        .header("Referrer-Policy", "no-referrer")
        .header("Content-Security-Policy", &build_csp(app_id))
        .header("Permissions-Policy", "camera=(), microphone=(), geolocation=()")
        .body(content)
}

fn build_csp(app_id: &str) -> String {
    format!(
        "default-src 'self' postapp://{app_id}; \
         script-src 'self' 'wasm-unsafe-eval' postapp://{app_id}; \
         style-src 'self' 'unsafe-inline' postapp://{app_id}; \
         img-src 'self' data: blob: postapp://{app_id}; \
         connect-src 'none'; \
         frame-ancestors 'self'; \
         form-action 'self'; \
         base-uri 'none'"
    )
}

fn is_valid_app_id(app_id: &str) -> bool {
    // Reverse DNS format: com.example.app
    let re = regex::Regex::new(r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$").unwrap();
    re.is_match(app_id) && app_id.len() <= 128
}
```

### Permission Enforcement

```rust
// src-tauri/src/permissions/registry.rs

pub struct PermissionRegistry {
    method_to_capability: HashMap<String, Vec<String>>,
    permission_tiers: HashMap<String, PermissionTier>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PermissionTier {
    AlwaysGranted,
    PromptOnce,
    PromptAlways,
    SystemOnly,
}

impl PermissionRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            method_to_capability: HashMap::new(),
            permission_tiers: HashMap::new(),
        };

        // Storage - always granted
        registry.register("storage.get", &["storage:app"], PermissionTier::AlwaysGranted);
        registry.register("storage.set", &["storage:app"], PermissionTier::AlwaysGranted);
        registry.register("storage.delete", &["storage:app"], PermissionTier::AlwaysGranted);
        registry.register("storage.list", &["storage:app"], PermissionTier::AlwaysGranted);

        // System
        registry.register("system.get_time", &[], PermissionTier::AlwaysGranted);
        registry.register("system.get_identity", &["system:identity:read"], PermissionTier::AlwaysGranted);
        registry.register("system.get_app_info", &[], PermissionTier::AlwaysGranted);
        registry.register("system.get_random", &["system:random"], PermissionTier::PromptOnce);

        // Contacts
        registry.register("contacts.list", &["contacts:read"], PermissionTier::PromptOnce);

        // Messaging
        registry.register("messaging.send", &["messaging:send"], PermissionTier::PromptAlways);
        registry.register("messaging.subscribe", &["messaging:subscribe"], PermissionTier::PromptOnce);

        // Notifications
        registry.register("notifications.show", &["notifications:show"], PermissionTier::PromptOnce);

        // Inter-app
        registry.register("app.invoke", &["app:invoke:*"], PermissionTier::PromptOnce);

        registry
    }

    pub fn check_permission(
        &self,
        method: &str,
        granted: &[String],
    ) -> PermissionCheckResult {
        let Some(required) = self.method_to_capability.get(method) else {
            return PermissionCheckResult::Denied("Unknown method".to_string());
        };

        if required.is_empty() {
            return PermissionCheckResult::Allowed;
        }

        let missing: Vec<String> = required.iter()
            .filter(|cap| !granted.contains(cap))
            .cloned()
            .collect();

        if missing.is_empty() {
            PermissionCheckResult::Allowed
        } else {
            let tier = self.get_tier(&missing);
            match tier {
                PermissionTier::SystemOnly => {
                    PermissionCheckResult::Denied("System-only".to_string())
                }
                _ => PermissionCheckResult::NeedsPrompt { capabilities: missing, tier }
            }
        }
    }
}
```

---

## 3. Frontend Architecture

### React Component Hierarchy

```
App
├── ShellProvider (context)
│   ├── TitleBar
│   │   ├── WindowControls
│   │   └── StatusIndicators
│   ├── MainLayout
│   │   ├── Sidebar
│   │   │   ├── NavigationItems
│   │   │   ├── AppList
│   │   │   └── UserProfile
│   │   └── ContentArea
│   │       ├── Router
│   │       │   ├── Dashboard
│   │       │   ├── Contacts
│   │       │   ├── Settings
│   │       │   └── AppStore
│   │       └── AppContainer (when app open)
│   │           ├── AppHeader
│   │           └── SandboxedIframe
│   └── OverlayManager
│       ├── PermissionPrompt
│       ├── NotificationCenter
│       └── CommandPalette
└── ErrorBoundary
```

### Zustand Store Example

```typescript
// src/stores/appsStore.ts
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

interface AppsStore {
  apps: InstalledApp[];
  activeApp: string | null;
  activeSessions: Map<string, AppSession>;
  loading: boolean;
  error: string | null;

  fetchApps: () => Promise<void>;
  openApp: (appId: string) => Promise<void>;
  closeApp: (appId: string) => void;
  installApp: (source: { type: string; value: string }) => Promise<InstalledApp>;
  uninstallApp: (appId: string, keepData?: boolean) => Promise<void>;
  grantPermissions: (appId: string, permissions: string[]) => Promise<void>;
}

export const useAppsStore = create<AppsStore>((set, get) => ({
  apps: [],
  activeApp: null,
  activeSessions: new Map(),
  loading: false,
  error: null,

  fetchApps: async () => {
    set({ loading: true, error: null });
    try {
      const apps = await invoke<InstalledApp[]>('apps_list');
      set({ apps, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  openApp: async (appId) => {
    const { activeSessions } = get();

    if (!activeSessions.has(appId)) {
      const session = await invoke<AppSession>('apps_create_session', { appId });
      const newSessions = new Map(activeSessions);
      newSessions.set(appId, session);
      set({ activeSessions: newSessions });
    }

    set({ activeApp: appId });
  },

  closeApp: (appId) => {
    const { activeApp, activeSessions } = get();
    const newSessions = new Map(activeSessions);
    newSessions.delete(appId);

    set({
      activeApp: activeApp === appId ? null : activeApp,
      activeSessions: newSessions,
    });
  },

  // ... other methods
}));
```

### AppContainer Component

```typescript
// src/components/shell/AppContainer.tsx
import { useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppsStore } from '@/stores/appsStore';

export function AppContainer({ appId }: { appId: string }) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const channelRef = useRef<MessageChannel | null>(null);
  const { activeSessions } = useAppsStore();

  const session = activeSessions.get(appId);

  useEffect(() => {
    if (!session || !iframeRef.current) return;

    // Create MessageChannel
    const channel = new MessageChannel();
    channelRef.current = channel;

    // Handle messages from app
    channel.port1.onmessage = async (event) => {
      const response = await invoke<number[]>('bridge_request', {
        requestBytes: Array.from(new Uint8Array(event.data)),
      });
      channel.port1.postMessage(new Uint8Array(response).buffer);
    };
    channel.port1.start();

    // Send handshake when iframe loads
    const handleLoad = () => {
      iframeRef.current?.contentWindow?.postMessage(
        {
          type: 'post_urbit_handshake',
          version: 1,
          session_id: session.sessionId,
          token: session.token,
          app_id: appId,
          capabilities: session.capabilities,
        },
        '*',
        [channel.port2]
      );
    };

    iframeRef.current.addEventListener('load', handleLoad);

    return () => {
      channel.port1.close();
      iframeRef.current?.removeEventListener('load', handleLoad);
    };
  }, [session, appId]);

  if (!session) {
    return <div>Loading...</div>;
  }

  return (
    <div className="flex-1 relative">
      <iframe
        ref={iframeRef}
        src={`postapp://${appId}/index.html`}
        sandbox="allow-scripts allow-same-origin"
        className="w-full h-full border-0"
      />
    </div>
  );
}
```

---

## 4. Security Implementation

### CSP Enforcement

CSP headers are injected in the protocol handler (shown above). Key directives:

- `default-src 'self'` - Only allow same-origin resources
- `connect-src 'none'` - **Block all network requests**
- `script-src 'wasm-unsafe-eval'` - Allow WASM but no eval
- `frame-ancestors 'self'` - Prevent embedding elsewhere
- `form-action 'self'` - Block form submissions to external URLs

### Navigation Blocking

```typescript
// In AppContainer
useEffect(() => {
  const iframe = iframeRef.current;
  if (!iframe) return;

  const handleLoad = () => {
    try {
      const currentUrl = iframe.src;
      if (!currentUrl.startsWith(`postapp://${appId}`)) {
        // Navigation attempt - reload proper content
        iframe.src = `postapp://${appId}/index.html`;
      }
    } catch (e) {
      // Cross-origin error = app tried to navigate elsewhere
      iframe.src = `postapp://${appId}/index.html`;
    }
  };

  iframe.addEventListener('load', handleLoad);
  return () => iframe.removeEventListener('load', handleLoad);
}, [appId]);
```

### Audit Logging

```rust
// src-tauri/src/audit.rs

pub struct AuditEntry {
    pub timestamp: String,
    pub app_id: String,
    pub method: String,
    pub capability_used: Option<String>,
    pub result: AuditResult,
}

pub enum AuditResult {
    Allowed,
    Denied { reason: String },
    PromptShown { granted: bool },
}

pub struct AuditLogger {
    entries: tokio::sync::Mutex<Vec<AuditEntry>>,
    max_entries: usize,
}

impl AuditLogger {
    pub async fn log(&self, entry: AuditEntry) {
        let mut entries = self.entries.lock().await;
        entries.push(entry);
        while entries.len() > self.max_entries {
            entries.remove(0);
        }
    }
}
```

---

## 5. Build & Development

### Development Workflow

```bash
# Terminal 1: Start Vite dev server
npm run dev

# Terminal 2: Start Tauri dev mode
npm run tauri dev
```

### Build Process

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "tauri": "tauri",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build",
    "test": "vitest",
    "test:e2e": "playwright test"
  }
}
```

### Testing Strategy

**Unit Tests (Rust)**
- Token generation/validation
- Permission checking
- Path traversal prevention

**Integration Tests (TypeScript)**
- Bridge request/response
- Session lifecycle
- Store actions

**E2E Tests (Playwright)**
- App installation flow
- Permission prompts
- App lifecycle

---

## 6. Phase Breakdown

### Phase 1: Foundation (Weeks 1-3)
**Milestone:** Basic Tauri shell with embedded core

- [ ] Set up Tauri project scaffold
- [ ] Integrate post-urbit-core
- [ ] Create basic shell UI
- [ ] Implement `postapp://` protocol handler
- [ ] Add shell authentication

**Testable:** App launches, can serve static files via protocol

### Phase 2: App Container (Weeks 4-5)
**Milestone:** Sandboxed app hosting with bridge

- [ ] Implement iframe sandbox
- [ ] Build CSP header injection
- [ ] Create postMessage bridge router
- [ ] Implement token generation
- [ ] Build AppContainer component
- [ ] Implement storage bridge commands

**Testable:** Can load test app, read/write storage

### Phase 3: Permission System (Weeks 6-7)
**Milestone:** Full permission enforcement

- [ ] Build PermissionRegistry
- [ ] Implement permission checking
- [ ] Create PermissionPrompt UI
- [ ] Add audit logging
- [ ] Persist permission grants

**Testable:** Apps prompted for permissions, grants persisted

### Phase 4: Full API Surface (Weeks 8-10)
**Milestone:** Complete bridge API

- [ ] Messaging commands
- [ ] Contacts commands
- [ ] System commands
- [ ] App-to-app invocation
- [ ] Real-time subscriptions
- [ ] Notifications

**Testable:** App can send messages, read contacts, invoke other apps

### Phase 5: App Management UI (Weeks 11-12)
**Milestone:** Complete app lifecycle

- [ ] AppLauncher component
- [ ] Installation flow
- [ ] Update detection
- [ ] Settings panel
- [ ] App store browsing

**Testable:** Can install/uninstall apps from URL

### Phase 6: SDK & Dev Tools (Weeks 13-14)
**Milestone:** Developer-ready SDK

- [ ] Package @post-urbit/sdk
- [ ] SDK documentation
- [ ] create-post-urbit-app CLI
- [ ] Example apps
- [ ] Developer mode logging

**Testable:** Can scaffold and run new app with CLI

### Phase 7: Polish & Production (Weeks 15-16)
**Milestone:** Production-ready release

- [ ] Performance optimization
- [ ] Error handling
- [ ] Platform packaging
- [ ] CI/CD pipeline
- [ ] Security audit

**Testable:** Bundle < 30MB, startup < 2s, all platforms build

---

## Key Integration Points with Existing Code

| Component | Existing File | Integration Approach |
|-----------|---------------|---------------------|
| WASM Runtime | `runtime_wasm.rs` | Wrap in Tauri state, expose via bridge |
| Package System | `app_store.rs` | Reuse parse/verify/install functions |
| Node | `node.rs` | Embed in AppState, start on launch |
| Auth Patterns | `admin_auth.rs` | Adapt for app session tokens |
| Capabilities | `runtime.rs` | Map to frontend permission checks |
