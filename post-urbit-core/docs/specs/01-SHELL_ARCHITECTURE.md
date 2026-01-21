# Shell Architecture Specification

## Overview

The shell is the privileged system UI for Post-Urbit, hosting apps in isolated webviews and providing platform services. **Any XSS in the shell becomes full platform compromise** - this is the most security-critical component.

### Architecture Principles

1. **Defense in Depth** - Multiple security layers, assume any one can fail
2. **Rust Authoritative** - All app lifecycle, sessions, and permissions enforced in Rust
3. **UI-Only Shell** - Shell webview handles rendering only, not message brokering
4. **Minimal Attack Surface** - Lock down Tauri commands to shell-only webview
5. **Accessibility First** - Full keyboard navigation and screen reader support

### Related ADRs

- ADR-001: State Management Approach (Zustand)
- ADR-002: Component Library Constraints (shadcn/Tailwind)
- ADR-003: Shell Navigation Model (state-based)
- ADR-004: Windowing/Layout Paradigm
- ADR-005: Privilege Boundary Placement
- ADR-006: Theming Contract
- ADR-007: Command System & Shortcuts
- ADR-008: Asset/Manifest Rendering Policy

---

## Component Hierarchy

```
<AppRoot>
  <ErrorBoundaryRoot fallback={<SafeModeRecovery />}>
    <A11yProviders>
      <FocusManager>
      <ReducedMotionProvider>
      <HighContrastProvider>
    </A11yProviders>
    <ThemeProvider defaultTheme="system" tokens={designTokens}>
      <ShellLayout>
        <TitleBar decorations={platform.titleBar}>
          <WindowControls />
          <DragRegion />
          <StatusIndicators>
            <ConnectivityIndicator />
            <SyncIndicator />
            <IdentityBadge />
          </StatusIndicators>
        </TitleBar>

        <Sidebar collapsible defaultWidth={240}>
          <AppLauncherButton />
          <NavItems>
            <NavItem to="home" icon={Home} />
            <NavItem to="settings" icon={Settings} />
          </NavItems>
          <Divider />
          <RunningAppsList />
          <UserProfile compact />
        </Sidebar>

        <MainArea>
          <TopBar>
            <BreadcrumbsOrTabs />
            <GlobalSearchTrigger shortcut="Cmd+K" />
            <StatusIndicators />
          </TopBar>

          <Workspace>
            <WindowManager>
              <AppSurfaceSlot windowId={id}>
                {/* Multi-webview integration point */}
              </AppSurfaceSlot>
            </WindowManager>
          </Workspace>
        </MainArea>

        <SystemOverlays portal>
          <NotificationCenter />
          <ToastContainer />
          <PermissionPromptModal />
          <ConfirmDialog />
          <AppInstallFlowModal />
          <DiagnosticsModal />
          <CommandPalette shortcut="Cmd+K" />
        </SystemOverlays>
      </ShellLayout>
    </ThemeProvider>
  </ErrorBoundaryRoot>
</AppRoot>
```

### Component Responsibilities

| Component | Responsibility |
|-----------|----------------|
| `ErrorBoundaryRoot` | Catch all unhandled errors, provide safe mode recovery |
| `A11yProviders` | Focus management, reduced motion, high contrast support |
| `ThemeProvider` | CSS variable theming with design tokens |
| `TitleBar` | Custom title bar with window controls and status |
| `Sidebar` | Navigation, app launcher, running apps list |
| `WindowManager` | Manages app surfaces, focus, z-order |
| `SystemOverlays` | Portal root for all modals/toasts |
| `CommandPalette` | Global command palette (Cmd+K) |

---

## State Management Specification

### Architecture: Rust Authoritative, Zustand for UI

- **Rust owns**: App lifecycle, webview creation/destruction, permissions, sessions, resource usage
- **Shell store owns**: Layout, focus, UI preferences, transient prompts

### Zustand Slice Architecture

```typescript
// Store Structure
interface ShellStore {
  ui: UISlice;
  windows: WindowsSlice;
  apps: AppsSlice;
  permissions: PermissionsSlice;
  notifications: NotificationsSlice;
  connectivity: ConnectivitySlice;
}

// UI Slice - Pure UI state
interface UISlice {
  theme: 'light' | 'dark' | 'system';
  sidebarCollapsed: boolean;
  sidebarWidth: number;
  commandPaletteOpen: boolean;
  reducedMotion: boolean;
  highContrast: boolean;
  setTheme: (theme: Theme) => void;
  toggleSidebar: () => void;
  openCommandPalette: () => void;
}

// Windows Slice - Mirrors Rust state
interface WindowsSlice {
  windows: Map<string, WindowState>;
  focusedWindowId: string | null;
  zOrder: string[];
  _syncFromRust: (payload: WindowsPayload) => void;
}

// Apps Slice - Mirrors Rust state
interface AppsSlice {
  installed: Map<string, InstalledApp>;
  running: Map<string, RunningSession>;
  loading: Set<string>;
  _syncFromRust: (payload: AppsPayload) => void;
}

// Permissions Slice - UI state for prompts
interface PermissionsSlice {
  pendingPrompts: PermissionPrompt[];
  enqueuePrompt: (prompt: PermissionPrompt) => void;
  resolvePrompt: (id: string, granted: string[]) => void;
}

// Notifications Slice
interface NotificationsSlice {
  toasts: Toast[];
  notifications: Notification[];
  unreadCount: number;
  showToast: (toast: Toast) => void;
  dismissToast: (id: string) => void;
}

// Connectivity Slice - Synced from Rust
interface ConnectivitySlice {
  online: boolean;
  syncStatus: 'synced' | 'syncing' | 'offline';
  lastSync: Date | null;
  _syncFromRust: (payload: ConnectivityPayload) => void;
}
```

### Event-Driven Sync from Rust

```typescript
import { listen } from '@tauri-apps/api/event';

async function initializeRustSync() {
  // Apps lifecycle
  await listen('shell://apps/installed_changed', (event) => {
    useShellStore.getState().apps._syncFromRust(event.payload);
  });

  await listen('shell://apps/running_changed', (event) => {
    useShellStore.getState().apps._syncFromRust(event.payload);
  });

  // Windows
  await listen('shell://windows/changed', (event) => {
    useShellStore.getState().windows._syncFromRust(event.payload);
  });

  // Permission prompts
  await listen('shell://permissions/prompt', (event) => {
    useShellStore.getState().permissions.enqueuePrompt(event.payload);
  });

  // Connectivity
  await listen('shell://connectivity/changed', (event) => {
    useShellStore.getState().connectivity._syncFromRust(event.payload);
  });
}
```

---

## Security Hardening

### A. Tauri Command Lockdown

ALL privileged commands verify caller webview label:

```rust
#[tauri::command]
async fn shell_only_command(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // CRITICAL: Verify caller is shell
    if webview.label() != "shell" {
        log::warn!("Unauthorized command attempt from: {}", webview.label());
        return Err("Unauthorized: shell only".to_string());
    }
    // Proceed with command
    Ok(())
}
```

### B. Shell CSP Policy

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';
img-src 'self' data: blob:;
font-src 'self';
connect-src 'self' tauri:;
frame-src 'none';
frame-ancestors 'none';
base-uri 'none';
form-action 'self';
object-src 'none';
```

**Notes:**
- `'unsafe-inline'` for style-src only (Tailwind)
- No `'unsafe-eval'` anywhere
- No frame-src (shell doesn't embed frames)

### C. Manifest Rendering Policy

| Content | Policy |
|---------|--------|
| App name | Plain text, max 64 chars, HTML-escaped |
| Description | Plain text, max 256 chars, HTML-escaped |
| Icons | PNG/WebP only, **SVG BANNED**, max 512KB |
| Reasons | Plain text, max 128 chars per permission |

**NEVER use `dangerouslySetInnerHTML` with app-provided content.**

### D. External URL Policy

ALL external URLs routed through Rust:

```rust
#[tauri::command]
async fn shell_open_external_url(
    webview: Webview,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    verify_shell_only(&webview)?;

    let parsed = url::Url::parse(&url)?;

    match parsed.scheme() {
        "https" => {
            // Allowed - open in system browser
            shell::open(url)?;
            audit_log("external_url_opened", &url);
            Ok(())
        }
        "mailto" | "tel" => {
            // Requires explicit permission prompt
            Err("Scheme requires permission".to_string())
        }
        _ => {
            // BLOCKED
            Err("Scheme not allowed".to_string())
        }
    }
}
```

### E. DevTools Policy

- **DISABLED** in production builds
- Build-time flag: `ENABLE_DEVTOOLS=1` for dev/nightly only
- Configure in `tauri.conf.json`:
  ```json
  "app": {
    "windows": [{
      "devtools": false
    }]
  }
  ```

---

## Accessibility Requirements

### Keyboard Navigation

| Shortcut | Action |
|----------|--------|
| Tab | Navigate forward through focusable elements |
| Shift+Tab | Navigate backward |
| Escape | Close current overlay or unfocus app |
| Cmd/Ctrl+K | Open command palette |
| F6 | Cycle focus between major regions |
| Arrow keys | Navigate within lists |

### Focus Management

- Visible focus ring on all interactive elements
- Focus trap in modals
- Focus restore after modal close
- App surface focus indicated distinctly from shell focus

### Screen Reader Support

- All regions labeled with `aria-label`
- Live regions for toasts (`aria-live="polite"`)
- Permission prompts fully announced
- App surface marked as `role="application"`

### Acceptance Criteria

- [ ] User can operate permission prompt fully via keyboard while app has focus
- [ ] All interactive elements have visible focus indicators
- [ ] Screen reader announces all system notifications
- [ ] Reduced motion preference respected (prefers-reduced-motion)

---

## tauri.conf.json Specification

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "identifier": "com.posturbit.desktop",
  "build": {
    "devUrl": "http://localhost:5173",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Post-Urbit",
        "label": "shell",
        "width": 1200,
        "height": 800,
        "minWidth": 800,
        "minHeight": 600,
        "center": true,
        "decorations": true,
        "devtools": false
      }
    ],
    "security": {
      "capabilities": ["shell-only"],
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' tauri:; frame-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'; object-src 'none'"
    }
  },
  "plugins": {
    "shell": {
      "open": "^https://"
    }
  }
}
```

### Capability File (capabilities/shell-only.json)

```json
{
  "identifier": "shell-only",
  "description": "Full access for shell webview only",
  "windows": ["shell"],
  "permissions": [
    "core:default",
    "shell:allow-open"
  ]
}
```

---

## Data Models

### TypeScript Interfaces

```typescript
// Window State
interface WindowState {
  id: string;
  appId: string;
  title: string;
  bounds: { x: number; y: number; width: number; height: number };
  state: 'normal' | 'minimized' | 'maximized';
  focused: boolean;
  webviewLabel: string;
}

// Installed App (mirrors Rust)
interface InstalledApp {
  id: string;
  name: string;
  version: string;
  authorIid: string;
  authorName: string | null;
  description: string;
  icon: string | null; // base64 PNG data URL
  installedAt: string;
  lastOpened: string | null;
  updateAvailable: string | null;
  status: 'installed' | 'running' | 'disabled' | 'error';
  permissions: AppPermissions;
  storageUsed: number;
  storageQuota: number;
}

// Running Session
interface RunningSession {
  sessionId: string;
  appId: string;
  webviewLabel: string;
  capabilities: string[];
  createdAt: string;
  lastActivity: string;
}

// Permission Prompt
interface PermissionPrompt {
  id: string;
  appId: string;
  appName: string;
  appIcon: string | null;
  permissions: RequestedPermission[];
  reasons: Record<string, string>;
  createdAt: string;
}

interface RequestedPermission {
  capability: string;
  displayName: string;
  description: string;
  tier: 'always_granted' | 'prompt_once' | 'prompt_always' | 'system_only';
}

// Toast Notification
interface Toast {
  id: string;
  type: 'info' | 'success' | 'warning' | 'error';
  title: string;
  message?: string;
  duration: number;
  dismissible: boolean;
  action?: { label: string; onClick: () => void };
}

// Theme Tokens
interface ThemeTokens {
  colors: {
    background: string;
    foreground: string;
    primary: string;
    primaryForeground: string;
    secondary: string;
    muted: string;
    mutedForeground: string;
    border: string;
    destructive: string;
  };
  spacing: Record<string, string>;
  radii: Record<string, string>;
}
```

---

## Rust Interfaces

### Shell-Only Commands

```rust
// All commands verify webview label = "shell"

#[tauri::command]
async fn shell_get_installed_apps(
    webview: Webview,
    state: State<'_, AppState>,
) -> Result<Vec<InstalledApp>, String>;

#[tauri::command]
async fn shell_launch_app(
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
) -> Result<RunningSession, String>;

#[tauri::command]
async fn shell_close_app(
    webview: Webview,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String>;

#[tauri::command]
async fn shell_install_app(
    webview: Webview,
    state: State<'_, AppState>,
    source: AppSource,
) -> Result<InstallResult, String>;

#[tauri::command]
async fn shell_uninstall_app(
    webview: Webview,
    state: State<'_, AppState>,
    app_id: String,
    keep_data: bool,
) -> Result<(), String>;

#[tauri::command]
async fn shell_resolve_permission_prompt(
    webview: Webview,
    state: State<'_, AppState>,
    prompt_id: String,
    granted: Vec<String>,
) -> Result<(), String>;

#[tauri::command]
async fn shell_get_identity(
    webview: Webview,
    state: State<'_, AppState>,
) -> Result<IdentityInfo, String>;

#[tauri::command]
async fn shell_open_external_url(
    webview: Webview,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String>;

#[tauri::command]
async fn shell_get_settings(
    webview: Webview,
    state: State<'_, AppState>,
) -> Result<ShellSettings, String>;

#[tauri::command]
async fn shell_set_settings(
    webview: Webview,
    state: State<'_, AppState>,
    patch: ShellSettingsPatch,
) -> Result<(), String>;
```

### Events (Rust → Shell)

| Event | Payload | Description |
|-------|---------|-------------|
| `shell://apps/installed_changed` | `AppsPayload` | App installed/uninstalled |
| `shell://apps/running_changed` | `RunningAppsPayload` | App launched/closed |
| `shell://windows/changed` | `WindowsPayload` | Window state changed |
| `shell://permissions/prompt` | `PermissionPrompt` | Permission request from app |
| `shell://connectivity/changed` | `ConnectivityPayload` | Online/sync status |
| `shell://notifications/new` | `Notification` | New notification |

---

## Settings Persistence

```typescript
interface ShellSettings {
  ui: {
    theme: 'light' | 'dark' | 'system';
    sidebarWidth: number;
    sidebarCollapsed: boolean;
    reducedMotion: boolean;
    highContrast: boolean;
  };
  layout: {
    lastWindowBounds: WindowBounds;
    lastOpenApps: string[];
  };
  notifications: {
    enabled: boolean;
    sound: boolean;
    quietHoursStart: string | null;
    quietHoursEnd: string | null;
  };
  developer: {
    devToolsEnabled: boolean;
    verboseLogging: boolean;
  };
}
```

**Migration Strategy:**
- Version field in settings JSON
- Forward-only migrations
- Default values for new fields
- Backup before migration

---

## Error Containment

### React ErrorBoundaries

- Root ErrorBoundary catches all unhandled errors
- Fallback UI allows diagnostics export
- Safe mode boot if repeated crashes detected

### App Surface Failures

- Webview crash detection via Rust events
- "App crashed" overlay with retry button
- Auto-recovery after 5 seconds
- Max 3 auto-retries, then manual only

### Safe Mode

- Triggered if shell crashes 3 times in 5 minutes
- Disables all third-party apps
- Allows settings access and diagnostics export
- Stored flag in local storage, cleared manually

---

## Platform Considerations

| Feature | Windows | macOS | Linux |
|---------|---------|-------|-------|
| Title bar | Custom or native | Native preferred | Native |
| Window decorations | Standard | Translucent | Standard |
| System tray | Supported | Supported | Best effort |
| Keyboard shortcuts | Ctrl+* | Cmd+* | Ctrl+* |
| High DPI | WebView2 handles | Automatic | Manual scaling |
| DevTools | WebView2 F12 | Safari inspector | WebKitGTK |

---

## Acceptance Criteria

1. [ ] Shell launches in < 2s on all platforms
2. [ ] Keyboard navigation works for all system UI
3. [ ] No XSS vectors in shell code
4. [ ] App webviews cannot access `__TAURI__` APIs
5. [ ] CSP enforced on all 3 platforms
6. [ ] Permission prompts fully keyboard accessible
7. [ ] Theme switching works (light/dark/system)
8. [ ] Settings persist across restarts
9. [ ] Error boundaries prevent full crashes
10. [ ] Memory usage < 200MB for shell alone

---

## Test Cases

1. **Security**: Attempt `invoke()` from app webview → rejected
2. **Security**: Attempt external URL open from app → blocked
3. **Security**: Inject HTML in app name → escaped
4. **A11y**: Navigate all UI with keyboard only
5. **A11y**: Screen reader announces permission prompt
6. **State**: Close and reopen app → state restored
7. **Error**: Crash app webview → shell remains responsive
8. **Theme**: Switch dark/light → all components update
