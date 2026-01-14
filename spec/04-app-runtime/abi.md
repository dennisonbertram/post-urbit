# Runtime ABI Specification

## Overview

This document is the **authoritative specification** for the Application Binary Interface (ABI) between WASM applications and the host runtime. All other documents defer to this specification for ABI details.

## ABI Version

```
ABI_VERSION = 1
```

Apps declare required ABI version via `dependencies.api_version` in the manifest. The ABI version and API version are the same value.

## Memory Model

### Linear Memory

Apps export a single linear memory that the host uses for all data exchange:

```wasm
(export "memory" (memory $mem))
```

Memory is owned by the guest. The host only writes to memory regions allocated by the guest.

### Allocation Protocol

Apps must export allocation functions:

```wasm
;; Allocate `size` bytes, return pointer to allocated region
;; Returns 0 on allocation failure
(export "alloc" (func $alloc (param $size i32) (result i32)))

;; Free region previously allocated via alloc
(export "dealloc" (func $dealloc (param $ptr i32) (param $size i32)))
```

**Ownership rules:**
1. Host calls `alloc(size)` before writing data to guest memory
2. Guest owns allocated memory after `alloc` returns
3. Guest is responsible for calling `dealloc` when done with host-provided data
4. Host never frees guest memory directly

## Host Imports

Apps may import these functions from the `host` module:

### host.call

Make an asynchronous host API call.

```wasm
(import "host" "call" (func $host_call
  (param $method_ptr i32)    ;; Pointer to UTF-8 method name
  (param $method_len i32)    ;; Length of method name
  (param $args_ptr i32)      ;; Pointer to CBOR-encoded arguments
  (param $args_len i32)      ;; Length of arguments
  (result i32)))             ;; Returns call_id (>0) or error (<0)
```

**Return values:**
- `> 0`: Call ID for retrieving result via `get_result`
- `-1`: Invalid method name
- `-2`: Invalid arguments (not valid CBOR)
- `-3`: Too many outstanding calls (max: 16 per invocation)
- `-4`: Permission denied (missing capability)

### host.get_result

Retrieve the result of a host call.

```wasm
(import "host" "get_result" (func $host_get_result
  (param $call_id i32)       ;; Call ID from host.call
  (param $result_ptr i32)    ;; Pointer to write result (guest-allocated)
  (param $result_max i32)    ;; Maximum bytes to write
  (result i32)))             ;; Returns status code
```

**Return values:**
- `> 0`: Bytes written (result complete)
- `0`: Result not ready (call still pending)
- `-1`: Invalid call_id
- `-2`: Buffer too small (use `get_result_len` to query size)
- `-3`: Call failed (result contains CBOR error)

### host.get_result_len

Query the size of a pending result.

```wasm
(import "host" "get_result_len" (func $host_get_result_len
  (param $call_id i32)       ;; Call ID from host.call
  (result i32)))             ;; Returns length or status code
```

**Return values:**
- `> 0`: Length of result in bytes
- `0`: Result not ready
- `-1`: Invalid call_id

### host.poll

Wait for any pending call to complete.

```wasm
(import "host" "poll" (func $host_poll
  (param $timeout_ms i32)    ;; Maximum milliseconds to wait (0 = non-blocking)
  (result i32)))             ;; Returns completed call_id or status
```

**Return values:**
- `> 0`: Call ID that completed (check with `get_result`)
- `0`: Timeout (no calls completed)
- `-1`: No pending calls

### host.log

Log a message for debugging (always allowed, no capability required).

```wasm
(import "host" "log" (func $host_log
  (param $level i32)         ;; Log level: 0=trace, 1=debug, 2=info, 3=warn, 4=error
  (param $msg_ptr i32)       ;; Pointer to UTF-8 message
  (param $msg_len i32)))     ;; Length of message
```

Messages are truncated at 1024 bytes. Logs are not guaranteed to be persisted.

## Guest Exports

Apps must export these functions:

### _start

Called once when the app is loaded. Used for initialization.

```wasm
(export "_start" (func $start))
```

The app should:
1. Initialize internal state
2. Set up subscriptions via host calls
3. Return promptly (fuel-limited)

### handle

Called for each invocation (user action, background trigger, callback).

```wasm
(export "handle" (func $handle
  (param $input_ptr i32)     ;; Pointer to CBOR-encoded input
  (param $input_len i32)     ;; Length of input
  (result i64)))             ;; Packed result: (ptr << 32) | len
```

**Input format (CBOR):**
```typescript
interface HandleInput {
  type: 'user_action' | 'background' | 'callback' | 'message';
  action?: string;           // For user_action: action identifier
  trigger?: string;          // For background: trigger type
  callback_id?: string;      // For callback: subscription ID
  data: Uint8Array;          // CBOR-encoded payload
}
```

**Return value:**
The return is a packed 64-bit value containing pointer and length:
- Upper 32 bits: pointer to CBOR-encoded result in guest memory
- Lower 32 bits: length of result

If the app returns 0, no result is produced.

**Result format (CBOR):**
```typescript
interface HandleResult {
  ok: boolean;
  value?: unknown;           // Present if ok=true
  error?: {                  // Present if ok=false
    code: string;
    message: string;
  };
}
```

### get_error (optional)

Get the last error message if `handle` trapped.

```wasm
(export "get_error" (func $get_error
  (result i64)))             ;; Packed result: (ptr << 32) | len
```

Returns 0 if no error. Error string is UTF-8 encoded.

## Async Execution Model

The runtime uses a **polling model** for async operations:

```
┌─────────────────────────────────────────────────────────────┐
│  App Invocation                                             │
│                                                             │
│  1. Host calls handle(input_ptr, input_len)                 │
│  2. App processes input, may call host.call() N times       │
│  3. App calls host.poll(timeout) to wait for results        │
│  4. App retrieves results via host.get_result()             │
│  5. App returns result via handle return value              │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Key properties:**
- No callbacks interrupt execution (no reentrancy)
- App controls when to yield via `host.poll()`
- All pending calls complete or timeout before invocation ends
- Subscriptions (messages, sync) are delivered as new invocations, not callbacks

### Outstanding Call Limits

| Resource | Limit |
|----------|-------|
| Max outstanding calls per invocation | 16 |
| Max result retention time | Until invocation ends |
| Max result size | 1 MB |

### Polling Semantics

When `host.poll(timeout)` is called:
1. If any calls are complete, return the first completed call_id immediately
2. If timeout > 0, suspend the WASM instance (saves fuel)
3. Resume when a call completes or timeout expires
4. Return 0 on timeout, call_id on completion

## Result Envelope Format

All host call results use a unified CBOR envelope:

```typescript
// Success
{
  ok: true,
  value: T  // Type depends on method
}

// Error
{
  ok: false,
  error: {
    code: string,      // Error code (e.g., "PERMISSION_DENIED")
    message: string,   // Human-readable message
    details?: object   // Optional additional context
  }
}
```

The return value of `get_result` indicates transport-level status only:
- Positive: bytes written (CBOR envelope in buffer)
- Zero: pending
- Negative: transport error (invalid call_id, buffer too small)

Application-level errors are always returned inside the CBOR envelope with `ok: false`.

## CBOR Encoding

All structured data uses CBOR (RFC 8949) with these constraints:

| Constraint | Value |
|------------|-------|
| Maps | Definite-length only |
| Arrays | Definite-length only |
| Strings | UTF-8 only |
| Integers | Up to 64-bit |
| Tags | None required (may be ignored) |

## Subscription Delivery

Subscriptions (messaging, sync) are delivered as new invocations, not callbacks:

```typescript
// Message delivery input
{
  type: 'message',
  callback_id: '<subscription_id>',
  data: {
    id: string,
    sender: string,
    message_type: string,
    content: Uint8Array,
    sent_at: string,
    received_at: string,
    group_id?: string
  }
}
```

**Delivery semantics:**
- Messages are queued when app is not running
- App is loaded on-demand if it has `system:background` capability
- Delivery is at-least-once (app should handle duplicates)
- Max queue depth per app: 1000 messages

## Inter-App Invocation

When app A invokes app B via `app.invoke`:

1. App A's invocation is suspended
2. App B is loaded if not running
3. App B receives `handle` call with `type: 'app_invoke'`
4. App B returns result
5. App A's `host.get_result` returns with B's result

**Security properties:**
- App B runs under its own capabilities (not A's)
- App B receives A's app_id as metadata (if A has `app:invoke:reveal_caller`)
- Max call depth: 8 (prevents A→B→C→...→A cycles)
- Fuel is charged to the app consuming it (A pays for A's code, B pays for B's)

## Transaction Semantics

Each invocation has implicit transaction semantics for storage:

| Operation | Behavior |
|-----------|----------|
| Storage writes | Atomic per invocation |
| On successful return | All writes committed |
| On trap/timeout | All writes rolled back |
| On fuel exhaustion | All writes rolled back |
| Messaging sends | NOT rolled back (fire-and-forget) |

Apps that need multi-step transactions should use optimistic concurrency (storage versions).

## Common Limits

| Resource | Limit |
|----------|-------|
| Method name length | 64 bytes |
| Key length (storage) | 256 bytes |
| Value size (storage) | 1 MB |
| Message size | 64 KB |
| Notification title | 100 characters |
| Notification body | 500 characters |
| Group name | 100 characters |
| Max subscriptions per app | 100 |
| Max groups per app | 1000 |
| Cursor string | Opaque, max 256 bytes |
| Message type pattern | Max 128 bytes |

## Method String Format

Host API method names use `snake_case`:

```
storage.get
storage.set
storage.delete
storage.list
storage.shared.get
storage.shared.set
messaging.send
messaging.send_group
messaging.subscribe
messaging.unsubscribe
messaging.create_group
messaging.list_groups
contacts.list
contacts.get
contacts.list_app_users
sync.create_document
sync.get_document
sync.apply_operation
sync.subscribe
sync.share
notifications.show
notifications.set_badge
notifications.cancel
system.get_time
system.get_random
system.get_identity
system.get_app_info
app.invoke
app.share
```

Unknown methods return error envelope with code `METHOD_NOT_FOUND`.
