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
- `-1`: Invalid method name bytes (not valid UTF-8 or exceeds 64 bytes)
- `-2`: Invalid arguments (not valid CBOR)
- `-3`: Too many outstanding calls (max: 16 per invocation)

**Error model (Normative):** `host.call` returns a positive call_id for ANY syntactically valid method name (valid UTF-8, ≤64 bytes) and valid CBOR arguments, regardless of whether the method exists or the app has permission to call it. All method-level errors (permission denied, method not found, not implemented, validation errors) are returned as CBOR envelopes with `ok: false` via `get_result`. Negative `host.call` values are reserved for ABI-level failures only (malformed input, resource exhaustion).

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
- `> 0`: Bytes written (result complete, buffer contains CBOR envelope)
- `0`: Result not ready (call still pending)
- `-1`: Invalid call_id
- `-2`: Buffer too small (use `get_result_len` to query size)

**Note:** Negative return values indicate transport-level errors only. Application-level call failures (e.g., permission denied, method not found) return `> 0` bytes containing a CBOR envelope with `ok: false`. See "Result Envelope Format" below.

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

**Return buffer ownership (Normative):**
- Guest allocates and owns the result buffer
- Guest MUST keep the buffer valid and unchanged until `handle` returns [REQ-APP-001]
- Host MUST copy result bytes immediately upon `handle` return (host does NOT call `dealloc`) [REQ-APP-002]
- Guest MAY reuse or free the buffer after `handle` returns [REQ-APP-003]
- Host MUST validate `(ptr, len)`: if ptr is 0 and len > 0, or if `ptr + len` exceeds memory bounds, treat as trap [REQ-APP-004]
- Maximum result size: 1 MB (1048576 bytes); larger results are truncated

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

All structured data exchanged via `host.call` arguments and results, as well as `handle` input and output, uses CBOR (RFC 8949). This section specifies the **normative encoding rules** that implementations MUST follow to ensure interoperability. [REQ-APP-005]

### CBOR Wire Schema (Normative)

The TypeScript interfaces in this document and `api-surface.md` define the **logical structure** of messages. The following rules specify the **exact CBOR encoding** for wire transmission:

#### Type Mapping

| TypeScript Type | CBOR Encoding | Notes |
|-----------------|---------------|-------|
| `string` | Text string (major type 3) | MUST be valid UTF-8  [REQ-APP-006]|
| `number` (integer) | Unsigned/signed integer (major type 0/1) | Use smallest encoding that fits |
| `number` (float) | Float64 (0xFB prefix) | Only when fractional; prefer integers |
| `boolean` | `true` (0xF5) or `false` (0xF4) | Simple values |
| `null` | `null` (0xF6) | Only for explicitly nullable fields |
| `Uint8Array` / byte array | Byte string (major type 2) | NOT array of integers |
| `Array<T>` | Array (major type 4) | Definite-length only |
| `object` / struct | Map (major type 5) | Text string keys, definite-length |

#### Struct Encoding Rules

1. **Map keys:** Structs MUST be encoded as CBOR maps with **text string keys** (major type 3). Keys MUST use `snake_case` matching the field names in the TypeScript interfaces. [REQ-APP-007]

2. **Required fields:** All required fields MUST be present in the map. Missing required fields MUST cause decoding to fail with error code `INVALID_ARGUMENT`. [REQ-APP-008]

3. **Optional fields:** Optional fields (marked with `?` in TypeScript) MUST be **omitted entirely** when the value is absent. Do NOT encode absent optional fields as CBOR `null`. This reduces message size and distinguishes "not provided" from "explicitly null". [REQ-APP-009]

4. **Nullable fields:** For fields that can hold an explicit `null` value (e.g., `value: T | null`), encode the null case as CBOR `null` (0xF6). These are distinct from optional fields.

5. **Unknown fields:** Decoders MUST ignore unknown map keys (forward compatibility). Encoders MUST NOT add keys not defined in the schema. [REQ-APP-010]

#### Integer Encoding

1. **Smallest encoding:** Integers MUST use the smallest CBOR encoding that fits the value: [REQ-APP-011]
   - 0-23: Single byte (major type 0 + value)
   - 24-255: Two bytes (0x18 + uint8)
   - 256-65535: Three bytes (0x19 + uint16)
   - 65536-4294967295: Five bytes (0x1A + uint32)
   - Larger: Nine bytes (0x1B + uint64)

2. **Signed integers:** Negative integers use major type 1 with the same size rules.

3. **64-bit range:** Implementations MUST handle unsigned integers up to 2^64-1 and signed integers in the range -(2^63) to 2^63-1. [REQ-APP-012]

4. **JavaScript interop:** Values in the range 0 to 2^53-1 are safe for JavaScript `number`. Larger values MUST be handled as BigInt or similar in JavaScript environments. The `api-surface.md` schemas indicate which fields may exceed this range. [REQ-APP-013]

#### Binary Data Encoding

1. **Byte strings:** All binary data (`Uint8Array` in TypeScript) MUST be encoded as CBOR byte strings (major type 2), NOT as arrays of integers. [REQ-APP-014]

2. **Definite length:** Byte strings MUST use definite-length encoding (length prefix), NOT indefinite-length streaming. [REQ-APP-015]

3. **Example:** `Uint8Array([0xDE, 0xAD, 0xBE, 0xEF])` encodes as `44 DE AD BE EF` (byte string, length 4).

#### String Encoding

1. **UTF-8:** All text strings MUST be valid UTF-8. Invalid UTF-8 sequences MUST cause decoding to fail. [REQ-APP-016]

2. **Definite length:** Text strings MUST use definite-length encoding. [REQ-APP-017]

3. **No BOM:** Text strings MUST NOT include a UTF-8 BOM (0xEF 0xBB 0xBF). [REQ-APP-018]

#### Structural Constraints

| Constraint | Requirement |
|------------|-------------|
| Maps | Definite-length only (no 0xBF) |
| Arrays | Definite-length only (no 0x9F) |
| Strings | UTF-8 only, definite-length |
| Integers | Up to 64-bit |
| Tags | MUST be ignored by decoders; encoders SHOULD NOT emit tags  [REQ-APP-019]|
| Floating-point | Float64 (0xFB) only; Float16/Float32 MUST NOT be used  [REQ-APP-020]|
| Special values | `undefined` (0xF7), `break` (0xFF) MUST NOT be used  [REQ-APP-021]|

### Deterministic CBOR (Normative)

All CBOR encoding in the ABI MUST follow **deterministic encoding rules** per RFC 8949 Section 4.2. This ensures that: [REQ-APP-022]
- Identical logical values produce identical byte sequences
- Hashing and signing operations are reproducible
- Test vectors can be precisely verified

**Deterministic encoding requirements:**

1. **Map key ordering:** Map keys MUST be sorted in bytewise lexicographic order of their encoded CBOR representation. For text string keys (as required by this spec), this means: [REQ-APP-023]
   - Shorter keys sort before longer keys
   - Keys of equal length sort by byte comparison

2. **Preferred integer encoding:** Use the smallest integer encoding (as specified above).

3. **No duplicate keys:** Maps MUST NOT contain duplicate keys. [REQ-APP-024]

4. **Preferred float encoding:** Floating-point values that can be represented exactly as integers MUST be encoded as integers instead. [REQ-APP-025]

5. **Preferred length encoding:** Use the smallest length prefix for strings, arrays, and maps.

**Example key ordering:**
```
Keys: ["a", "ab", "b", "aa"]
Sorted: ["a", "b", "aa", "ab"]  // length first, then byte comparison
CBOR: 61 61, 61 62, 62 61 61, 62 61 62
```

**Consistency with sync protocol:** These rules are consistent with the CBOR canonicalization specified in `sync-protocol.md` Section "CBOR Canonicalization (Normative)". Both the ABI and sync protocol use RFC 8949 Section 4.2 deterministic encoding.

### Example Encodings

**HandleInput (user_action):**
```typescript
{
  type: 'user_action',
  action: 'submit_form',
  data: Uint8Array([0x01, 0x02, 0x03])
}
```

CBOR (hex, deterministic key order: "data" < "type" < "action" by length-first sorting):
```
A3                                      // map(3)
  64 64 61 74 61                        // text(4) "data"
  43 01 02 03                           // bytes(3)
  64 74 79 70 65                        // text(4) "type"
  6B 75 73 65 72 5F 61 63 74 69 6F 6E   // text(11) "user_action"
  66 61 63 74 69 6F 6E                  // text(6) "action"
  6B 73 75 62 6D 69 74 5F 66 6F 72 6D   // text(11) "submit_form"
```

**Result envelope (success):**
```typescript
{
  ok: true,
  value: { message_id: "abc123", sent_at: "2025-01-15T12:00:00Z" }
}
```

CBOR (hex, deterministic key order: "ok" < "value"):
```
A2                                      // map(2)
  62 6F 6B                              // text(2) "ok"
  F5                                    // true
  65 76 61 6C 75 65                     // text(5) "value"
  A2                                    // map(2) - nested, keys: "sent_at" < "message_id"
    67 73 65 6E 74 5F 61 74             // text(7) "sent_at"
    74 32 30 32 35 2D 30 31 2D 31 35    // text(20) "2025-01-15T12:00:00Z"
       54 31 32 3A 30 30 3A 30 30 5A
    6A 6D 65 73 73 61 67 65 5F 69 64    // text(10) "message_id"
    66 61 62 63 31 32 33                // text(6) "abc123"
```

**Result envelope (error):**
```typescript
{
  ok: false,
  error: { code: "PERMISSION_DENIED", message: "Missing capability" }
}
```

CBOR (hex, deterministic key order: "ok" < "error"; nested: "code" < "message"):
```
A2                                      // map(2)
  62 6F 6B                              // text(2) "ok"
  F4                                    // false
  65 65 72 72 6F 72                     // text(5) "error"
  A2                                    // map(2)
    64 63 6F 64 65                      // text(4) "code"
    71 50 45 52 4D 49 53 53 49 4F 4E    // text(17) "PERMISSION_DENIED"
       5F 44 45 4E 49 45 44
    67 6D 65 73 73 61 67 65             // text(7) "message"
    72 4D 69 73 73 69 6E 67 20 63 61    // text(18) "Missing capability"
       70 61 62 69 6C 69 74 79
```

### CBOR Libraries and Implementation Notes

Implementations SHOULD use well-tested CBOR libraries that support deterministic encoding: [REQ-APP-026]

- **Rust:** `ciborium` with deterministic mode, or `minicbor`
- **JavaScript/TypeScript:** `cbor-x` with `canonical: true`, or `cborg`
- **Go:** `fxamacker/cbor/v2` with `CanonicalEncMode`
- **C/C++:** `libcbor` or `tinycbor`

**Validation:** Implementations SHOULD validate that decoded CBOR matches expected types. Type mismatches (e.g., integer where string expected) MUST cause decoding to fail with `INVALID_ARGUMENT`. [REQ-APP-027]

### Reference

- **RFC 8949:** CBOR (Concise Binary Object Representation) - https://www.rfc-editor.org/rfc/rfc8949
- **RFC 8949 Section 4.2:** Deterministically Encoded CBOR

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
| Message size | 1 MB (1048576 bytes) |
| Notification title | 100 characters |
| Notification body | 500 characters |
| Group name | 100 characters |
| Max subscriptions per app | 100 |
| Max groups per app | 1000 |
| Cursor string | Opaque, max 256 bytes |
| Message type pattern | Max 128 bytes |

## Method String Format

Host API method names use `snake_case`. Methods are categorized as **v1** (fully specified in api-surface.md) or **reserved** (registered for future use; schema intentionally unspecified in v1):

| Method | Status | Schema Location |
|--------|--------|-----------------|
| `storage.get` | v1 | api-surface.md |
| `storage.set` | v1 | api-surface.md |
| `storage.delete` | v1 | api-surface.md |
| `storage.list` | v1 | api-surface.md |
| `storage.shared.get` | reserved | — |
| `storage.shared.set` | reserved | — |
| `messaging.send` | v1 | api-surface.md |
| `messaging.send_group` | v1 | api-surface.md |
| `messaging.subscribe` | v1 | api-surface.md |
| `messaging.unsubscribe` | reserved | — |
| `messaging.create_group` | v1 | api-surface.md |
| `messaging.list_groups` | reserved | — |
| `contacts.list` | v1 | api-surface.md |
| `contacts.get` | reserved | — |
| `contacts.list_app_users` | v1 | api-surface.md |
| `sync.create_document` | v1 | api-surface.md |
| `sync.get_document` | reserved | — |
| `sync.apply_operation` | v1 | api-surface.md |
| `sync.subscribe` | reserved | — |
| `sync.share` | reserved | — |
| `notifications.show` | v1 | api-surface.md |
| `notifications.set_badge` | v1 | api-surface.md |
| `notifications.cancel` | reserved | — |
| `system.get_time` | v1 | api-surface.md |
| `system.get_random` | v1 | api-surface.md |
| `system.get_deterministic_random` | v1 | api-surface.md |
| `system.get_identity` | v1 | api-surface.md |
| `system.get_app_info` | v1 | api-surface.md |
| `app.invoke` | v1 | api-surface.md |
| `app.share` | reserved | — |

**Reserved methods:** Calling a reserved method returns `{ ok: false, error: { code: "NOT_IMPLEMENTED", message: "Method reserved for future use" }}`. Reserved methods are out-of-scope for v1 and MUST NOT be used on the wire.

Unknown methods return error envelope with code `METHOD_NOT_FOUND`.
