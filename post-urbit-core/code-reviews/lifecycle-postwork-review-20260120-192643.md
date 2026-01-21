## Score: **6 / 10**

Strong outline and good alignment with the multi-webview/LRU/atomic-install intent, but it currently has a few **spec-breaking cross-document conflicts** (notably around **Rust→app events and eviction handshake**) and some **session-expiry semantics** that don’t compose with the SDK as written.

---

## 1) State machine gaps / invalid transitions

### 1.1 Missing “in-progress” runtime/install substates (race-prone)
You have `InstallState::{Installed, Disabled, Corrupted}` but no explicit transient states like:
- `Installing / Updating / Uninstalling / Repairing`
- `Launching / Closing` (or “Draining”)

Without them, concurrent commands become ambiguous:
- `shell_launch_app()` during `install()` or `update()`
- `shell_update_app()` while app is `Hot` and receiving bridge calls
- resource-manager `Evict` racing `Close(Uninstall|Upgrade)` (see §5)

**Fix:** add transient states or a per-app operation lock + explicit “operation in progress” error codes.

### 1.2 “Cold → Hot: launch() / show()” is semantically wrong
In the diagram: `Cold --> Hot : launch() / show()`.  
But “show” should only apply to an existing webview (`Warm/Hot`). If you allow `show()` to implicitly create a webview, it must be specified as an alias of `launch()` with identical semantics (session creation, bootstrap injection, etc.).

**Fix:** constrain:
- `launch`: `Cold→Hot`
- `show`: `Warm→Hot` (and `Hot→Hot` no-op)
- define behavior when `show` called in `Cold`: either error or call `launch`.

### 1.3 Single-instance vs multi-instance not specified
Across the repository, the webview label convention is `"app-{app_id}"` (02,04), which implies **one instance per app**. Shell UI (“WindowManager”) suggests potentially multiple windows/surfaces.

**Fix:** explicitly decide:
- **Single-instance per app** (most consistent with `label = app-{app_id}`), OR
- **Multi-instance** by introducing instance IDs: `app-{app_id}-{instance_id}` and updating session binding + permission scoping accordingly.

---

## 2) Session lifecycle edge cases (major)

### 2.1 Session expiry + SDK “reload on UNAUTHORIZED” can dead-loop
Spec 08 says: token expiry → `UNAUTHORIZED` → SDK does `window.location.reload()` (also in spec 07).  
But **reload does not guarantee a new bootstrap/session** unless you recreate the webview or have a defined “bootstrap refresh” mechanism.

As written, the likely outcome is:
1) token expires  
2) any call returns `UNAUTHORIZED`  
3) SDK reloads  
4) app reloads with the **same injected bootstrap/token** (or no bootstrap)  
5) repeat → permanent failure

**Fix options (pick one and align across 04/07/08):**
- **A. Make sessions effectively webview-lifetime**: remove/relax TTL (or set extremely long) and rely on explicit destroy on `Close/Evict/Uninstall`.  
- **B. Define a session refresh mechanism** compatible with “no auto-refresh”:
  - backend signals shell to recreate the webview upon session expiry, **or**
  - define a *one-way* “bootstrap rotation” injection (trusted JS bridge) and SDK support for replacing in-memory token (but this contradicts current SDK invariant).

### 2.2 In-flight requests during close/destroy are unspecified
When `destroy_session()` runs, you clear replay cache and expire session grants. What happens to:
- in-flight `postbridge_invoke` requests already authorized?
- long-poll `events.poll` calls?

**Fix:** define a closing policy:
- mark session as `Closing` (reject new requests with `UNAUTHORIZED` or `CONFLICT`)
- allow in-flight to complete for up to `N ms`, then abort.

### 2.3 Session table persistence conflicts with “in-memory only token”
08 includes a persistent `app_sessions` table. That’s fine for audit/diagnostics, but ensure it never stores tokens/nonces. Right now it stores `capabilities_json`, timestamps, labels—OK—but the spec should explicitly say **tokens/nonces are not persisted** to match SDK/security invariants.

---

## 3) Missing / inconsistent error handling

### 3.1 `Result<..., String>` is too under-specified for shell commands
Other specs define structured error taxonomies (`BridgeErrorCode`, `LifecycleErrorCode`). 08 ends with `LifecycleErrorCode` but the command signatures still use `Result<..., String>`.

**Fix:** make shell commands return structured errors:
- `Result<T, LifecycleError>` where `LifecycleError { code: LifecycleErrorCode, message, details? }`
and ensure mapping to UI-friendly messages.

### 3.2 Atomic rename is not always sufficient
`std::fs::rename(staging_dir, final_dir)` is atomic **only on same filesystem** and can fail on Windows when files are in use.

**Fix:** specify:
- staging dir must be on same volume as final
- Windows-specific retry/backoff and “ensure no open handles”
- cleanup guarantees on failure (delete staging dir, emit failed stage)

---

## 4) Cross-spec conflicts (bridge protocol, permissions, SDK)

### 4.1 **Rust→App events conflict with Bridge spec**
Spec 04 explicitly states: backend-to-app push happens via **app-initiated long-polling** (`events.poll`).  
Spec 08 uses direct events like:
- `app://lifecycle/created`
- `app://resource/evicting`
- `app://session/expiring`

But apps are sandboxed and “have no Tauri APIs”; they can’t `listen()` to Tauri events. So these `app://...` events have no defined delivery mechanism consistent with 04/07.

**Fix:** unify one way:
- **Option A (preferred with 04):** represent lifecycle/resource/permission notifications as topics in the `events.*` subsystem:
  - `events.subscribe(topic="lifecycle")` etc.
- **Option B:** define a minimal injected JS event bridge (not Tauri API) and describe exactly how Rust delivers events into the webview (e.g., `webview.eval` with an internal dispatcher). If you do this, reconcile with the “only IPC is postbridge_invoke” claim in 04.

### 4.2 Eviction handshake “prepare_for_unload” is not coherent end-to-end
08 close flow says: emit `app://resource/evicting`, allow app `prepare_for_unload` response.

But:
- 04 method registry does **not** include `resource.prepare_for_unload`
- 03 shows an SDK method `prepareForUnload()` that calls `bridge.call('resource.prepare_for_unload')`, but also says “App implements this handler” (internally contradictory)

**Fix:** pick a concrete mechanism:
- Add a bridge method `resource.prepare_for_unload` (app→Rust) and specify that on receiving `resource.evicting` event, app must call it within deadline.
- Or make eviction state retrieval **shell-driven** via `webview.eval` calling a well-known app hook (but then it’s not “bridge only”, and you need strict size/time limits and error behavior).

### 4.3 Permissions: uninstall revocation vs pending actions
08 uninstall revokes permissions + invalidates sessions, but doesn’t mention clearing:
- `PendingAction` rows (06)
- outstanding permission prompts / rate limiter state

**Fix:** on uninstall, explicitly:
- revoke all permission records
- delete all pending actions for that app
- cancel queued prompts and deny/expire them deterministically.

---

## 5) Eviction/cleanup race conditions (important)

### 5.1 Evict vs Upgrade/Uninstall/Close ordering needs a single winner
ResourceManager may trigger `Evict` while shell triggers `Close(Upgrade)` or `Close(Uninstall)`. Without serialization you can get:
- double-destroy of webview
- destroying session twice (replay cache cleared while still used)
- persisting state for an app that is about to be uninstalled (leaks metadata)

**Fix:** define precedence + locking:
- Introduce a per-app lifecycle mutex.
- Define close reason precedence, e.g. `Uninstall > Upgrade > UserClose > Eviction`.
- Ensure eviction persistence is skipped or scoped when uninstalling.

### 5.2 Data deletion while webview still running
Uninstall deletes app dirs, clears web storage, deletes blobs. If webview teardown isn’t complete (or OS still flushing IndexedDB), you can get partial deletion or recreation.

**Fix:** specify uninstall as:
1) transition to `Closing(Uninstall)`
2) destroy webview and confirm termination
3) then delete data, with retries and verification

### 5.3 Session invalidation vs `events.poll` / long-poll
If using long-poll (04), a session can be stuck in `events.poll` when you evict/close. Define whether:
- poll returns immediately with `UNAUTHORIZED`
- poll is canceled server-side
- client handles it without spinning

---

## Additional smaller correctness issues

- **Type mismatch bug in 08**: `create_session()` signature returns `Result<AppSession, SessionError>` but `Ok((session, token))` returns a tuple. Needs correction (and should align with bootstrap injection pipeline).
- `destroy_session()` calls `permission_store.expire_session_grants(&session.session_id)?;` but spec 06 makes session grants keyed by `session_id` *and* `(app_id, capability)` uniqueness in SQLite. Ensure the schema supports multiple session grants over time without violating `UNIQUE(app_id, capability)`—currently it does not (06 schema uses `UNIQUE(app_id, capability)`), which conflicts with having session-scoped grants recorded as separate rows.

---

## Recommended targeted revisions (highest ROI)

1) **Resolve Rust→app event delivery** (08 vs 04/07): either standardize on `events.*` long-poll topics or define an injected event dispatcher mechanism.
2) **Resolve session-expiry behavior** to avoid reload loops; either remove TTL or define shell-driven webview recreation.
3) **Make eviction handshake concrete** (`resource.prepare_for_unload` or equivalent) and align registry/SDK/spec.
4) **Add operation serialization** (transient states or per-app lock) to prevent close/evict/upgrade/uninstall races.
5) **Unify event names** with Shell spec (`shell://apps/installed_changed` vs `shell://apps/installed`, etc.) to avoid implementation drift.

If you want, I can propose a corrected, cross-spec-consistent lifecycle+session state machine (including transient states and precedence rules) that directly maps onto the existing bridge/permission primitives.
