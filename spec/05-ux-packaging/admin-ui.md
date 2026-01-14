# Admin UI

## Overview

The Admin UI is a web-based interface for managing the personal node. It provides:

- Identity and device management
- Contact management
- App installation and configuration
- Node settings
- Logs and diagnostics

## Design Principles

### Responsive Design

The UI must work across device sizes:
- Desktop (primary): Full-featured experience
- Tablet: Adapted layout, full functionality
- Mobile: Essential operations only

### Accessibility

- WCAG 2.1 AA compliance
- Keyboard navigation
- Screen reader compatible
- High contrast mode

### Security First

- No inline scripts (strict CSP)
- All actions require authentication
- Sensitive operations require re-authentication
- Session timeout with warning

## Architecture

### Frontend Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Framework | React 18+ | Component model, ecosystem |
| State | React Query | Server state management |
| Routing | React Router | Standard routing |
| Styling | Tailwind CSS | Utility-first, responsive |
| Build | Vite | Fast builds, ES modules |
| Testing | Vitest + Testing Library | React-native testing |

### Component Structure

```
admin-ui/
├── src/
│   ├── components/         # Reusable UI components
│   │   ├── common/         # Buttons, inputs, cards
│   │   ├── layout/         # Page layouts, navigation
│   │   └── domain/         # Feature-specific components
│   ├── pages/              # Route components
│   │   ├── Dashboard.tsx
│   │   ├── Identity.tsx
│   │   ├── Contacts.tsx
│   │   ├── Apps.tsx
│   │   ├── Messages.tsx
│   │   ├── Settings.tsx
│   │   └── Login.tsx
│   ├── hooks/              # Custom React hooks
│   ├── api/                # API client
│   ├── utils/              # Utilities
│   └── types/              # TypeScript types
├── public/                 # Static assets
└── tests/                  # Test files
```

## Pages

### Dashboard

The landing page after login showing node status at a glance.

```
┌─────────────────────────────────────────────────────────────────┐
│  [Logo] Post Node                    [Search] [?] [Settings] [U]│
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │   Status    │  │   Contacts  │  │    Apps     │              │
│  │   ● Online  │  │      42     │  │     12      │              │
│  │             │  │   active    │  │  installed  │              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  Recent Activity                                    [See All]││
│  ├─────────────────────────────────────────────────────────────┤│
│  │  Alice sent you a message                           2m ago  ││
│  │  NoteApp synced 3 documents                         1h ago  ││
│  │  Bob added you as a contact                         3h ago  ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
│  ┌────────────────────────────┐ ┌──────────────────────────────┐│
│  │  Quick Actions             │ │  Node Health                 ││
│  │  [+ Add Contact]           │ │  CPU:    ████░░ 65%          ││
│  │  [+ Install App]           │ │  Memory: ██░░░░ 40%          ││
│  │  [✉ New Message]           │ │  Storage: ███░░░ 55%         ││
│  └────────────────────────────┘ │  Network: ● Connected        ││
│                                  └──────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

### Identity Page

Manage the node's identity and devices.

**Sections:**
1. Identity Overview (IID, genesis key fingerprint, creation date)
2. Current Keys (signing key, encryption key, last rotated)
3. Devices (list of authorized devices, current device highlighted)
4. Recovery Configuration (current method, trustees if social)
5. Public Profile (display name, avatar, bio if configured)

**Actions:**
- Rotate signing key
- Rotate encryption key
- Add device
- Remove device
- Configure recovery
- Export identity

### Contacts Page

Manage contacts and their trust levels.

**Views:**
- List view: Sortable table with search/filter
- Card view: Visual grid layout

**Contact Card:**
```
┌─────────────────────────────────────────────────────┐
│  [Avatar]  Alice                                     │
│            k5xq7z4m...                               │
│            ● Online | Last seen: 2 min ago          │
├─────────────────────────────────────────────────────┤
│  Trust: ★★★☆☆ (Verified)                            │
│  Added: Jan 1, 2025                                  │
│  Groups: Team, Family                                │
├─────────────────────────────────────────────────────┤
│  [Message]  [View Profile]  [···]                   │
└─────────────────────────────────────────────────────┘
```

**Actions:**
- Add contact (by IID or invite link)
- Remove contact
- Adjust trust level
- Block contact
- View contact's identity document

### Apps Page

Browse, install, and manage applications.

**Sections:**
1. Installed Apps (grid of app cards)
2. Available Updates (if any)
3. Browse Repository (searchable catalog)

**App Card:**
```
┌─────────────────────────────────────────────────────┐
│  [Icon]  Notes App                    v1.2.0        │
│          A simple note-taking app                   │
├─────────────────────────────────────────────────────┤
│  Permissions: Storage, Sync                         │
│  Storage used: 12 MB / 100 MB                       │
│  Last opened: Today                                 │
├─────────────────────────────────────────────────────┤
│  [Open]  [Settings]  [···]                          │
└─────────────────────────────────────────────────────┘
```

**App Detail View:**
- Full description
- Screenshots
- Permissions list with explanations
- Author info (IID, verification status)
- Reviews/ratings (if from repository)
- Version history
- Storage usage
- Activity log

**Actions:**
- Install app
- Update app
- Uninstall app
- Grant/revoke permissions
- Clear app data
- Export app data

### Messages Page

View message history (read-only in admin UI; full messaging in dedicated app).

**Features:**
- Conversation list
- Message search
- Filter by contact/group
- Export conversation

### Settings Page

Configure node behavior.

**Sections:**
1. **Network**
   - Listen addresses
   - Relay servers
   - NAT traversal options
   - Bandwidth limits

2. **Security**
   - Admin password change
   - API key management
   - IP allowlist
   - Auto-update settings

3. **Storage**
   - Data directory info
   - Storage usage breakdown
   - Backup configuration
   - Retention policies

4. **Privacy**
   - Identity publishing frequency
   - Online status visibility
   - Read receipt settings

5. **Apps**
   - Default permissions
   - Sideloading toggle
   - Repository sources

6. **Advanced**
   - Log level
   - Debug mode
   - Export config
   - Factory reset

## API Client

### Client Architecture

```typescript
// Base API client
class ApiClient {
  private baseUrl: string;
  private authToken: string | null;

  constructor(baseUrl: string = '/admin/v1') {
    this.baseUrl = baseUrl;
    this.authToken = null;
  }

  setAuthToken(token: string): void {
    this.authToken = token;
  }

  async request<T>(
    method: string,
    path: string,
    options?: {
      body?: unknown;
      headers?: Record<string, string>;
    }
  ): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        'Content-Type': 'application/json',
        ...(this.authToken && { 'Authorization': `Bearer ${this.authToken}` }),
        ...options?.headers,
      },
      body: options?.body ? JSON.stringify(options.body) : undefined,
      credentials: 'same-origin',
    });

    if (!response.ok) {
      const error = await response.json() as ApiError;
      throw new ApiError(error.error.code, error.error.message);
    }

    return response.json();
  }

  // Convenience methods
  get<T>(path: string): Promise<T> {
    return this.request<T>('GET', path);
  }

  post<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('POST', path, { body });
  }

  put<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('PUT', path, { body });
  }

  delete<T>(path: string): Promise<T> {
    return this.request<T>('DELETE', path);
  }
}
```

### Domain API Modules

```typescript
// Identity API
interface IdentityApi {
  getIdentity(): Promise<IdentityInfo>;
  rotateSigningKey(): Promise<KeyRotationResult>;
  rotateEncryptionKey(): Promise<KeyRotationResult>;
  getDevices(): Promise<Device[]>;
  addDevice(name: string): Promise<DeviceAddResult>;
  removeDevice(did: string): Promise<void>;
  getRecoveryConfig(): Promise<RecoveryConfig>;
  updateRecoveryConfig(config: RecoveryConfig): Promise<void>;
}

// Contacts API
interface ContactsApi {
  listContacts(options?: ListOptions): Promise<PaginatedResult<Contact>>;
  getContact(iid: string): Promise<Contact>;
  addContact(iid: string, metadata?: ContactMetadata): Promise<Contact>;
  updateContact(iid: string, updates: ContactUpdate): Promise<Contact>;
  removeContact(iid: string): Promise<void>;
  blockContact(iid: string): Promise<void>;
  unblockContact(iid: string): Promise<void>;
}

// Apps API
interface AppsApi {
  listInstalled(): Promise<InstalledApp[]>;
  getApp(appId: string): Promise<InstalledApp>;

  // Install by URL or repository reference
  installFromSource(source: { type: 'url' | 'repository'; value: string }): Promise<InstallResult>;

  // Install by file upload (browser) - uses multipart/form-data
  installFromFile(file: File): Promise<InstallResult>;

  uninstall(appId: string, options?: UninstallOptions): Promise<void>;
  update(appId: string): Promise<UpdateResult>;
  getSettings(appId: string): Promise<PerAppSettings>;
  updateSettings(appId: string, settings: PerAppSettings): Promise<void>;
  getPermissions(appId: string): Promise<AppPermissions>;
  updatePermissions(appId: string, patch: PermissionPatch): Promise<AppPermissions>;
  clearData(appId: string): Promise<void>;
}

// Settings API
interface SettingsApi {
  getAll(): Promise<NodeSettings>;
  get<K extends keyof NodeSettings>(key: K): Promise<NodeSettings[K]>;
  update(settings: Partial<NodeSettings>): Promise<NodeSettings>;
  reset(key?: keyof NodeSettings): Promise<void>;
}

// System API
interface SystemApi {
  getStatus(): Promise<NodeStatus>;
  getLogs(options?: LogOptions): Promise<LogsResponse>;
  createBackup(type?: 'full' | 'identity' | 'data'): Promise<BackupResult>;
  listBackups(): Promise<BackupListEntry[]>;
  downloadBackup(id: string): Promise<Blob>;      // Returns file download
  uploadBackup(file: File): Promise<BackupListEntry>;
  restoreBackup(id: string, password?: string): Promise<RestoreResult>;
  restart(): Promise<void>;
  shutdown(): Promise<void>;
}
```

## Authentication Flow

### Login Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                         Login Page                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    [Node Logo]                               ││
│  │                    Post Node                                 ││
│  │                                                              ││
│  │  Admin Password                                              ││
│  │  ┌─────────────────────────────────────────────────────┐    ││
│  │  │ ••••••••••••                                        │    ││
│  │  └─────────────────────────────────────────────────────┘    ││
│  │                                                              ││
│  │  ☑ Remember this device (30 days)                           ││
│  │                                                              ││
│  │  ┌─────────────────────────────────────────────────────┐    ││
│  │  │                    [Sign In]                         │    ││
│  │  └─────────────────────────────────────────────────────┘    ││
│  │                                                              ││
│  │  Forgot password? Reset via CLI                             ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Session Management

```typescript
interface Session {
  id: string;
  createdAt: string;
  expiresAt: string;
  lastActivity: string;
  userAgent: string;
  ipAddress: string;
  deviceId?: string;          // If "remember device" was checked
}

interface AuthState {
  isAuthenticated: boolean;
  session?: Session;
  needsReauth: boolean;       // For sensitive operations
}

// Session cookie settings
const SESSION_COOKIE = {
  name: 'postnode_session',
  httpOnly: true,
  secure: true,               // Always (even localhost uses TLS)
  sameSite: 'strict',
  maxAge: 24 * 60 * 60,       // 24 hours default
};
```

### Re-authentication

Sensitive operations require fresh authentication:

| Operation | Re-auth Required |
|-----------|------------------|
| View settings | No |
| Change settings | Yes |
| Rotate keys | Yes |
| Add/remove device | Yes |
| Uninstall app | No |
| Clear app data | Yes |
| Create backup | No |
| Restore backup | Yes |
| Change password | Yes |

## Real-time Updates

### WebSocket Connection

```typescript
// WebSocket for real-time updates
const ws = new WebSocket('wss://localhost:8080/admin/v1/events');

// Event types
type AdminEvent =
  | { type: 'status_change'; data: NodeStatus }
  | { type: 'contact_online'; data: { iid: string; online: boolean } }
  | { type: 'message_received'; data: MessageSummary }
  | { type: 'app_installed'; data: InstalledApp }
  | { type: 'app_updated'; data: InstalledApp }
  | { type: 'sync_progress'; data: SyncProgress }
  | { type: 'error'; data: ErrorEvent };

// React hook for events
function useAdminEvents(handler: (event: AdminEvent) => void) {
  useEffect(() => {
    const ws = new WebSocket('/admin/v1/events');
    ws.onmessage = (e) => handler(JSON.parse(e.data));
    return () => ws.close();
  }, [handler]);
}
```

## Security Headers

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self';
  style-src 'self' 'unsafe-inline';
  img-src 'self' data: blob:;
  font-src 'self';
  connect-src 'self';
  frame-ancestors 'none';
  form-action 'self';
  base-uri 'self';
  upgrade-insecure-requests;

X-Content-Type-Options: nosniff
X-Frame-Options: DENY
X-XSS-Protection: 0
Referrer-Policy: strict-origin-when-cross-origin
Permissions-Policy: geolocation=(), microphone=(), camera=()
```

**CSP Notes:**
- `connect-src 'self'` allows both HTTP/HTTPS and WS/WSS to same origin
- `style-src 'unsafe-inline'` needed for Tailwind's runtime styles (tradeoff for utility-first CSS)
- All styles are built at compile time from Tailwind (no external CDN)
- WebSocket connects to same origin only (no external WS allowed)

## Error Handling

### Error Display

```typescript
// Error boundary for React
class ErrorBoundary extends React.Component {
  state = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }

  render() {
    if (this.state.hasError) {
      return <ErrorPage error={this.state.error} />;
    }
    return this.props.children;
  }
}

// Toast notifications for API errors
function useApiErrorHandler() {
  return useCallback((error: ApiError) => {
    toast.error(getErrorMessage(error.code), {
      description: error.message,
      action: error.code === 'UNAUTHORIZED' ? {
        label: 'Sign In',
        onClick: () => navigate('/login'),
      } : undefined,
    });
  }, []);
}
```

### Error Messages

| Code | User Message |
|------|--------------|
| `UNAUTHORIZED` | Your session has expired. Please sign in again. |
| `FORBIDDEN` | You don't have permission to perform this action. |
| `NOT_FOUND` | The requested resource was not found. |
| `RATE_LIMITED` | Too many requests. Please wait a moment. |
| `INTERNAL_ERROR` | Something went wrong. Please try again. |
| `SERVICE_UNAVAILABLE` | The node is temporarily unavailable. |
