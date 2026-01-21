# Tauri Multi-Webview Research for App Sandboxing

## Executive Summary

**Recommendation: Multi-webview is viable and recommended** for true sandbox isolation of untrusted third-party apps.

Tauri 2.x supports multiple webviews in a single window via the unstable feature. This provides real process isolation on all platforms, solving the critical security concern that iframes share the same renderer process.

---

## 1. Tauri Multi-Window/Multi-Webview Support

**Status:** Tauri 2.x DOES support multiple webviews in a single window via the unstable feature.

### API Capabilities

**Rust API:**
```rust
// Create a webview within an existing window
let webview = WebviewBuilder::new("app-todo", tauri::WebviewUrl::App("/apps/todo".into()))
    .build()?;
window.add_child(webview, tauri::LogicalPosition::new(0.0, 50.0), tauri::LogicalSize::new(800.0, 600.0))?;
```

**JavaScript API:**
```typescript
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

// Create a new webview
const webview = new WebviewWindow('app-todo', {
  url: '/apps/todo',
  x: 0,
  y: 50,
  width: 800,
  height: 600
});
```

### Key Implementation Details

- Multiple webviews can be positioned and sized independently within a single window
- Each webview has a unique label (string identifier) for reference
- Official example: https://github.com/tauri-apps/tauri/tree/dev/examples/multiwebview
- Feature flag required: Enable "unstable" in Tauri dependency

**Limitation:** Windows will deadlock if webviews are created in synchronous command handlers - must use async commands.

---

## 2. Platform-Specific Process Isolation

### Windows (WebView2)

| Aspect | Behavior |
|--------|----------|
| **Process Model** | Browser process + renderer processes + helper processes |
| **Isolation Mechanism** | Site isolation per frame, user data folder association |
| **Key Insight** | Each `CoreWebView2Environment` (user data folder) gets its own process collection |
| **Multiple Instances** | Separate user data folders = separate process groups |

### macOS (WKWebView)

| Aspect | Behavior |
|--------|----------|
| **Process Model** | Native multi-process architecture by design |
| **Isolation** | WebKit renders in **separate processes** from main app |
| **Default Behavior** | Each WKWebView gets its own process space |
| **Configuration** | `WKProcessPool` allows explicit sharing if needed |

### Linux (WebKitGTK)

| Aspect | Behavior |
|--------|----------|
| **Process Model** | Multi-process by default (WebKit2 architecture) |
| **Isolation** | Web content runs in separate process |
| **Sandbox** | Linux namespaces sandbox enabled (WebKitGTK 6.0+) |
| **Cross-Site** | Mandatory process swapping on cross-site navigation |

**Summary:** All three platforms support native process isolation. This is the key insight - multi-webview gives us real OS-level isolation.

---

## 3. IPC Between Webviews

### Communication Patterns

**Pattern 1: Events (Fire-and-Forget)**
```rust
// Emit from Rust to specific webview
app.emit_to("app-todo", "message", payload)?;

// Emit from frontend
emit('message', payload);
```

**Pattern 2: Commands (Request-Response)**
```typescript
// Frontend calls Rust
const result = await invoke('storage_get', { appId: 'todo', key: 'tasks' });
```

### State Sharing

- All webviews access the same Tauri `State<T>` via commands
- State changes must route through Rust backend
- Webviews can emit events when state changes

### Per-Webview Permissions (Capabilities)

This is crucial for our security model:

```json
// capabilities/todo-app.json
{
  "identifier": "todo-app",
  "windows": ["app-todo"],
  "permissions": [
    "storage:read",
    "storage:write"
  ]
}
```

- A webview without matching capability has **zero IPC access**
- Different apps see different APIs based on their capability file
- Windows and webviews get **cumulative permissions**

---

## 4. Resource Implications

### Memory Overhead Per Webview

| Platform | Overhead | Notes |
|----------|----------|-------|
| **WebView2** | ~350 MB worst case, ~50-100 MB typical | Reported in Teams/Office |
| **WKWebView** | Very Heavy | 1.8-7x RAM vs native |
| **WebKitGTK** | Moderate | Lighter than WKWebView |

### Practical Limits

| Scenario | Recommendation |
|----------|----------------|
| **3-5 apps** | Manageable (500MB-2GB overhead) |
| **10+ apps** | Requires memory management strategy |
| **Long-running** | Need memory monitoring/garbage collection |

### Startup Time

- Tauri overall: ~500ms faster than Electron
- Per-webview creation: Platform-dependent, 500ms-5s range
- Some Windows-specific delays (2-5s webview render)

---

## 5. Comparison: Iframe vs Multi-Webview

| Aspect | Single Webview + Iframes | Multi-Webview |
|--------|--------------------------|---------------|
| **Process Isolation** | None | Full |
| **Crash Containment** | App crash = shell crash | Isolated crash |
| **Memory Usage** | Minimal | ~50-350MB per app |
| **Implementation** | Simple | Moderate |
| **API Isolation** | CSP-based (weak) | Capability-based (strong) |
| **Startup Time** | Instant | 500ms-5s per app |
| **Suitable for Untrusted Apps** | **No** | **Yes** |

---

## 6. Recommended Architecture

### Hybrid Approach (Best of Both)

1. **Hot Apps:** Keep 2-3 most-used apps as persistent webviews
2. **Warm Apps:** Cache webviews for last 5 accessed apps
3. **Cold Apps:** Lazy-load webviews on first interaction
4. **Memory Pressure:** Unload LRU apps when threshold hit

### Required Components

**1. App Registry System (Rust)**
```rust
pub struct AppRegistry {
    loaded_apps: HashMap<String, LoadedApp>,
    webview_labels: HashMap<String, String>,  // app_id -> webview_label
    memory_usage: HashMap<String, u64>,
}

pub struct LoadedApp {
    pub app_id: String,
    pub webview_label: String,
    pub permissions: Vec<String>,
    pub last_active: Instant,
    pub state: AppState,  // Hot, Warm, Cold
}
```

**2. Webview Lifecycle Manager**
```rust
#[tauri::command]
async fn create_app_webview(
    app: AppHandle,
    app_id: String,
    permissions: Vec<String>,
) -> Result<String, String> {
    let label = format!("app-{}", app_id);
    // Create webview with proper isolation
    // Return label for tracking
}

#[tauri::command]
async fn destroy_app_webview(app: AppHandle, label: String) -> Result<(), String> {
    // Clean up webview and resources
}
```

**3. Per-App Capability Files**
```
src-tauri/capabilities/
├── shell.json           # Full access for shell
├── app-todo.json        # storage:read, storage:write
├── app-chat.json        # messaging:*, contacts:read
└── app-calendar.json    # storage:*, notifications:show
```

**4. Memory Management**
```rust
pub struct MemoryManager {
    warning_threshold: f64,  // 70%
    cleanup_threshold: f64,  // 85%
    aggressive_threshold: f64, // 95%
}

impl MemoryManager {
    pub async fn check_and_cleanup(&self, registry: &mut AppRegistry) {
        let usage = self.get_system_memory_usage();
        if usage > self.cleanup_threshold {
            // Unload LRU apps
            self.unload_least_recently_used(registry).await;
        }
    }
}
```

---

## 7. Migration Path

### Phase 1: Foundation
- [ ] Add "unstable" feature to Tauri
- [ ] Implement webview creation/destruction API
- [ ] Create base capability files

### Phase 2: Shell Integration
- [ ] Update shell UI for webview-based app switching
- [ ] Implement app container component
- [ ] Add loading/error states

### Phase 3: Security
- [ ] Generate per-app capability files from manifests
- [ ] Implement session token system
- [ ] Add audit logging

### Phase 4: Resource Management
- [ ] Implement memory monitoring
- [ ] Add LRU unloading policy
- [ ] Create warm/cold app caching

### Phase 5: Testing
- [ ] Benchmark with 5+ concurrent apps
- [ ] Test crash isolation
- [ ] Memory profiling

---

## 8. Sources

- [Tauri Process Model](https://v2.tauri.app/concept/process-model/)
- [Tauri Capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri IPC](https://v2.tauri.app/concept/inter-process-communication/)
- [Tauri Multiwebview Example](https://github.com/tauri-apps/tauri/tree/dev/examples/multiwebview)
- [WebView2 Process Model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model)
- [WKWebView Process Isolation](https://developer.apple.com/documentation/webkit/wkprocesspool)
- [WebKitGTK Documentation](https://webkitgtk.org/)
- [WKWebView Memory Analysis](https://embrace.io/blog/wkwebview-memory-leaks/)
