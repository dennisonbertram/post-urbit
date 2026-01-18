Below is a DEEP DIVE review of the App Runtime specs, focusing on implementability, edge cases, interfaces between components, security, lifecycle/state machines, and wire-format consistency. Each issue includes: problem, citation(s), and a proposed fix.

---

## BLOCKING (cannot implement without resolution)

### 1) WASM host ABI signatures conflict across documents
**Problem:** Import function signatures don’t match between specs, making it impossible to implement a single runtime ABI.
- `host.get_result`:
  - **api-surface** defines `(param i32 i32 i32) (result i32)` (call_id, result_ptr, result_len).
  - **wasm-sandbox** defines `(param i32 i32) (result i32)` (missing `result_len`).
- `host.log`:
  - **api-surface**: `(level, msg_ptr, msg_len)`.
  - **wasm-sandbox**: `(param i32 i32)` (missing length and level or ambiguous).
- `host.call` naming mismatch: api-surface uses `$host_call`, wasm-sandbox uses `$call` (not fatal by itself, but indicates ABI drift).

**Citations:**
- `spec/04-app-runtime/api-surface.md` → “Host Call Convention” WASM imports
- `spec/04-app-runtime/wasm-sandbox.md` → “Host Imports”

**Proposed fix:**
- Create a single normative “Runtime ABI” section (or separate `abi.md`) that is authoritative.
- Include exact WASM import names, signatures, semantics, and error conventions.
- Add a versioned ABI identifier (e.g., `abi_version: 1`) tied to `dependencies.api_version`.

---

### 2) “Async by default” is underspecified / internally contradictory
**Problem:** The Host API claims “All I/O operations are asynchronous” with callbacks, but the only defined mechanism is `call()` returning a `call_id` plus `get_result()` polling. That forces either busy-waiting or inventing an event loop design; no clear yield/resume semantics are specified. Additionally, “callbacks” are mentioned but not defined at the ABI level (how does the host call into WASM safely?).

**Citations:**
- `spec/04-app-runtime/api-surface.md` → “Async by Default”, “Call Flow”
- `spec/04-app-runtime/interfaces.md` → `HostBridge.registerCallback()` / `invokeCallback()`
- `spec/04-app-runtime/api-surface.md` → `messaging.subscribe` includes `callback_entry`

**Proposed fix:**
Pick and fully specify one async model:
1) **Polling model (simpler but needs clarity):**
   - Define `host.poll()` or reuse WASI `poll_oneoff` integration with a host-managed eventfd.
   - Specify max outstanding calls and blocking behavior.
   - Define that `get_result()` returns `PENDING` vs `READY`.
2) **Callback/re-entrancy model (more complex):**
   - Define a host-to-WASM callback ABI (export signatures, memory passing, reentrancy rules).
   - Specify whether callbacks can interrupt a running invocation (likely “no”; queue until safe point).
3) **Asyncify / stack switching model:**
   - Specify toolchain requirements and runtime mechanism (e.g., Wasmtime async + asyncify).
   - Define that `host.call()` suspends and resumes the same invocation.

Without choosing, implementation is blocked.

---

### 3) Invocation ABI (passing args/return values) is incomplete
**Problem:** `wasm-sandbox.md` mandates exports like:
- `handle(param i32 i32) (result i32)`
But there is no normative definition of:
- what the two params represent (ptr/len? ptr/call_id?),
- how results are returned (pointer only? where is length?),
- where CBOR bytes live in linear memory and who allocates/frees,
- how errors are returned from `handle`.

**Citations:**
- `spec/04-app-runtime/wasm-sandbox.md` → “Required Exports”
- `spec/04-app-runtime/interfaces.md` → `AppRuntimeService.invoke()` expects `args: Bytes` and returns `InvocationResult.result: Bytes`

**Proposed fix:**
Define a stable “guest ABI” for invocation, e.g.:
- Host writes input bytes into guest memory via `alloc(len)`; calls `handle(ptr, len)`.
- `handle` returns a **packed pair** `(ptr, len)` via:
  - two return values (WASM multi-value), or
  - returning ptr and exposing `get_last_result_len()`, or
  - writing into a host-provided output buffer.
- Define `get_error()` semantics similarly (ptr+len).
- Explicitly define ownership: guest allocates; host copies then calls `dealloc(ptr,len)` (or guest frees itself).

---

### 4) Capability enforcement can be bypassed via enabled WASI calls
**Problem:** The sandbox enables WASI `random_get` and `clock_time_get` while also defining capability-gated host APIs `system.get_random` (`system:random`) and `system.get_time` (`system:time`). If WASI provides these directly, apps can bypass permissions and audit logging.

**Citations:**
- `spec/04-app-runtime/wasm-sandbox.md` → “Enabled WASI Capabilities”
- `spec/04-app-runtime/api-surface.md` → “System API” methods + capabilities
- `spec/04-app-runtime/capability-system.md` → “System Capabilities”

**Proposed fix:**
- Disable raw WASI `random_get` / wall-clock, or wrap them so they route through the host capability system.
- If monotonic time is allowed without permission, explicitly declare it and document privacy implications.
- Ensure audit logs capture these accesses if they remain enabled.

---

### 5) Package signature does not clearly secure the actual code/assets
**Problem:** Manifest signature verification is specified, but it signs “canonical manifest (without signature field)”. That does not bind the signature to the WASM binary or other package files unless the manifest includes hashes of them (it currently doesn’t). The “content hash” exists, but there’s no normative requirement that the signature covers it.

**Citations:**
- `spec/04-app-runtime/manifest-schema.md` → “Signature Verification”
- `spec/04-app-runtime/manifest-schema.md` → “Content Addressing” + “Package Verification”

**Proposed fix:**
- Bind signature to package contents by one of:
  1) Include a `files: { path: sha256 }` map in the manifest and sign it, or
  2) Sign `package_hash` explicitly (signature over hash + manifest fields).
- Make this step mandatory during install/update verification.

---

### 6) Two storage models exist (virtual FS + key/value Host Storage API) with unclear mapping
**Problem:** The runtime offers:
- Virtual filesystem `/data`, `/cache`, `/tmp` via WASI virtualization, **and**
- A Host Storage API (`storage.get/set/list`) with quotas/versioning.
But there’s no clear statement whether both are supported, which is preferred, how quotas are unified, and which capability gates filesystem access. This blocks implementation choices (backing store layout, quotas, version semantics, concurrency).

**Citations:**
- `spec/04-app-runtime/wasm-sandbox.md` → “Filesystem Virtualization”
- `spec/04-app-runtime/api-surface.md` → “Storage API”
- `spec/04-app-runtime/capability-system.md` → storage capabilities

**Proposed fix:**
- Decide and document one of:
  - **Option A:** Deprecate WASI FS for apps; expose only Host Storage + asset read-only FS.
  - **Option B:** Keep both, but define:
    - `/data/*` operations require `storage:app`
    - `/cache/*` allowed without `storage:app` but quota-limited (or still gated)
    - quotas are unified across KV + FS or explicitly separate
    - versioning applies only to KV (document clearly)

---

### 7) Method-to-capability mapping is inconsistent and incomplete
**Problem:** Capability enforcement depends on a correct mapping, but the mapping example uses method names not present in the Host API spec (e.g., `storage.read` vs `storage.get`, `system.getTime` vs `system.get_time`). If implementers copy this, capability checks will be wrong.

**Citations:**
- `spec/04-app-runtime/capability-system.md` → “Method-to-Capability Mapping”
- `spec/04-app-runtime/api-surface.md` → method names tables (e.g., `storage.get`, `system.get_time`)

**Proposed fix:**
- Make a single authoritative mapping table in the Host API spec itself, using **exact** method strings.
- Require that the host rejects unknown methods with `NOT_FOUND` or `INVALID_ARGUMENT` and logs the attempt.

---

## HIGH (significant gaps or security/operational risks)

### 8) Result/error wire format is inconsistent (negative integers vs CBOR `ErrorResponse`)
**Problem:** `get_result` “returns negative for error” but elsewhere errors are defined as typed codes and/or CBOR structures (`ErrorResponse`). It’s unclear whether:
- the error is returned via negative length only,
- or returned as CBOR-encoded `ErrorResponse`,
- or both (and if so, how to distinguish).

**Citations:**
- `spec/04-app-runtime/api-surface.md` → `get_result` description; “Error Response Format”
- `spec/04-app-runtime/interfaces.md` → `HostBridge.handleCall(): Result<Bytes, HostError>`

**Proposed fix:**
- Define a single envelope for all host call results, e.g. CBOR:
  ```cbor
  { ok: true, value: <T> } OR { ok: false, error: { code, message, details } }
  ```
- Make `get_result()` return:
  - `0` = pending
  - `>0` = bytes written
  - `<0` = transport-level error only (e.g., buffer too small), with a defined enum
- Or remove negative-length errors and always return CBOR.

---

### 9) Subscription + background delivery lifecycle is not defined
**Problem:** Specs allow `messaging.subscribe` with callbacks and define background triggers (message/sync/interval), but do not specify:
- whether messages can arrive when the app instance is unloaded,
- whether the runtime auto-loads apps for callbacks,
- how to avoid DoS by message-triggered auto-wake,
- whether subscriptions persist across restarts and updates,
- what happens to subscriptions on uninstall/disable.

**Citations:**
- `spec/04-app-runtime/api-surface.md` → `messaging.subscribe`
- `spec/04-app-runtime/interfaces.md` → `HostBridge.getSubscriptions()` / `cancelSubscription()`, `AppRuntimeService.scheduleTask()`
- `spec/04-app-runtime/manifest-schema.md` → `background.triggers`
- `spec/04-app-runtime/wasm-sandbox.md` → instance timeout/unload rules

**Proposed fix:**
- Define a lifecycle/state machine for:
  - subscription persistence (in DB vs memory),
  - wake policy (auto-load allowed only with `system:background` + per-trigger quotas),
  - callback dispatch semantics (queued, at-most-once vs at-least-once),
  - cleanup rules (on disable/uninstall/update: cancel or migrate).

---

### 10) Messaging capability model is internally inconsistent (`receive` vs `subscribe`)
**Problem:** Capability list includes `messaging:receive`, but Host API methods table uses `messaging.subscribe` gated by `messaging:subscribe`. There is no `messaging.receive` method, and it’s unclear whether receiving requires `receive`, `subscribe`, or both.

**Citations:**
- `spec/04-app-runtime/capability-system.md` → Messaging Capabilities
- `spec/04-app-runtime/api-surface.md` → Messaging API methods/capabilities
- `spec/04-app-runtime/capability-system.md` → mapping includes `messaging.receive`

**Proposed fix:**
- Define receiving model explicitly:
  - Option A: `messaging:subscribe` implies receive; remove `messaging:receive`.
  - Option B: require both: `messaging.subscribe` requires `messaging:receive` AND `messaging:subscribe`.
- Update all tables and mapping accordingly.

---

### 11) Deterministic execution claims conflict with crypto/security requirements
**Problem:** Sandbox says `random_get` uses “deterministic PRNG seeded per invocation” (good for replay) but Host API `system.get_random` is described as “cryptographic randomness”. Deterministic PRNG is not cryptographically random and could break key generation, nonces, etc.

**Citations:**
- `spec/04-app-runtime/wasm-sandbox.md` → “Non-Deterministic Operations” (`random_get`)
- `spec/04-app-runtime/api-surface.md` → `system.get_random`

**Proposed fix:**
- Split into two APIs:
  - `system.get_random` = CSPRNG, non-deterministic, capability gated, audited.
  - `system.get_deterministic_random(seed?)` = deterministic replay helper, explicitly **not** for crypto.
- Update determinism section: reproducibility applies only when deterministic APIs are used.

---

### 12) Inter-app invocation semantics are unspecified (reentrancy, auth, limits)
**Problem:** `app.invoke` allows one app to invoke another, but the spec doesn’t define:
- execution context propagation (caller identity?),
- whether callee can see caller app id,
- whether invocation is synchronous/async,
- recursion limits / cycle handling (A→B→A),
- capability interaction (callee should not inherit caller’s capabilities),
- resource accounting (fuel attribution: caller or callee?).

**Citations:**
- `spec/04-app-runtime/api-surface.md` → “Inter-App API”
- `spec/04-app-runtime/overview.md` → isolation claims

**Proposed fix:**
- Define an explicit inter-app RPC model:
  - calls run under **callee’s** permissions only
  - caller identity is provided as metadata (optional, capability-gated)
  - set max call depth and total time/fuel budget
  - require explicit method registration in manifest (`exports` list) to reduce attack surface

---

### 13) Revocation behavior is underspecified (and `requestCapability()` may reprompt unexpectedly)
**Problem:** Capability workflow says user can revoke at any time, but `handleHostCall()` example calls `requestCapability()` even for required capabilities, potentially reprompting at runtime in surprising ways.

**Citations:**
- `spec/04-app-runtime/capability-system.md` → “Host API Integration” code example
- `spec/04-app-runtime/capability-system.md` → “Revocable”

**Proposed fix:**
- Separate checks:
  - Required capabilities: must be `GRANTED` at install; if later revoked, calls fail with `PERMISSION_DENIED` **without reprompt** (or reprompt only via explicit UI action).
  - Optional capabilities: may prompt on first use.
- Specify behavior when revoked mid-invocation (fail current call? cancel pending?).

---

## MEDIUM (implementation hazards, missing edge cases)

### 14) Host call lifecycle missing: outstanding calls, buffer-too-small, and cleanup
**Problem:** No limits or cleanup rules for:
- max outstanding `call_id`s per invocation/app,
- what happens if app never calls `get_result`,
- how long results are retained,
- how to handle result larger than buffer (partial reads? retry with bigger buffer?).

**Citations:**
- `spec/04-app-runtime/api-surface.md` → Host Call Convention
- `spec/04-app-runtime/interfaces.md` → `HostBridge.handleCall()` (no call_id concept here)

**Proposed fix:**
- Define:
  - `MAX_OUTSTANDING_CALLS` per invocation/app
  - retention policy (e.g., results expire after N seconds or on invocation end)
  - `BUFFER_TOO_SMALL` error and a way to query required size (`host.get_result_len(call_id)`), or allow chunked reads with an offset.

---

### 15) Transactionality/rollback claims are not specified
**Problem:** Sandbox claims “in-progress operations are rolled back” after traps, and “storage writes order preserved within transaction,” but there is no transaction model defined for host calls, especially across async boundaries.

**Citations:**
- `spec/04-app-runtime/wasm-sandbox.md` → “Recovery”, “Determinism Guarantees”
- `spec/04-app-runtime/api-surface.md` → async host call model

**Proposed fix:**
- Define a transaction scope explicitly:
  - Option A: each host call is atomic; no multi-call transactions.
  - Option B: invocation has an implicit transaction for storage ops only; commit on successful return; abort on trap/timeout/cancel.
- Clarify messaging side effects (generally can’t be rolled back once sent).

---

### 16) Background scheduling interface gap (apps can’t schedule their own tasks via Host API)
**Problem:** `AppRuntimeService.scheduleTask()` exists as a host-side interface, but there is no Host API method for WASM apps to request background scheduling (despite `system:background` capability existing).

**Citations:**
- `spec/04-app-runtime/interfaces.md` → `AppRuntimeService.scheduleTask`
- `spec/04-app-runtime/capability-system.md` → `system:background`

**Proposed fix:**
- Add Host API methods:
  - `system.background.schedule` (capability: `system:background`)
  - `system.background.cancel`
  - `system.background.list`
- Or explicitly state background triggers are **manifest-only** and cannot be requested dynamically.

---

### 17) Manifest storage quota vs quota-as-capability is inconsistent
**Problem:** Capability system defines `storage:quota:{size}` as a capability, while manifest defines `storage.quota: "50mb"`. It’s unclear which is authoritative, how user consent applies, and how upgrades request more quota.

**Citations:**
- `spec/04-app-runtime/capability-system.md` → “Quota Capabilities”
- `spec/04-app-runtime/manifest-schema.md` → `storage.quota`

**Proposed fix:**
- Make quota request a manifest field only, and user grants it as part of install/update UI; store as a permission record.
- Or make quota purely capability-based and remove `storage.quota` from manifest (or treat it as a requested default that becomes a `storage:quota:*` grant).

---

### 18) Parameterized capabilities and wildcards need matching rules (and regex risks)
**Problem:** Capabilities support `*` and regex-based scopes, but matching rules are not defined (glob? prefix? exact?), and regex introduces ReDoS risk if untrusted patterns are evaluated.

**Citations:**
- `spec/04-app-runtime/capability-system.md` → “Capability Format”, “PermissionScope: pattern; regex”
- `spec/04-app-runtime/api-surface.md` → e.g., `storage.shared.get` uses `storage:shared:*`

**Proposed fix:**
- Define deterministic matching:
  - Prefer glob/prefix matching with a safe engine.
  - If regex is kept, require RE2-style engine or validate regex complexity.
- For shared storage, prefer explicit namespace capability: `storage:shared:photos` rather than `*` for most apps.

---

### 19) Naming conventions drift (snake_case vs camelCase) causes integration bugs
**Problem:** Method names and fields vary:
- Host API methods use `snake_case` (`system.get_time`).
- Capability map uses camelCase (`system.getTime`).
- Manifest uses `snake_case` fields; TS interfaces use `camelCase`.

This isn’t inherently wrong, but without explicit mapping rules it will cause subtle mismatches.

**Citations:**
- `spec/04-app-runtime/api-surface.md` method names
- `spec/04-app-runtime/capability-system.md` mapping table
- `spec/04-app-runtime/manifest-schema.md` vs `spec/04-app-runtime/interfaces.md`

**Proposed fix:**
- Add a “Naming & Mapping” section:
  - manifest JSON is snake_case
  - internal TS types are camelCase
  - host method strings are **exactly** as documented (no aliases)
- Provide a conformance test list of method strings.

---

### 20) Limits and validation rules are inconsistently specified across APIs
**Problem:** Some endpoints define constraints (key max 256 bytes, value max 1MB), many do not:
- `message_type` validation says “INVALID_MESSAGE_TYPE” but gives no pattern/length limits.
- `cursor` types are strings but no encoding/format is defined.
- group sizes, subscription counts, etc: partially specified.

**Citations:**
- `spec/04-app-runtime/api-surface.md` across Storage/Messaging sections

**Proposed fix:**
- Add a global “Common Limits” section:
  - max string length per field type
  - max CBOR payload sizes per method
  - cursor format (opaque base64url string) and stability rules
  - max subscriptions per app and per filter type

---

## LOW (suggestions / polish)

### 21) Clarify uninstall/update cleanup sequencing and invariants
**Problem:** Uninstall options include `keepData`, but the spec doesn’t describe ordering: cancel tasks/subscriptions first? stop instance? revoke permissions? remove shared namespace references?

**Citations:**
- `spec/04-app-runtime/interfaces.md` → `uninstall()`, `UninstallOptions`

**Proposed fix:**
- Add a short uninstall state machine:
  1) stop invocations / cancel pending host calls
  2) cancel tasks/subscriptions
  3) revoke capabilities
  4) delete package + optionally delete `/data`
  5) emit events

---

### 22) Add explicit conformance tests / reference vectors for CBOR encoding
**Problem:** “CBOR-encoded structures” are referenced, but no canonical CBOR profile or test vectors are provided; interoperability across SDKs will suffer.

**Citations:**
- `spec/04-app-runtime/api-surface.md` → “Structured Data”

**Proposed fix:**
- Specify:
  - CBOR flavor (RFC 8949), required tags (if any), whether maps must be definite-length, etc.
  - Provide golden test vectors per method for SDK authors.

---

# Summary of the most urgent spec actions
If you want a minimal path to an implementable v1, the top fixes are:
1) Publish a single authoritative ABI (imports/exports + memory/result conventions).
2) Choose and specify the async model (polling vs callback vs asyncify).
3) Prevent WASI bypass of capability-gated APIs (time/random).
4) Bind signatures to package contents, not just manifest text.
5) Resolve storage model duplication (KV vs FS) and enforce quotas/capabilities consistently.
6) Make method strings and capability mapping authoritative and consistent.

If you want, I can propose a concrete “ABI v1” spec (imports/exports, result envelopes, polling/callback model) that fits Wasmtime constraints and integrates cleanly with your `HostBridge` / `AppRuntimeService` interfaces.
