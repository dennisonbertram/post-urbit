Below are concrete, production-oriented recommendations to tighten **Post-Urbit’s Secure Bridge Protocol** as the *only* IPC between sandboxed app webviews and the Rust backend. I’m assuming the transport is your existing single Tauri command:

- `postbridge_invoke(webview, state, request_bytes: Vec<u8>) -> Vec<u8>`
- with **identity derived from `webview.label()`** (`app-{app_id}`), never from payload.

I’ll separate **wire protocol** (CBOR envelopes), **session/auth**, **authorization**, and **streaming** so the spec is transport-agnostic even though the implementation is one Tauri command.

---

## 1) Message format & serialization (CBOR)

### 1.1 Define a *strict* CBOR profile (MUST-level)
CBOR is flexible; flexibility is where parsing and security bugs live. Specify a profile the backend enforces:

**MUST**
- Use **CBOR map** at top-level with **fixed required keys**.
- Reject:
  - **indefinite-length** items
  - **duplicate map keys**
  - **non-UTF8 text**
  - floats/NaN (unless explicitly needed)
  - tags (unless explicitly whitelisted)
- Enforce hard limits during decode:
  - max nesting depth (e.g., 16–32)
  - max map/array length (e.g., 1k)
  - max string length (e.g., 8–64KB depending)
  - max total decoded bytes (your repo already targets **256KB** per request; enforce pre-decode too)

**SHOULD**
- Use **canonical CBOR** (RFC 8949 canonical form) for deterministic encoding *if you ever sign payloads or hash params*, and for test vectors. Even if you don’t sign, canonicalization reduces “weird CBOR” edge cases.
- Prefer **byte strings** for binary fields (token, ids) instead of base64 text to avoid normalization issues.

### 1.2 Strongly type the envelope (and version it)
Your current request/response envelopes are good; make them **normative** and tighten field types:

**BridgeRequest (v=1)**
- `v`: uint (exactly 1)
- `id`: **16-byte bstr UUID** (or UUID string, but pick one and standardize)
- `ts`: uint64 (ms since epoch)
- `session`: bstr/uuid
- `token`: bstr (32 bytes HMAC output) or bstr variable if you change algorithms
- `method`: text, restricted charset (e.g. `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$`)
- `params`: any (but subject to limits)

**BridgeResponse**
- `v`, `id`, `ts`, `ok`
- if `ok=true`: `result` present, `error` absent
- if `ok=false`: `error` present, `result` absent

### 1.3 Add protocol-control metadata fields
To make production ops and safety easier, add optional envelope metadata:

- `deadline_ms` (request): client hint; backend may clamp.
- `trace_id` (request): opaque, for correlating logs across layers.
- `idempotency_key` (request): optional; see §5.

### 1.4 Publish CDDL + test vectors
Production-ready specs include:
- **CDDL schema** for request/response/event
- CBOR **hex test vectors** for:
  - a valid request
  - invalid version
  - oversized payload
  - unknown method
  - permission denied
  - replayed request

---

## 2) Session/token management & anti-replay

### 2.1 Bind sessions to infrastructure identity (webview label)
Your code already derives app identity from `webview.label()`. Extend session state to bind to the webview:

**Session record SHOULD include**
- `app_id`
- `webview_label` (or a stable internal webview instance id)
- `created_at`, `expires_at`
- `capabilities` snapshot
- replay/ordering state (below)

**On every request MUST verify**
- `label starts_with "app-"` (you do)
- derived `app_id == session.app_id` (you do)
- (recommended) `webview_label == session.bound_label`  
  This prevents a stolen token being used from a different webview process.

### 2.2 Token design: keep it simple, rotateable
Your HMAC token is fine. Production recommendations:

**MUST**
- Use a **per-install secret** stored in OS key store (Keychain / DPAPI / libsecret) or a hardened on-disk secret with file permissions.
- Use **constant-time compare** (you do conceptually).
- Include `session_id`, `app_id`, `iat`, and a random `nonce` in the MAC input (you do).

**SHOULD**
- Support secret rotation with `kid` (key id) in the token payload, or maintain `{current, previous}` secrets for a rotation window.
- Avoid base64 text token inside CBOR; prefer `bstr` raw bytes.

### 2.3 Anti-replay: don’t just “reject duplicates”; support safe retries
Your current replay mitigation stores `seen_request_ids`. Two production issues:
1) memory growth / cleanup strategy
2) client retries after timeouts become “replay detected” and can cause duplicate side effects if clients generate new ids

**Recommended model**
- Maintain a bounded **LRU “request cache”** per session keyed by `id` with values:
  - `{received_at, response_bytes_hash or full response_bytes, status}`
- Behavior:
  - If `(session,id)` is seen again **within replay_window**:
    - return the **same response** (idempotent retry)
    - do **not** re-execute side effects
  - If seen outside the window: treat as invalid/replay

This gives you:
- replay protection
- safe retry semantics
- deterministic behavior under loss/timeouts

**Alternative / additional**
- Add `seq` (monotonic uint64) and enforce a sliding window for out-of-order concurrency. This reduces storage versus UUID sets, but is more complex.

### 2.4 Timestamp checking: keep, but make it clearly secondary
Timestamp window checks are fine, but they’re not replay protection by themselves. Keep:
- reject if `abs(now - ts) > skew_ms` (e.g., 5 min)
- specify server clock source
- specify error code (but see §4 on avoiding enumeration)

---

## 3) Method namespace & permission model

### 3.1 Define namespaces and reserve protocol methods
Define a strict method naming policy and reserve prefixes:

**Reserved (protocol/control)**
- `bridge.ping`
- `bridge.batch`
- `bridge.capabilities` (optional introspection)
- `events.subscribe`
- `events.poll`
- `events.unsubscribe`
- `events.ack` (optional)
- `system.get_identity` (safe, minimal)
- `system.refresh_session` (optional)

**Domain namespaces**
- `storage.*`
- `resource.*`
- `permission.*`
- `shell.*` (should be **shell-only**; apps MUST be denied regardless of payload)

### 3.2 Capability model: method → capability mapping (default deny)
Since apps only have the one Tauri command permission, your bridge handler MUST enforce app-level permissions.

**MUST**
- Implement **default deny**: unknown method => deny.
- Method authorization MUST be computed from:
  - `session.capabilities` (minted by Rust)
  - `app_id` derived from webview label
  - optional runtime state (focused, user gesture, etc.)

**SHOULD**
- Capabilities should be **non-wildcard** for third-party apps. Avoid patterns like `storage:*` unless you have a strong reason.
- Use stable capability strings like:
  - `storage.read`, `storage.write`
  - `clipboard.write`
  - `external.open_url`
  - `contacts.read`
- For “dangerous” actions require both:
  - capability granted
  - and a *per-request policy check* (user prompt, rate limit, focus, etc.)

### 3.3 Don’t allow apps to select their security context in params
**MUST**
- Never accept `app_id`, `session_id`, “tenant”, filesystem paths, etc. as authority-bearing parameters unless they’re treated as *data* and re-validated.
- All partitioning keys MUST come from infrastructure identity:
  - `app_id` from `webview.label()`
  - session from server store

This prevents classic “confused deputy / tenant breakout” bugs.

---

## 4) Error handling & security-sensitive messaging

### 4.1 Use stable error codes, but avoid oracle leaks
You want consistent developer ergonomics *without* enabling probing (method enumeration, session guessing).

**Recommendation**
- Return a stable machine code set (similar to your table), but collapse sensitive auth failures:

For example:
- `UNAUTHORIZED` for:
  - invalid session
  - invalid token
  - expired session
  - session/app mismatch
- `NOT_FOUND` for unknown method (no “did you mean”)
- `PERMISSION_DENIED` only when session is valid but capability missing (optional; if you want less oracle power, fold into `UNAUTHORIZED` too)

### 4.2 Split “public” vs “internal” error detail
**MUST**
- App-visible `error.message` must not include:
  - filesystem paths
  - internal object ids
  - stack traces
  - database details
- Include an `error_id` (UUID) for internal log correlation.

**Example**
```json
{ "code":"INTERNAL_ERROR", "message":"Request failed", "retryable":true, "details":{ "error_id":"..." } }
```

### 4.3 Rate limit errors should include `retry_after_ms`
Make client behavior predictable:

- `RATE_LIMITED` with `details.retry_after_ms`
- optionally also `details.limit` / `details.burst`

---

## 5) Correlation, timeouts, and idempotency

### 5.1 Correlation rules
**MUST**
- `response.id == request.id`
- Backend logs MUST include `(app_id, session_id, request_id, method)`.

**SHOULD**
- Add `server_ts` and `processing_ms` to responses (helps observability and debugging without leaking sensitive info).

### 5.2 Server-side timeouts and cancellation boundaries
Client timeouts alone are insufficient. Production backend MUST:
- enforce a **global per-request timeout** (e.g., 30s)
- enforce **per-method** tighter timeouts (e.g., 2s for storage reads, 10s for network-less operations)
- use `tokio::time::timeout` around handler execution
- ensure timed-out requests:
  - don’t keep expensive work running in background unless explicitly designed

### 5.3 Idempotency semantics (critical for “send”, “write”, etc.)
Specify per method whether it is:
- **idempotent** (safe to retry)
- **non-idempotent** (requires idempotency key)
- **exactly-once best-effort** (via request cache described in §2.3)

Concrete recommendation:
- Treat `id` as the idempotency key within the replay window and return cached responses for duplicates.
- For operations where the side effect might occur after a timeout, this prevents double-execution on retry.

---

## 6) Streaming and subscription patterns (while keeping a single IPC command)

If you truly want **only `postbridge_invoke`** as the IPC mechanism, then backend → app “push” must happen via *app-initiated polling* (long poll). Avoid relying on Tauri event/listen as a second channel.

### 6.1 Subscriptions via long-poll (recommended)
Define methods:

- `events.subscribe { topic, filter } -> { subscription_id, starting_seq }`
- `events.poll { subscription_id, after_seq, timeout_ms, max_events } -> { events[], last_seq, dropped?:bool }`
- `events.ack { subscription_id, up_to_seq }` (optional; allows trimming buffers)
- `events.unsubscribe { subscription_id }`

**MUST**
- subscription is scoped to `(session_id, app_id)`
- enforce `max_subscriptions_per_session`
- enforce `max_pending_events` and a drop policy:
  - drop oldest and set `dropped=true`, or
  - drop newest and set `dropped=true`
- enforce `timeout_ms` clamp (e.g., max 30s) to prevent “infinite hang” resource pinning

### 6.2 Streaming large payloads (chunking)
Given your **256KB payload cap**, define chunked transfers for bigger blobs:

- `blob.put_start { total_bytes, sha256?, content_type? } -> { upload_id }`
- `blob.put_chunk { upload_id, offset, chunk:bstr } -> { next_offset }`
- `blob.put_finish { upload_id } -> { blob_id }`
- `blob.get_chunk { blob_id, offset, max_bytes } -> { chunk, next_offset, done }`

**MUST**
- cap `total_bytes` and `max_bytes`
- require content hash for integrity if blobs are stored
- tie uploads to session/app_id

---

## 7) Attack vectors to mitigate (and concrete mitigations)

### Replay & retry confusion
- Mitigation: request cache returning same response for duplicate `(session,id)`; bounded TTL/LRU.

### TOCTOU (especially prompts / external opens / quota increases)
- Mitigation: prompt dialogs MUST display *exact* parameters that will be executed, and backend MUST execute exactly those parameters (no later mutation).
- If a request needs user confirmation, consider a **two-step** flow:
  1) `permission.prepare_action` → returns `action_token` bound to params + expiry
  2) `permission.execute_action { action_token }`
  This prevents racey “approve one thing, execute another”.

### Amplification (small request → huge response)
- Mitigation: cap response sizes; for list APIs require pagination; for logs/exports require chunking.

### Enumeration (methods, resources, sessions)
- Mitigation:
  - uniform `UNAUTHORIZED` for auth failures
  - unknown method => generic `NOT_FOUND`
  - avoid detailed “permission missing X” unless you accept that leakage
  - avoid timing differences (don’t do expensive lookups before auth)

### Parser bombs / memory exhaustion
- Mitigation: CBOR limits (depth, collection size), reject indefinite lengths, hard cap bytes before decode.

### Flooding / CPU exhaustion
- Mitigation:
  - token bucket rate limiting per session (you already plan this in resource constraints)
  - `max_concurrent_requests` (you already plan 16)
  - per-method cost limits (e.g., `storage.scan` expensive; require pagination + rate limit)

### Cross-app confused deputy
- Mitigation: never accept `app_id` in params for scoping; always derive from webview label and session binding.

---

## 8) What must be specified to be production-ready (minimum checklist)

To move from “design” to “spec you can ship and audit”, the document should explicitly specify:

1) **Transport binding**
- “Bridge messages are CBOR-encoded and carried over the single Tauri command `postbridge_invoke`.”
- Max request/response bytes.
- Concurrency semantics.

2) **Formal schemas**
- CDDL for request/response/event.
- Field types, required/optional, unknown field handling (recommend: reject unknown fields for v1).

3) **State machines**
- Session lifecycle: created → active → expired → revoked
- What happens on:
  - app eviction to cold
  - webview crash
  - shell restart
  - clock skew

4) **Key management**
- Where HMAC secret lives
- rotation strategy
- backup/restore behavior (important for user migrations)

5) **Authorization rules**
- canonical capability strings
- mapping method → capability (table)
- rules for user-prompted permissions (when required, UI constraints)

6) **Anti-replay + idempotency semantics**
- replay window definition
- duplicate request handling (reject vs cached response; I recommend cached)
- method idempotency classification

7) **Backpressure + resource limits**
- per-session rate limits (rps, burst)
- max in-flight
- subscription buffer sizes and drop behavior
- long-poll timeout clamps

8) **Error taxonomy**
- stable codes
- retryability rules
- what is safe to expose vs internal-only

9) **Security considerations section**
- explicitly list threats and mitigations (replay, TOCTOU, enumeration, DoS)
- include “MUST NOT rely on app-provided app_id” as a highlighted invariant

10) **Conformance + hardening**
- fuzz CBOR decoder with corpora
- property tests for replay window, duplicate id behavior, permission enforcement
- test vectors for canonical CBOR parsing

---

### One high-impact alignment note (based on your repo)
Your `docs/SECURE_BRIDGE_PROTOCOL.md` currently centers a **MessageChannel handshake** (iframe model). But your architecture spec is **multi-webview** and your enforcement point is **`postbridge_invoke`**. For production clarity, I recommend rewriting the bridge spec to explicitly treat:

- **Handshake/session minting**: shell-only command, session bound to `app-{app_id}` label
- **Transport**: single `postbridge_invoke` CBOR request/response
- **Subscriptions**: long-poll via `events.poll` (so you truly keep a single IPC primitive)

If you want, I can propose an updated “v1 normative spec” outline (sections + exact tables for method→capability and envelope CDDL) that matches your `postbridge_invoke` implementation model.
