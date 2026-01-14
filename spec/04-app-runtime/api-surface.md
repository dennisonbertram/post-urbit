# Host API Surface

## Overview

The Host API is the interface between applications and the node. Applications call host functions to access storage, messaging, contacts, sync, and system services. All calls are capability-gated.

## API Design Principles

### Async by Default

All I/O operations are asynchronous. Apps yield to the host and receive callbacks.

### Capability-Gated

Every API method requires a specific capability. Calls without required capability return `PERMISSION_DENIED`.

### Structured Data

Data is passed as CBOR-encoded structures. JSON is used in documentation for readability.

### Error Handling

All methods return `Result<T, Error>` with typed error codes.

## Host Call Convention

See `abi.md` for the authoritative ABI specification including import signatures, return codes, and memory conventions.

### Call Flow

```
1. App encodes arguments as CBOR
2. App calls host.call(method_ptr, method_len, args_ptr, args_len)
3. Host validates capability
4. Host begins operation (may be async)
5. App calls host.poll(timeout) to wait for completion
6. App calls host.get_result(call_id, buffer_ptr, buffer_max)
7. App decodes CBOR result envelope
```

### Result Envelope Format

All host call results are CBOR-encoded with this envelope:

```typescript
// Success
interface SuccessEnvelope<T> {
  ok: true;
  value: T;
}

// Error
interface ErrorEnvelope {
  ok: false;
  error: {
    code: string;      // Error code from method-specific error types
    message: string;   // Human-readable description
    details?: object;  // Optional additional context
  };
}

type ResultEnvelope<T> = SuccessEnvelope<T> | ErrorEnvelope;
```

The `host.get_result` return value indicates transport-level status only:
- `> 0`: Bytes written to buffer (decode CBOR to check ok/error)
- `0`: Call still pending
- `< 0`: Transport error (see `abi.md` for codes)

## Storage API

### Methods

| Method | Capability | Description |
|--------|------------|-------------|
| `storage.get` | `storage:app` | Get value by key |
| `storage.set` | `storage:app` | Set value by key |
| `storage.delete` | `storage:app` | Delete key |
| `storage.list` | `storage:app` | List keys with prefix |
| `storage.shared.get` | `storage:shared:*` | Get from shared namespace |
| `storage.shared.set` | `storage:shared:*` | Set in shared namespace |

### storage.get

```typescript
// Request
interface StorageGetRequest {
  key: string;        // Max 256 bytes
}

// Response
interface StorageGetResponse {
  value: Uint8Array | null;  // null if not found
  version: number;           // For optimistic concurrency
}

// Errors
type StorageGetError =
  | 'KEY_TOO_LONG'
  | 'PERMISSION_DENIED';
```

### storage.set

```typescript
// Request
interface StorageSetRequest {
  key: string;                  // Max 256 bytes
  value: Uint8Array;            // Max 1MB
  expected_version?: number;    // For optimistic concurrency (optional)
}

// Response
interface StorageSetResponse {
  version: number;              // New version after set
}

// Errors
type StorageSetError =
  | 'KEY_TOO_LONG'
  | 'VALUE_TOO_LARGE'
  | 'QUOTA_EXCEEDED'
  | 'VERSION_MISMATCH'
  | 'PERMISSION_DENIED';
```

### storage.delete

```typescript
// Request
interface StorageDeleteRequest {
  key: string;
}

// Response
interface StorageDeleteResponse {
  deleted: boolean;   // true if key existed
}

// Errors
type StorageDeleteError =
  | 'PERMISSION_DENIED';
```

### storage.list

```typescript
// Request
interface StorageListRequest {
  prefix: string;           // Key prefix to match
  cursor?: string;          // Pagination cursor
  limit?: number;           // Max results (default: 100, max: 1000)
}

// Response
interface StorageListResponse {
  keys: string[];
  cursor?: string;          // null if no more results
  has_more: boolean;
}

// Errors
type StorageListError =
  | 'PERMISSION_DENIED';
```

## Messaging API

### Methods

| Method | Capability | Description |
|--------|------------|-------------|
| `messaging.send` | `messaging:send` | Send message to peer |
| `messaging.send_group` | `messaging:send` + `messaging:group` | Send to group |
| `messaging.subscribe` | `messaging:subscribe` | Subscribe to messages |
| `messaging.unsubscribe` | `messaging:subscribe` | Cancel subscription |
| `messaging.create_group` | `messaging:group` | Create a group |
| `messaging.list_groups` | `messaging:group` | List groups |

### messaging.send

```typescript
// Request
interface MessagingSendRequest {
  recipient: string;        // IID of recipient
  message_type: string;     // App-defined type
  content: Uint8Array;      // CBOR-encoded content
  reply_to?: string;        // Message ID to reply to
}

// Response
interface MessagingSendResponse {
  message_id: string;       // UUID of sent message
  sent_at: string;          // RFC3339 timestamp
}

// Errors
type MessagingSendError =
  | 'RECIPIENT_NOT_FOUND'
  | 'MESSAGE_TOO_LARGE'
  | 'INVALID_MESSAGE_TYPE'
  | 'PERMISSION_DENIED';
```

### messaging.subscribe

```typescript
// Request
interface MessagingSubscribeRequest {
  filter: MessageFilter;
  callback_entry: string;   // WASM export to call on message
}

interface MessageFilter {
  message_types?: string[];   // Pattern matching (e.g., "com.example.*")
  senders?: string[];         // Specific IIDs
  groups?: string[];          // Specific group IDs
}

// Response
interface MessagingSubscribeResponse {
  subscription_id: string;
}

// Callback (invoked on matching message)
interface MessageCallback {
  message: ReceivedMessage;
}

interface ReceivedMessage {
  id: string;
  sender: string;
  message_type: string;
  content: Uint8Array;
  sent_at: string;
  received_at: string;
  group_id?: string;
  reply_to?: string;
}

// Errors
type MessagingSubscribeError =
  | 'INVALID_FILTER'
  | 'CALLBACK_NOT_FOUND'
  | 'TOO_MANY_SUBSCRIPTIONS'
  | 'PERMISSION_DENIED';
```

### Subscription Lifecycle

Subscriptions have specific lifecycle semantics:

| Event | Behavior |
|-------|----------|
| App instance unloaded | Subscription persists in database |
| Message arrives (app loaded) | Delivered immediately via `handle` |
| Message arrives (app unloaded) | Queued (max 1000 messages per app) |
| App loaded with pending messages | Delivered on next invocation |
| App disabled | Subscriptions paused, messages dropped |
| App uninstalled | Subscriptions deleted, queued messages deleted |
| App updated | Subscriptions preserved if callback exists |
| Capability revoked | Subscription deleted, queued messages deleted |

**Message delivery:**
- Messages are delivered as invocations with `type: 'message'`
- Delivery is at-least-once (apps should handle duplicates via message ID)
- Order is preserved per-sender but not globally
- Apps need `system:background` capability to receive messages when not actively running

**Auto-wake policy:**
- Without `system:background`: messages queue until user opens app
- With `system:background`: app loaded on-demand for delivery
- Rate limit: max 10 auto-wakes per minute per app

### messaging.create_group

```typescript
// Request
interface MessagingCreateGroupRequest {
  name: string;
  description?: string;
  members: string[];          // Initial member IIDs
  settings?: GroupSettings;
}

interface GroupSettings {
  join_rule: 'invite_only' | 'open';
  history_visibility: 'joined' | 'invited' | 'shared' | 'none';
}

// Response
interface MessagingCreateGroupResponse {
  group_id: string;
  created_at: string;
}

// Errors
type MessagingCreateGroupError =
  | 'NAME_TOO_LONG'
  | 'TOO_MANY_MEMBERS'
  | 'MEMBER_NOT_FOUND'
  | 'PERMISSION_DENIED';
```

## Contacts API

### Methods

| Method | Capability | Description |
|--------|------------|-------------|
| `contacts.list` | `contacts:read` | List all contacts |
| `contacts.get` | `contacts:read` | Get contact details |
| `contacts.list_app_users` | `contacts:read:limited` | List contacts using this app |

### contacts.list

```typescript
// Request
interface ContactsListRequest {
  cursor?: string;
  limit?: number;
}

// Response
interface ContactsListResponse {
  contacts: ContactSummary[];
  cursor?: string;
  has_more: boolean;
}

interface ContactSummary {
  iid: string;
  name?: string;
  avatar_hash?: string;
  last_seen?: string;
}

// Errors
type ContactsListError =
  | 'PERMISSION_DENIED';
```

### contacts.list_app_users

```typescript
// Request (no parameters)

// Response
interface ContactsAppUsersResponse {
  contacts: AppUserContact[];
}

interface AppUserContact {
  iid: string;
  name?: string;
  avatar_hash?: string;
  app_data?: Uint8Array;    // App-specific public data
}

// Errors
type ContactsAppUsersError =
  | 'PERMISSION_DENIED';
```

## Sync API

### Methods

| Method | Capability | Description |
|--------|------------|-------------|
| `sync.create_document` | `sync:documents` | Create syncable document |
| `sync.get_document` | `sync:documents` | Get document state |
| `sync.apply_operation` | `sync:documents` | Apply CRDT operation |
| `sync.subscribe` | `sync:documents` | Subscribe to document changes |
| `sync.share` | `sync:documents` | Share document with peer |

### sync.create_document

```typescript
// Request
interface SyncCreateDocumentRequest {
  document_type: string;      // App-defined type
  initial_state?: Uint8Array; // CBOR-encoded initial CRDT state
  access: DocumentAccess;
}

interface DocumentAccess {
  owner: string;              // IID (usually self)
  readers: string[];          // IIDs with read access
  writers: string[];          // IIDs with write access
}

// Response
interface SyncCreateDocumentResponse {
  document_id: string;
  created_at: string;
}

// Errors
type SyncCreateDocumentError =
  | 'INVALID_STATE'
  | 'QUOTA_EXCEEDED'
  | 'PERMISSION_DENIED';
```

### sync.apply_operation

```typescript
// Request
interface SyncApplyOperationRequest {
  document_id: string;
  operation: Uint8Array;      // CBOR-encoded CRDT operation
}

// Response
interface SyncApplyOperationResponse {
  operation_id: string;
  applied_at: string;
}

// Errors
type SyncApplyOperationError =
  | 'DOCUMENT_NOT_FOUND'
  | 'INVALID_OPERATION'
  | 'ACCESS_DENIED'
  | 'PERMISSION_DENIED';
```

## Notifications API

### Methods

| Method | Capability | Description |
|--------|------------|-------------|
| `notifications.show` | `notifications:show` | Display notification |
| `notifications.set_badge` | `notifications:badge` | Set app badge |
| `notifications.cancel` | `notifications:show` | Cancel notification |

### notifications.show

```typescript
// Request
interface NotificationsShowRequest {
  id?: string;                // Optional ID for updates
  title: string;              // Max 100 chars
  body: string;               // Max 500 chars
  icon?: string;              // Asset path
  sound?: boolean;            // Requires notifications:sound
  action?: NotificationAction;
}

interface NotificationAction {
  type: 'open_app' | 'custom';
  data?: Uint8Array;          // Passed to app on action
}

// Response
interface NotificationsShowResponse {
  notification_id: string;
}

// Errors
type NotificationsShowError =
  | 'TITLE_TOO_LONG'
  | 'BODY_TOO_LONG'
  | 'ICON_NOT_FOUND'
  | 'PERMISSION_DENIED';
```

### notifications.set_badge

```typescript
// Request
interface NotificationsSetBadgeRequest {
  count: number;              // 0 = clear badge
}

// Response (empty on success)

// Errors
type NotificationsSetBadgeError =
  | 'PERMISSION_DENIED';
```

## System API

### Methods

| Method | Capability | Description |
|--------|------------|-------------|
| `system.get_time` | `system:time` | Get current time |
| `system.get_random` | `system:random` | Get cryptographic random bytes |
| `system.get_deterministic_random` | (none) | Get deterministic random bytes |
| `system.get_identity` | `system:identity:read` | Get user's identity info |
| `system.get_app_info` | (none) | Get this app's info |

### system.get_time

```typescript
// Request (no parameters)

// Response
interface SystemGetTimeResponse {
  timestamp: string;          // RFC3339 UTC
  monotonic_ns: number;       // Nanoseconds since boot
}
```

### system.get_random

Returns cryptographically secure random bytes. **Do not use for reproducible scenarios.**

```typescript
// Request
interface SystemGetRandomRequest {
  length: number;             // Bytes to generate (max 1024)
}

// Response
interface SystemGetRandomResponse {
  bytes: Uint8Array;
}
```

### system.get_deterministic_random

Returns deterministic random bytes from a PRNG. Safe for games, simulations, testing.
**Never use for security-sensitive operations (keys, nonces, etc.).**

```typescript
// Request
interface SystemGetDeterministicRandomRequest {
  length: number;             // Bytes to generate (max 1024)
  seed?: Uint8Array;          // Optional explicit seed (max 32 bytes)
}

// Response
interface SystemGetDeterministicRandomResponse {
  bytes: Uint8Array;
}
```

If no seed is provided, the PRNG is seeded with `hash(app_id || invocation_id)` for per-invocation reproducibility.

### system.get_identity

```typescript
// Request (no parameters)

// Response
interface SystemGetIdentityResponse {
  iid: string;
  name?: string;
  avatar_hash?: string;
  // Note: No private keys exposed
}
```

### system.get_app_info

```typescript
// Request (no parameters)

// Response
interface SystemGetAppInfoResponse {
  app_id: string;
  version: string;
  installed_at: string;
  storage_used: number;
  capabilities_granted: string[];
}
```

## Inter-App API

### Methods

| Method | Capability | Description |
|--------|------------|-------------|
| `app.invoke` | `app:invoke:{app_id}` | Invoke specific app |
| `app.share` | `app:share:{app_id}` | Share data with specific app |

### app.invoke

Invoke a method on another app. The target app runs under its own capabilities.

```typescript
// Request
interface AppInvokeRequest {
  target_app: string;         // App ID to invoke
  method: string;             // Method to call (must be in target's exports)
  args: Uint8Array;           // CBOR-encoded arguments
  reveal_caller?: boolean;    // Include caller app_id (requires app:invoke:reveal_caller)
}

// Response
interface AppInvokeResponse {
  result: Uint8Array;         // CBOR-encoded result from target
}

// Errors
type AppInvokeError =
  | 'APP_NOT_FOUND'
  | 'APP_NOT_INSTALLED'
  | 'METHOD_NOT_FOUND'
  | 'METHOD_NOT_EXPORTED'
  | 'INVOCATION_FAILED'
  | 'CALL_DEPTH_EXCEEDED'
  | 'PERMISSION_DENIED';
```

### Inter-App Invocation Semantics

| Property | Behavior |
|----------|----------|
| Execution context | Target runs under its own capabilities, not caller's |
| Caller visibility | Target sees caller app_id only if caller has `app:invoke:reveal_caller` AND sets `reveal_caller: true` |
| Sync/async | Synchronous: caller suspends until target returns |
| Max call depth | 8 (prevents A→B→C→...→A cycles) |
| Fuel attribution | Each app pays for its own fuel consumption |
| Timeout | Inherits from caller's remaining timeout |
| Exported methods | Target must declare exportable methods in manifest |

**Manifest export declaration:**
```json
{
  "exports": {
    "methods": ["getData", "processItem"],
    "allow_callers": ["com.example.trusted", "*"]
  }
}
```

If `exports` is not declared, no methods can be invoked by other apps.

### app.share

```typescript
// Request
interface AppShareRequest {
  target_app: string;
  share_type: string;         // App-defined sharing type
  data: Uint8Array;           // Shared data
}

// Response
interface AppShareResponse {
  accepted: boolean;
}

// Errors
type AppShareError =
  | 'APP_NOT_FOUND'
  | 'SHARE_REJECTED'
  | 'DATA_TOO_LARGE'
  | 'PERMISSION_DENIED';
```

## Error Codes

### Common Error Codes

| Code | Description |
|------|-------------|
| `OK` | Success (not an error) |
| `PERMISSION_DENIED` | Missing required capability |
| `INVALID_ARGUMENT` | Malformed request |
| `NOT_FOUND` | Resource doesn't exist |
| `QUOTA_EXCEEDED` | Storage or rate limit exceeded |
| `INTERNAL_ERROR` | Host-side failure |
| `TIMEOUT` | Operation timed out |
| `CANCELLED` | Operation was cancelled |

### Error Response Format

```typescript
interface ErrorResponse {
  code: string;               // Error code
  message: string;            // Human-readable message
  details?: Record<string, unknown>;
}
```

## Versioning

### API Version

Current API version: `1`

Apps declare required API version in manifest:

```json
{
  "dependencies": {
    "api_version": "1"
  }
}
```

### Compatibility

- Minor additions are backwards compatible
- Breaking changes require new major version
- Old APIs deprecated but maintained for 1 year
