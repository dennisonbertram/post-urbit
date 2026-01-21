# Post-Urbit Frontend Architecture: Hybrid App Platform

## Executive Summary

This document proposes a hybrid frontend architecture for Post-Urbit that enables:
1. A performant, native-feeling desktop application using Tauri (Rust)
2. A beautiful, accessible UI using React + shadcn + Tailwind
3. A sandboxed app platform where developers can build and distribute applications
4. Integration with the existing Post-Urbit WASM runtime and capability system

## Design Goals

1. **Performance**: Near-native performance for the shell, efficient app isolation
2. **Developer Accessibility**: Use familiar web technologies (React, Tailwind)
3. **Security**: Strong sandboxing, capability-based permissions
4. **Composability**: Apps can interact with each other through controlled APIs
5. **Offline-First**: Everything works locally, syncs when connected
6. **Sovereignty**: Users own their data, apps are portable

## Architecture Overview

```
┌────────────────────────────────────────────────────────────────────────────┐
│                           TAURI APPLICATION                                 │
├────────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                    WEBVIEW (System Shell)                            │  │
│  │  ┌────────────────────────────────────────────────────────────────┐  │  │
│  │  │                React + shadcn + Tailwind                       │  │  │
│  │  │                                                                │  │  │
│  │  │  ┌──────────┐ ┌─────────────────────────────────────────────┐ │  │  │
│  │  │  │ Sidebar  │ │              App Container                  │ │  │  │
│  │  │  │          │ │  ┌─────────────────────────────────────────┐│ │  │  │
│  │  │  │ • Home   │ │  │           Sandboxed iframe              ││ │  │  │
│  │  │  │ • Apps   │ │  │                                         ││ │  │  │
│  │  │  │ • Chat   │ │  │      Developer's WASM/JS App            ││ │  │  │
│  │  │  │ • Notes  │ │  │                                         ││ │  │  │
│  │  │  │ • +      │ │  │  ┌─────────────────────────────────┐    ││ │  │  │
│  │  │  │          │ │  │  │    @post-urbit/sdk              │    ││ │  │  │
│  │  │  │          │ │  │  │    postMessage bridge           │    ││ │  │  │
│  │  │  └──────────┘ │  │  └─────────────────────────────────┘    ││ │  │  │
│  │  │               │  └─────────────────────────────────────────┘│ │  │  │
│  │  │               └─────────────────────────────────────────────┘ │  │  │
│  │  └────────────────────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                    ↕ Tauri IPC                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                         RUST BACKEND                                  │  │
│  │                                                                       │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │  │
│  │  │   Bridge    │  │  Permission │  │    App      │  │   State     │  │  │
│  │  │   Layer     │  │   Manager   │  │   Registry  │  │   Manager   │  │  │
│  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  │  │
│  │         │                │                │                │         │  │
│  │         └────────────────┴────────────────┴────────────────┘         │  │
│  │                                   │                                   │  │
│  │  ┌────────────────────────────────┴────────────────────────────────┐ │  │
│  │  │                    POST-URBIT CORE                              │ │  │
│  │  │                                                                 │ │  │
│  │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │ │  │
│  │  │  │ Identity │ │ Storage  │ │Messaging │ │   DHT    │           │ │  │
│  │  │  │ Manager  │ │  (CRDT)  │ │ Service  │ │ (libp2p) │           │ │  │
│  │  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘           │ │  │
│  │  │                                                                 │ │  │
│  │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │ │  │
│  │  │  │  WASM    │ │   App    │ │ Transport│ │  Sync    │           │ │  │
│  │  │  │ Runtime  │ │  Store   │ │  (QUIC)  │ │ Engine   │           │ │  │
│  │  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘           │ │  │
│  │  └─────────────────────────────────────────────────────────────────┘ │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
```

## Layer 1: Tauri Shell Application

### Why Tauri?

| Feature | Tauri | Electron |
|---------|-------|----------|
| Bundle Size | ~10-20 MB | ~150-200 MB |
| Memory Usage | ~50-100 MB | ~300-500 MB |
| Backend Language | Rust | Node.js |
| Security | Native OS sandboxing | Chromium sandbox |
| WebView | System native | Bundled Chromium |

Tauri provides:
- **Rust backend**: Direct integration with Post-Urbit core
- **Native performance**: No V8 overhead for system operations
- **Small footprint**: Ships with system webview
- **Strong security**: OS-level process isolation

### Tauri Integration Points

```rust
// src-tauri/src/main.rs

#[tauri::command]
async fn storage_get(
    state: State<'_, AppState>,
    app_id: &str,
    key: &str,
) -> Result<Option<Vec<u8>>, String> {
    // Permission check
    state.permissions.check(app_id, "storage:read")?;

    // Forward to Post-Urbit core
    state.core.storage_get(app_id, key).await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn identity_get(
    state: State<'_, AppState>,
    app_id: &str,
) -> Result<IdentityInfo, String> {
    state.permissions.check(app_id, "identity:read")?;
    state.core.get_identity().await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn messaging_send(
    state: State<'_, AppState>,
    app_id: &str,
    recipient: &str,
    message_type: &str,
    content: Vec<u8>,
) -> Result<MessageId, String> {
    state.permissions.check(app_id, "messaging:send")?;
    state.core.messaging_send(recipient, message_type, content).await
        .map_err(|e| e.to_string())
}
```

## Layer 2: System Shell (React + shadcn + Tailwind)

The shell provides the "operating system" chrome - navigation, window management, notifications.

### Shell Responsibilities

1. **Window Management**: Opening, closing, tiling, focusing apps
2. **Navigation**: Sidebar, app launcher, command palette
3. **System UI**: Status bar, notifications, settings
4. **App Lifecycle**: Loading, unloading, error handling
5. **Permission Prompts**: Showing permission requests from apps

### Shell Components

```typescript
// src/components/shell/AppHost.tsx
import { useEffect, useRef, useState } from 'react';
import { AppManifest, AppPermission } from '@/types';

interface AppHostProps {
  app: AppManifest;
  onClose: () => void;
}

export function AppHost({ app, onClose }: AppHostProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');

  useEffect(() => {
    const handleMessage = async (event: MessageEvent) => {
      // Verify origin matches our app protocol
      if (event.source !== iframeRef.current?.contentWindow) return;

      const { type, payload, requestId } = event.data;

      try {
        let result;

        switch (type) {
          case 'storage:get':
            result = await invoke('storage_get', {
              appId: app.id,
              key: payload.key
            });
            break;

          case 'identity:get':
            result = await invoke('identity_get', { appId: app.id });
            break;

          case 'messaging:send':
            result = await invoke('messaging_send', {
              appId: app.id,
              ...payload
            });
            break;

          case 'permission:request':
            result = await showPermissionPrompt(app, payload.permissions);
            break;
        }

        // Send response back to app
        iframeRef.current?.contentWindow?.postMessage({
          type: 'response',
          requestId,
          success: true,
          result
        }, '*');

      } catch (error) {
        iframeRef.current?.contentWindow?.postMessage({
          type: 'response',
          requestId,
          success: false,
          error: error.message
        }, '*');
      }
    };

    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [app]);

  return (
    <div className="app-host flex-1 relative">
      <iframe
        ref={iframeRef}
        src={`postapp://${app.id}/index.html`}
        sandbox="allow-scripts allow-same-origin"
        className="w-full h-full border-0"
        onLoad={() => setStatus('ready')}
        onError={() => setStatus('error')}
      />
      {status === 'loading' && <AppLoadingOverlay app={app} />}
      {status === 'error' && <AppErrorOverlay app={app} onRetry={() => {}} />}
    </div>
  );
}
```

### Window Manager

```typescript
// src/stores/windowStore.ts
import { create } from 'zustand';

interface Window {
  id: string;
  appId: string;
  title: string;
  position: { x: number; y: number };
  size: { width: number; height: number };
  state: 'normal' | 'minimized' | 'maximized';
  zIndex: number;
}

interface WindowStore {
  windows: Window[];
  activeWindowId: string | null;

  openApp: (appId: string) => void;
  closeWindow: (windowId: string) => void;
  focusWindow: (windowId: string) => void;
  minimizeWindow: (windowId: string) => void;
  maximizeWindow: (windowId: string) => void;
}

export const useWindowStore = create<WindowStore>((set, get) => ({
  windows: [],
  activeWindowId: null,

  openApp: (appId) => {
    const existing = get().windows.find(w => w.appId === appId);
    if (existing) {
      get().focusWindow(existing.id);
      return;
    }

    const newWindow: Window = {
      id: crypto.randomUUID(),
      appId,
      title: '', // Loaded from manifest
      position: calculateNewWindowPosition(),
      size: { width: 800, height: 600 },
      state: 'normal',
      zIndex: get().windows.length + 1,
    };

    set(state => ({
      windows: [...state.windows, newWindow],
      activeWindowId: newWindow.id,
    }));
  },

  // ... other methods
}));
```

## Layer 3: App Container & Sandbox

### iframe Sandbox Configuration

```html
<iframe
  src="postapp://com.example.app/index.html"
  sandbox="allow-scripts allow-same-origin allow-forms"
  allow="clipboard-write"
  csp="
    default-src 'self';
    script-src 'self' 'wasm-unsafe-eval';
    style-src 'self' 'unsafe-inline';
    connect-src 'none';
    img-src 'self' data: blob:;
  "
/>
```

**Security properties:**
- `allow-scripts`: Required for app functionality
- `allow-same-origin`: Required for postMessage origin verification
- `connect-src 'none'`: **No direct network access** - must go through platform APIs
- `script-src 'wasm-unsafe-eval'`: Required for WASM execution

### Custom Protocol Handler

Tauri supports custom protocol handlers to serve app content:

```rust
// src-tauri/src/protocol.rs

pub fn register_postapp_protocol(app: &mut tauri::App) -> Result<(), Box<dyn Error>> {
    tauri::protocol::register(
        app,
        "postapp",
        move |request| {
            let url = request.uri();
            // URL format: postapp://com.example.app/path/to/file
            let app_id = url.host().unwrap_or_default();
            let path = url.path();

            // Load from app's installed directory
            let app_dir = apps_dir.join(app_id);
            let file_path = app_dir.join(path.trim_start_matches('/'));

            // Security: Ensure path is within app directory
            let canonical = file_path.canonicalize()?;
            if !canonical.starts_with(&app_dir) {
                return Err(ProtocolError::PathTraversal);
            }

            let content = std::fs::read(&canonical)?;
            let mime_type = mime_guess::from_path(&canonical)
                .first_or_octet_stream()
                .to_string();

            Ok(ResponseBuilder::new()
                .header("Content-Type", mime_type)
                .body(content)?)
        }
    )
}
```

## Layer 4: Platform SDK (@post-urbit/sdk)

The SDK provides a clean, React-friendly API for app developers.

### Core APIs

```typescript
// @post-urbit/sdk

// ============ Identity ============

export function useIdentity(): Identity {
  const [identity, setIdentity] = useState<Identity | null>(null);

  useEffect(() => {
    bridge.call('identity:get').then(setIdentity);
  }, []);

  return identity;
}

// ============ Storage ============

export function useStore<T>(key: string, defaultValue?: T): [T | undefined, (value: T) => Promise<void>] {
  const [value, setValue] = useState<T | undefined>(defaultValue);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    bridge.call('storage:get', { key }).then(result => {
      if (result.value) {
        setValue(decode(result.value));
        setVersion(result.version);
      }
    });
  }, [key]);

  const update = async (newValue: T) => {
    const encoded = encode(newValue);
    const result = await bridge.call('storage:set', {
      key,
      value: encoded,
      expectedVersion: version,
    });
    setValue(newValue);
    setVersion(result.version);
  };

  return [value, update];
}

// ============ Messaging ============

export function useMessaging() {
  const send = async (recipient: string, messageType: string, content: any) => {
    return bridge.call('messaging:send', {
      recipient,
      messageType,
      content: encode(content),
    });
  };

  const subscribe = (filter: MessageFilter, callback: (message: Message) => void) => {
    return bridge.subscribe('messaging:message', filter, callback);
  };

  return { send, subscribe };
}

// ============ Contacts ============

export function useContacts(options?: { limit?: number }): Contact[] {
  const [contacts, setContacts] = useState<Contact[]>([]);

  useEffect(() => {
    bridge.call('contacts:list', options).then(result => {
      setContacts(result.contacts);
    });
  }, [options?.limit]);

  return contacts;
}

// ============ Cross-App ============

export function useApp(appId: string) {
  return {
    call: async (method: string, args?: any) => {
      return bridge.call('app:invoke', {
        targetApp: appId,
        method,
        args: args ? encode(args) : undefined,
      });
    }
  };
}

// ============ Permissions ============

export function usePermissions() {
  const request = async (permissions: string[]): Promise<PermissionResult> => {
    return bridge.call('permission:request', { permissions });
  };

  const check = async (permission: string): Promise<boolean> => {
    return bridge.call('permission:check', { permission });
  };

  return { request, check };
}
```

### Bridge Implementation

```typescript
// @post-urbit/sdk/bridge.ts

class PlatformBridge {
  private pendingRequests = new Map<string, { resolve: Function; reject: Function }>();
  private subscriptions = new Map<string, Set<Function>>();

  constructor() {
    window.addEventListener('message', this.handleMessage.bind(this));
  }

  async call<T>(type: string, payload?: any): Promise<T> {
    const requestId = crypto.randomUUID();

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(requestId, { resolve, reject });

      window.parent.postMessage({
        type,
        payload,
        requestId,
      }, '*');

      // Timeout after 30 seconds
      setTimeout(() => {
        if (this.pendingRequests.has(requestId)) {
          this.pendingRequests.delete(requestId);
          reject(new Error('Request timeout'));
        }
      }, 30000);
    });
  }

  subscribe(event: string, filter: any, callback: Function): () => void {
    // Implementation for real-time subscriptions
    const subscriptionId = crypto.randomUUID();

    this.call('subscribe', { event, filter, subscriptionId });

    if (!this.subscriptions.has(subscriptionId)) {
      this.subscriptions.set(subscriptionId, new Set());
    }
    this.subscriptions.get(subscriptionId)!.add(callback);

    return () => {
      this.subscriptions.get(subscriptionId)?.delete(callback);
      this.call('unsubscribe', { subscriptionId });
    };
  }

  private handleMessage(event: MessageEvent) {
    const { type, requestId, success, result, error, subscriptionId, data } = event.data;

    if (type === 'response' && requestId) {
      const pending = this.pendingRequests.get(requestId);
      if (pending) {
        this.pendingRequests.delete(requestId);
        if (success) {
          pending.resolve(result);
        } else {
          pending.reject(new Error(error));
        }
      }
    }

    if (type === 'event' && subscriptionId) {
      const callbacks = this.subscriptions.get(subscriptionId);
      callbacks?.forEach(cb => cb(data));
    }
  }
}

export const bridge = new PlatformBridge();
```

## Permission System

### Permission Tiers

```typescript
enum PermissionTier {
  // Granted automatically, no prompt
  ALWAYS_GRANTED = 'always_granted',

  // User prompted once, remembered
  PROMPT_ONCE = 'prompt_once',

  // User prompted every time (sensitive operations)
  PROMPT_ALWAYS = 'prompt_always',

  // Never granted to apps
  SYSTEM_ONLY = 'system_only',
}

const PERMISSION_CONFIG: Record<string, PermissionTier> = {
  // Always granted
  'storage:read': PermissionTier.ALWAYS_GRANTED,
  'storage:write': PermissionTier.ALWAYS_GRANTED,
  'identity:read:limited': PermissionTier.ALWAYS_GRANTED,  // Only IID

  // Prompt once
  'contacts:read': PermissionTier.PROMPT_ONCE,
  'notifications:show': PermissionTier.PROMPT_ONCE,
  'app:invoke:*': PermissionTier.PROMPT_ONCE,

  // Prompt always
  'messaging:send': PermissionTier.PROMPT_ALWAYS,
  'identity:read:full': PermissionTier.PROMPT_ALWAYS,
  'crypto:sign': PermissionTier.PROMPT_ALWAYS,

  // System only
  'system:shutdown': PermissionTier.SYSTEM_ONLY,
  'apps:install': PermissionTier.SYSTEM_ONLY,
  'identity:rotate': PermissionTier.SYSTEM_ONLY,
};
```

### Permission Prompt UI

```typescript
// src/components/shell/PermissionPrompt.tsx

interface PermissionPromptProps {
  app: AppManifest;
  permissions: string[];
  onGrant: (granted: string[]) => void;
  onDeny: () => void;
}

export function PermissionPrompt({ app, permissions, onGrant, onDeny }: PermissionPromptProps) {
  const [selected, setSelected] = useState<Set<string>>(new Set(permissions));

  return (
    <Dialog open>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <img src={app.icon} className="w-8 h-8 rounded" />
            {app.name} is requesting permissions
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          {permissions.map(permission => (
            <div key={permission} className="flex items-start gap-3">
              <Checkbox
                checked={selected.has(permission)}
                onCheckedChange={(checked) => {
                  const next = new Set(selected);
                  if (checked) next.add(permission);
                  else next.delete(permission);
                  setSelected(next);
                }}
              />
              <div>
                <p className="font-medium">{PERMISSION_NAMES[permission]}</p>
                <p className="text-sm text-muted-foreground">
                  {PERMISSION_DESCRIPTIONS[permission]}
                </p>
              </div>
            </div>
          ))}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onDeny}>Deny All</Button>
          <Button onClick={() => onGrant(Array.from(selected))}>
            Allow Selected
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

## App Manifest Format

Apps define their metadata and permissions in `manifest.json`:

```json
{
  "manifestVersion": 2,
  "app": {
    "id": "com.example.notes",
    "name": "Notes",
    "version": "1.0.0",
    "description": "A simple notes app with sync",
    "author": {
      "name": "Example Developer",
      "iid": "iid-abc123"
    },
    "icon": "./assets/icon.png",
    "license": "MIT"
  },
  "entry": "./dist/index.html",
  "permissions": {
    "required": [
      "storage:read",
      "storage:write"
    ],
    "optional": [
      "contacts:read",
      "messaging:send"
    ],
    "reasons": {
      "contacts:read": "To share notes with your contacts",
      "messaging:send": "To send note links to others"
    }
  },
  "exports": {
    "createNote": {
      "description": "Create a new note",
      "parameters": {
        "title": "string",
        "content": "string"
      },
      "returns": "NoteId"
    },
    "getNotes": {
      "description": "Get all notes",
      "returns": "Note[]"
    }
  }
}
```

## App-to-App Communication

### Registering Exports

```typescript
// In Notes app
import { registerExport } from '@post-urbit/sdk';

registerExport('createNote', async ({ title, content }) => {
  const note = await createNote(title, content);
  return note.id;
});

registerExport('getNotes', async () => {
  return await getAllNotes();
});
```

### Calling Other Apps

```typescript
// In Calendar app
import { useApp } from '@post-urbit/sdk';

function CalendarEvent({ event }) {
  const notes = useApp('com.example.notes');

  const createMeetingNotes = async () => {
    // This will trigger a permission prompt on first use
    const noteId = await notes.call('createNote', {
      title: `Notes: ${event.title}`,
      content: `Meeting on ${event.date}`
    });

    // Save reference to note
    await updateEvent(event.id, { noteId });
  };

  return (
    <div>
      <h2>{event.title}</h2>
      <Button onClick={createMeetingNotes}>Create Meeting Notes</Button>
    </div>
  );
}
```

## Data Model

### Per-App Namespacing

```
/apps/
  /com.example.notes/
    /data/
      /notes/
        /note-{uuid}  → NoteDocument
      /settings      → SettingsDocument
    /cache/
      /thumbnails/   → Blob references

  /com.example.chat/
    /data/
      /conversations/
        /conv-{uuid}  → ConversationDocument
      /messages/
        /msg-{uuid}   → MessageDocument
```

### CRDT Integration

The existing Post-Urbit sync system provides CRDT-based replication:

```typescript
// App perspective - just use React state patterns
const [notes, setNotes] = useStore<Note[]>('notes', []);

// Under the hood:
// 1. Write goes to local storage
// 2. Post-Urbit core wraps in CRDT operation
// 3. Syncs to other devices via the sync engine
// 4. Merges automatically resolve conflicts
```

## Developer Experience

### Project Setup

```bash
# Create new app
npx create-post-urbit-app my-app
cd my-app

# Project structure
my-app/
├── manifest.json
├── package.json
├── tsconfig.json
├── vite.config.ts
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   └── components/
├── public/
│   └── icon.png
└── dist/           # Built output
```

### Development Server

```bash
# Start dev server with hot reload
npm run dev

# This:
# 1. Starts Vite dev server
# 2. Connects to local Post-Urbit node
# 3. Injects dev SDK with enhanced error messages
# 4. Hot reloads on file changes
```

### Building & Publishing

```bash
# Build production bundle
npm run build

# Package as .postapp
npm run package

# Publish to your ship
npm run publish

# Or publish to a repository
npm run publish --repository https://apps.example.com
```

## Integration with Existing Post-Urbit

### Bridging to RuntimeManager

The Tauri backend can either:

1. **Direct integration**: Embed `RuntimeManager` for headless WASM apps
2. **HTTP bridge**: Connect to running Post-Urbit node via HTTP API
3. **Hybrid**: Use RuntimeManager for background tasks, webview for UI apps

```rust
// src-tauri/src/core.rs

pub struct AppState {
    // Option 1: Direct embedding
    pub runtime: Arc<Mutex<RuntimeManager>>,
    pub node: Arc<PostUrbitNode>,

    // Option 2: HTTP client to existing node
    pub node_client: PostUrbitClient,
}

impl AppState {
    pub async fn storage_get(&self, app_id: &str, key: &str) -> Result<Option<Vec<u8>>> {
        // Direct: use runtime manager
        let runtime = self.runtime.lock().await;
        runtime.storage_get(app_id, key)

        // Or HTTP: call node API
        self.node_client.get(&format!("/apps/{}/storage/{}", app_id, key)).await
    }
}
```

### Capability Mapping

Map existing Post-Urbit capabilities to frontend permissions:

| Post-Urbit Capability | Frontend Permission |
|-----------------------|---------------------|
| `storage:app` | `storage:read`, `storage:write` |
| `messaging:send` | `messaging:send` |
| `messaging:subscribe` | `messaging:receive` |
| `contacts:read` | `contacts:read` |
| `notifications:show` | `notifications:show` |
| `sync:documents` | `sync:create`, `sync:write` |
| `app:invoke:*` | `app:invoke:*` |

## Performance Considerations

### App Loading

1. **Lazy loading**: Only load app bundles when opened
2. **Preloading**: Prefetch frequently-used apps in background
3. **Caching**: Cache app bundles in Tauri's app data directory

### IPC Optimization

1. **Batching**: Batch multiple API calls into single IPC round-trip
2. **Subscriptions**: Use push model for real-time data instead of polling
3. **Binary encoding**: Use CBOR for efficient serialization (matching Post-Urbit core)

### Memory Management

1. **Iframe isolation**: Each app's memory is isolated
2. **Unload idle apps**: Unload apps that haven't been used recently
3. **Resource limits**: Enforce per-app memory limits via iframe policies

## Security Considerations

### Threat Model

1. **Malicious app**: An installed app tries to access data/capabilities it shouldn't
2. **XSS in app**: Attacker injects script into an app
3. **Supply chain**: Compromised app update
4. **Data exfiltration**: App tries to send data to external servers

### Mitigations

1. **iframe sandbox**: Browser-enforced isolation
2. **CSP**: No external network access
3. **Permission system**: Explicit user consent for sensitive operations
4. **Signature verification**: All apps must be signed by known developer
5. **Audit logging**: Log all permission-sensitive operations

## Open Questions

1. **Background apps**: How to handle apps that need to run in background (e.g., sync agents)?
2. **Native extensions**: Should apps be able to include native code for performance-critical operations?
3. **Theming**: How do apps inherit system theme vs. custom themes?
4. **Deep linking**: How to handle `postapp://com.example.app/path` links from outside?
5. **Multi-window**: Should apps be able to open multiple windows?
6. **Drag-and-drop**: How to handle drag-and-drop between apps and system?

## Implementation Phases

### Phase 1: Foundation
- [ ] Tauri app scaffold with Post-Urbit core integration
- [ ] Basic shell UI (sidebar, app list)
- [ ] iframe app container with postMessage bridge
- [ ] Core SDK (storage, identity)

### Phase 2: Platform APIs
- [ ] Full permission system with UI
- [ ] Messaging API
- [ ] Contacts API
- [ ] Notifications API
- [ ] App-to-app communication

### Phase 3: Developer Experience
- [ ] create-post-urbit-app CLI
- [ ] Development server with hot reload
- [ ] SDK documentation
- [ ] Example apps

### Phase 4: Ecosystem
- [ ] App signing and verification
- [ ] Repository support
- [ ] Marketplace UI
- [ ] App updates
