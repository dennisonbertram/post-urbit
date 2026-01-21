## 1) Required Sections (what the SDK & DX spec MUST cover)

1. **Scope, goals, non-goals**
   - Explicitly tie back to security invariants from specs 02/04/06: *single IPC command*, *identity from webview label*, *fail closed*, *TOCTOU* for prompted actions.
   - State what the SDK will *not* provide (no networking helpers, no filesystem, no direct Tauri API surface, no privileged shell APIs).

2. **Runtime environment contract**
   - “Apps run in isolated app webviews loaded from `postapp://{app_id}/…` with CSP `connect-src 'none'`”
   - “Only IPC available is `postbridge_invoke(request_bytes)`”
   - Size/time/concurrency limits and their developer-facing implications.

3. **SDK distribution + compatibility**
   - Packaging: ESM-first + optional UMD build.
   - Supported JS runtimes: browser/webview only (no Node).
   - Versioning policy: `sdk_major` must match `bridge.v`; additive methods are minor; removals are major.
   - Backward compatibility rules (what happens if app uses newer SDK than shell supports).

4. **Bootstrap & session contract**
   - How session/token is provided to apps (MUST be out-of-band because requests require `session` + `token`).
   - Token handling rules (sensitive, never persisted, rotation/expiry behavior).

5. **Bridge proxy design**
   - Transport (Tauri invoke) hidden behind a thin proxy.
   - CBOR encoding/decoding requirements.
   - Request ID generation, deadlines, idempotency keys, trace IDs.
   - Error mapping and retry semantics (RATE_LIMITED, TIMEOUT, UNAUTHORIZED collapse).

6. **Typed method surface**
   - Canonical method list, params, results (derived from CDDL).
   - Namespacing rules and reserved prefixes.
   - Optional helper wrappers (events loop, blob chunking, permission TOCTOU helpers).

7. **Permissions & TOCTOU developer workflow**
   - How apps request permissions at install/runtime.
   - How “PromptAlways” methods must be executed via `permission.prepare_action` → user confirms in shell → `permission.execute_action`.
   - SDK helpers to reduce footguns (polling, backoff, UX guidance).

8. **Local dev workflow**
   - Creating an app, running it locally in the shell, reloading, viewing logs, inspecting bridge traffic.
   - Dev-mode security posture and explicit user indication.

9. **Security constraints**
   - A concrete “MUST NOT expose” list (see section 7 below) plus how the SDK enforces/encourages it.

10. **Acceptance criteria & test plan**
   - Cross-webview isolation assumptions validated.
   - Session binding + replay + rate limit behavior validated end-to-end.

---

## 2) SDK Architecture (what is exposed and how)

### Packages (recommended)
- `@posturbit/sdk` (runtime)
  - Bridge transport + CBOR codec + typed client + helpers.
- `@posturbit/protocol` (generated types + method map)
  - TS types generated from CDDL + a machine-readable registry (method → tier/caps/timeout/idempotent).
- `@posturbit/devtools` (optional)
  - Bridge inspector UI hooks, log formatting, mock transport.

### Layering (MUST be explicit in spec)
1. **Transport (private)**
   - Calls Tauri `invoke('postbridge_invoke', { requestBytes })`.
   - No re-export of `invoke`, no general-purpose IPC.

2. **Codec**
   - CBOR encode/decode (definite-length only).
   - Enforces *client-side* request/response size ceilings (256KB) to fail fast before invoking.

3. **Protocol client**
   - Builds `BridgeRequest` envelope `{v,id,ts,session,token,method,params,…}`.
   - Validates `BridgeResponse` envelope and maps errors to SDK error classes.

4. **Typed façade**
   - `client.storage.get(...)`, `client.events.subscribe(...)`, etc.
   - Optional “ergonomic” helpers:
     - `client.events.on(topic, handler)` (wraps subscribe + long-poll loop)
     - `client.blob.put(data)` (wraps chunked upload)
     - `client.actions.clipboard.writeText(text)` (wraps TOCTOU prepare/execute + polling)

### Single global entry point
The spec should standardize either:
- ESM: `import { createClient } from '@posturbit/sdk'`
- Optional global (only if needed): `window.PostUrbit.createClient()`

Avoid multiple globals; avoid leaking internal transport.

---

## 3) Bootstrap Flow (how apps initialize and get session/token)

Because the bridge envelope requires `session` and `token`, bootstrap **MUST be out-of-band**.

### Recommended mechanism: shell-injected bootstrap object
On webview creation, the Rust backend injects an init script at **document_start** that defines a frozen object:

```ts
// injected by backend (document_start)
window.__POSTURBIT_BOOTSTRAP__ = Object.freeze({
  bridge_v: 1,
  app_id: "com.example.notes",
  session_id: "uuid",
  token: "kid.signature",
  issued_at_ms: 1234567890,
  expires_at_ms: 1234567890,
  // optional: build/dev flags, protocol capabilities snapshot, etc.
});
```

**SDK behavior:**
- `createClient()` reads `window.__POSTURBIT_BOOTSTRAP__`
- Validates fields and `bridge_v` compatibility
- Never writes token/session to storage
- If missing/invalid → throws `BootstrapError` with a safe message

### Session expiry handling
Define a required behavior:
- If a request returns `UNAUTHORIZED`, SDK:
  1) stops any background poll loops
  2) surfaces an error instructing app to reload
  3) optionally calls a *non-privileged* “bridge.ping” to confirm connectivity
- Do **not** attempt to create a new session from the app side (consistent with “shell-only session creation”).

### Dev-mode considerations
If dev mode needs different bootstrap fields (e.g., relaxed CSP flags), require:
- shell must be explicitly launched in dev mode
- visible “DEV MODE” indicator in shell chrome
- bootstrap includes `dev_mode: true` so SDK can enable extra diagnostics *without* changing security-relevant behavior.

---

## 4) API Design (core API surface for apps)

### Minimal core API (MUST)
```ts
type InvokeOptions = {
  deadlineMs?: number;         // client hint, clamped by backend
  traceId?: string;
  idempotencyKey?: string;
  timeoutMs?: number;          // client-side abort; must not exceed 30s
};

interface PostUrbitClient {
  invoke<M extends MethodName>(
    method: M,
    params: MethodParams[M],
    options?: InvokeOptions
  ): Promise<MethodResult[M]>;
}
```

### Typed namespaces (SHOULD)
Expose first-class wrappers for all non-shell methods in spec 04/06:

- `bridge.ping()`
- `storage.get/set/delete/list`
- `system.get_time`, `system.get_identity`
- `resource.get_budget`, `resource.get_storage_usage`, `resource.request_quota_increase` *(likely PromptAlways → must use TOCTOU helper)*
- `events.subscribe/poll/unsubscribe` + helper loop
- `permission.check/request/prepare_action/execute_action`
- `external.open_url` *(PromptAlways via TOCTOU)*
- `clipboard.write` / `clipboard.read` *(PromptAlways via TOCTOU)*
- `blob.put_start/put_chunk/put_finish/get_chunk` (chunked transfers)

### Errors (MUST be developer-usable)
Provide SDK error classes that preserve:
- `code` (from `BridgeErrorCode`)
- `message` (safe for display)
- `errorId` (support correlation)
- `retryable`, `retryAfterMs`

Example:
```ts
class BridgeCallError extends Error {
  code: 'UNAUTHORIZED' | 'PERMISSION_DENIED' | ...;
  errorId?: string;
  retryable: boolean;
  retryAfterMs?: number;
}
```

### Rate limit + retry guidance (SHOULD)
- SDK should **not** auto-retry non-idempotent methods unless explicitly opted-in.
- SDK **may** auto-retry `RATE_LIMITED` for idempotent methods if `retryable=true` and `retry_after_ms` present.

### Permission/TOCTOU helper (SHOULD, but strongly recommended)
Because PromptAlways flows are otherwise easy to misuse, standardize:

```ts
client.actions.clipboard.writeText(text, { promptTimeoutMs?: 60_000 })
```

Under the hood:
1) call `permission.prepare_action({ method: "clipboard.write", params: { text } })`
2) poll execution readiness (see note below)
3) call `permission.execute_action({ action_token })`

**Important spec gap to resolve in the full SDK spec:**
- The permission spec defines `PendingActionStatus` but does not define a bridge method to query it.
- The SDK & DX spec should require one of:
  - `permission.action_status({ action_token }) -> { status }`, or
  - `permission.execute_action` returns a typed retriable error like `CONFLICT` or `TIMEOUT` until confirmed.
  
Pick one and make it consistent; otherwise app developers will implement unsafe/busy-loop patterns.

---

## 5) Type Generation (from CDDL schemas)

### Source of truth
- Store CDDL in a canonical directory, e.g.:
  - `protocol/cddl/bridge_v1.cddl`
  - `protocol/cddl/permission_v1.cddl`
  - `protocol/cddl/events_v1.cddl`
- Method registry metadata (tier, caps, timeout, idempotent) must be generated from **one** canonical registry file to avoid drift.

### Generation outputs (MUST)
1. **TypeScript**
   - `MethodName` union
   - `MethodParams` map
   - `MethodResult` map
   - `BridgeErrorCode` union
   - Discriminated unions for result envelopes if exposed
2. **Rust (optional but recommended)**
   - Compile-time checks that Rust handler signatures match registry
   - A JSON export of the registry that SDK can embed for runtime capability UI strings / docs

### Tooling pipeline (actionable)
- Use `cddl` → JSON Schema → TS types:
  - `cddl2jsonschema` (or equivalent)
  - `quicktype` or `json-schema-to-typescript`
- Add a “type tests” step:
  - Generate fixtures from Rust (golden CBOR vectors) and ensure TS decode matches.
- Pin versions of generators to prevent churn; commit generated artifacts or generate in CI with checksum enforcement.

### Doc generation (SHOULD)
- Generate reference docs from the same registry:
  - method name, params schema, result schema, tier, required capabilities, max sizes, idempotent, timeout.

---

## 6) Developer Tooling (CLI, templates, debugging)

### CLI (MUST)
A single entry tool, e.g. `posturbit` or `posturbit-app`, with commands:

- `init` / `create`  
  - scaffolds manifest + UI + permissions examples
- `dev`
  - watches files and triggers reload in shell
  - prints app logs + bridge calls (when enabled)
- `build`
  - produces a `ui/` bundle compatible with `postapp://`
- `package`
  - creates distributable app bundle (and signature placeholder)
- `install --local`
  - installs into local shell apps directory
- `typegen`
  - regenerates TS types from CDDL and validates checksums
- `doctor`
  - validates environment and common failures (missing bootstrap, wrong app_id, oversized assets)

### Templates (MUST)
- Minimal “hello world” + `storage` example
- “Events” example (subscribe + poll loop)
- “Permission + TOCTOU” example (external.open_url, clipboard.write)

### Debugging tools (SHOULD)
- **Bridge inspector** (dev mode only):
  - logs: method, request id, timing, errorId, payload byte sizes (never tokens)
- **Devtools enablement**:
  - shell setting to open WebView devtools for app webviews
- **Audit correlation**:
  - expose `error_id` and document how to provide it in bug reports

### Local dev workflow under CSP constraints (MUST be addressed)
Given `connect-src 'none'`, classic Vite HMR over websockets will not work unless you introduce a dev exception.

Two acceptable paths—spec must pick one:

A) **Reload-based dev (simpler, secure by default)**
- CLI builds to disk on change
- shell file-watches `apps_dir/{app_id}/ui/` and triggers `window.location.reload()`
- no CSP changes, no network

B) **Explicit dev CSP exception (more ergonomic, higher risk)**
- only when shell is in dev mode + app is marked `dev: true`
- `connect-src ws://127.0.0.1:* http://127.0.0.1:*` for that app only
- must be visually indicated and never allowed in production builds

---

## 7) Security Constraints (what MUST NOT be exposed)

The SDK & DX spec should contain a hard “MUST NOT” list and how you enforce it:

1. **Must not expose general-purpose Tauri APIs**
   - Do not re-export `@tauri-apps/api`
   - Do not provide `invoke(commandName, …)`; only allow invoking *bridge methods*.

2. **Must not provide a way to call shell-only methods**
   - SDK should not ship typed wrappers for `shell.*`
   - If a developer passes `"shell.*"` into generic `invoke`, SDK should reject locally before transport.

3. **Must not persist session tokens**
   - No localStorage/IndexedDB storage of `token`/`session_id`
   - No query-string placement, no console logging of token

4. **Must not bypass permission flows**
   - SDK should guide developers into TOCTOU helpers for PromptAlways methods
   - Document that attempting direct calls may fail and is unsupported

5. **Must not weaken sandbox invariants**
   - No recommendation to relax CSP outside dev mode
   - No dynamic remote imports (apps have no network)

6. **Must not leak sensitive backend details**
   - SDK error messages must remain safe; surface `error_id` instead of internals

7. **Must respect platform limits**
   - Enforce 256KB request/response limits client-side
   - Provide blob chunking helpers; never encourage oversized single requests

---

## 8) Test Scenarios (key acceptance criteria)

### SDK unit tests (pure JS/TS)
- **Bootstrap parsing**
  - missing/invalid bootstrap object fails with safe error
  - version mismatch surfaces actionable guidance
- **Envelope correctness**
  - required fields present; `v=1`; `ts` set; UUID id generated
- **CBOR encode/decode**
  - rejects payloads > 256KB before transport
- **Error mapping**
  - maps each `BridgeErrorCode` to stable SDK exception type
  - handles `retry_after_ms` correctly

### Integration tests (in real shell sandbox)
- **Only bridge IPC is used**
  - SDK cannot call other commands (capabilities enforce; SDK also blocks locally)
- **Session binding**
  - token stolen from app A used in app B fails (`UNAUTHORIZED`)
- **Rate limit behavior**
  - burst requests cause `RATE_LIMITED` with `retry_after_ms`; SDK backoff guidance works
- **Replay semantics**
  - duplicate `(session,id)` returns cached response (validate deterministic behavior)
- **Events long-poll**
  - subscribe → poll returns events; timeout returns empty after requested duration
- **Chunked blob transfers**
  - upload >256KB succeeds via chunks; total size limit enforced (10MB)
- **Permission TOCTOU**
  - PromptAlways method cannot be executed without confirmation
  - prepared action expires after window; second execute fails (single-use)
  - SDK helper completes flow without busy-looping or leaking params

### DX acceptance (developer-facing)
- “Create → dev → install → run” path works on Win/macOS/Linux
- Template apps demonstrate storage, events, and a permissioned action
- Debug tooling shows request ids + timings + errorIds, never tokens

---

If you want, I can also provide a concrete **proposed table of contents** for the SDK & DX spec (as the subagent’s starting document), but the above is the minimum actionable guidance that should drive the full spec and keep it aligned with 02/04/06 security requirements.
