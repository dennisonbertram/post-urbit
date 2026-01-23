# Post-Urbit Shell

A System 7-inspired frontend shell for the Post-Urbit node.

## Features

- System 7 aesthetic UI components
- Real-time backend integration via HTTP API
- Authentication with password login
- Live app management
- Node health monitoring
- Identity display

## Development

### Prerequisites

- Node.js 18+ and npm
- Post-Urbit backend running on `http://localhost:4433`

### Setup

```bash
npm install
```

### Running

```bash
npm run dev
```

This will start the dev server on `http://localhost:5173` with API proxying to the backend.

### Building

```bash
npm run build
```

## API Integration

The shell integrates with the Post-Urbit HTTP API documented at `/docs/api/http-api.md`.

### Architecture

- **`src/api/client.ts`** - Base API client with fetch wrappers and auth handling
- **`src/api/types.ts`** - TypeScript types matching API responses
- **`src/api/hooks.ts`** - React hooks for data fetching with loading/error states
- **`src/api/index.ts`** - Module exports

### Authentication

The shell uses session-based authentication:

1. User enters password in login prompt
2. POST to `/admin/v1/auth/login` creates a session
3. Session cookies are stored by the browser
4. CSRF token is stored in localStorage
5. All subsequent requests include cookies and CSRF header

### API Hooks

Available hooks:

- `useHealth(pollInterval)` - Health check with auto-polling
- `useNodeStatus()` - Detailed node status
- `useIdentity()` - Current identity info
- `useApps()` - List of installed apps
- `useAppRuntime(appId)` - Runtime status for a specific app
- `useAuth()` - Authentication state and login/logout functions
- `useBackendStatus()` - Backend reachability check

### Error Handling

All hooks return `{ data, loading, error, refetch }`:

- `data` - API response data (null while loading or on error)
- `loading` - Boolean loading state
- `error` - `ApiClientError` instance or null
- `refetch` - Function to manually refetch data

Components gracefully handle:
- Backend unreachable (connection errors)
- Authentication failures (401)
- API errors (4xx, 5xx)
- Loading states
- Empty data

### Proxy Configuration

In development, Vite proxies API requests:

```typescript
// vite.config.ts
proxy: {
  '/health': { target: 'http://localhost:4433' },
  '/admin': { target: 'http://localhost:4433' },
  '/api': { target: 'http://localhost:4433' },
}
```

In production, the shell should be served from the same origin as the API or configured with the correct `baseUrl`.

## Components

### Shell Components

- **AppGrid** - Displays installed apps with real data from API
- **StatusBar** - Shows node health, storage, and connection status
- **LoginPrompt** - Password authentication dialog
- **MenuBar** - Top menu bar with identity display and logout
- **WindowManager** - Multi-window support with drag, resize, minimize
- **Dock** - Quick access to running applications
- **AlertManager** - System 7-styled alert dialogs

### Application Windows

- **System Monitor** - Node info, network status, storage usage, logs
- **Apps Manager** - View and manage installed applications
- **Mail** - Inbox, sent messages, compose new messages

### System 7 Components

Reusable UI primitives styled after classic Mac OS:

- Button, Checkbox, Radio
- TextInput, Dropdown, Slider
- Window, Alert, Icon
- ProgressBar, PermissionPrompt

## Testing

To test the integration:

1. Start the Post-Urbit backend:
   ```bash
   cargo run
   ```

2. Start the shell dev server:
   ```bash
   npm run dev
   ```

3. Open `http://localhost:5173` in a browser

4. Enter the admin password when prompted

5. Verify:
   - Apps load from the backend
   - Status bar shows real node metrics
   - Identity appears in menu bar
   - Logout clears session

## Backend Not Running

If the backend is not running, the shell will display an error:

> Backend Unreachable: Unable to connect to the Post-Urbit backend at http://localhost:4433. Please make sure the node is running.

The shell will retry the connection every 10 seconds.

## Current Features

- Multi-window desktop environment
- System Monitor with health, network, storage, and logs tabs
- Apps Manager for viewing installed applications
- Mail app with inbox, sent, and compose functionality
- Dock for quick access to running apps
- Window management (minimize, cascade, tile)

## Future Enhancements

- WebSocket events subscription for real-time updates
- App installation UI
- Settings management
- Contacts list
- Identity management UI
- Backup/restore UI
