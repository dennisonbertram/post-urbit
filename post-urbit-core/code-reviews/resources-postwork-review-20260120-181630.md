## Rating: **6/10**

Well-structured and comprehensive (device-aware defaults, hysteresis, eviction reasons, metrics, and a graceful eviction handshake). The biggest issues are (a) **LRU candidate scoring is wrong as written** and can select non-evictable candidates, (b) several thresholds/budgets are **not clearly justified or internally consistent**, (c) the **pressure/eviction handshake direction is ambiguous**, and (d) **platform memory accounting is oversimplified** (and Windows sample leaks handles).

---

## 1) Device class thresholds — are they well-justified?

### What’s good
- Classing by *physical RAM* is a reasonable first-order heuristic and easy to implement.
- Defaults for hot/warm webviews align with your earlier multi-webview guidance.

### Gaps / concerns
- **Buckets are coarse and a bit arbitrary**:
  - `Constrained: <= 8GB`, `Standard: 9..=24GB`, `Performance: >= 32GB` leaves common real-world configs (12GB, 24GB, 28GB, 30GB) in awkward bins.
  - 24GB is increasingly common; treating it as “Standard” may be fine, but it should be explicit why.
- **Budgets don’t scale with RAM**, especially shell budgets:
  - Shell target/warn/critical are constant across all classes (200/250/350MB). That may be fine if the shell is truly stable, but it’s not “device aware”.
- **Inconsistency between per-app caps and aggregate budgets**:
  - Standard: `max_hot=3`, `max_warm=4` (7 webviews total possible) and `per_app_hard_cap=500MB` implies worst-case 3.5GB just for apps, but `total_app_memory_budget=3GB`.
  - Constrained: 4 total webviews * 500MB = 2GB possible vs 1.5GB aggregate budget.
  - This is okay *if* aggregate budget is only a soft guideline; but the spec implies it’s a “budget” without defining enforcement.

### Improvements
- **Define budgets as percentages of RAM**, with floors/ceilings. Example:
  - `total_app_memory_budget = clamp(0.20 * RAM, min=1.0GB, max=6.0GB)`
  - `shell_memory_critical = clamp(0.05 * RAM, min=350MB, max=800MB)` (if you want device-aware)
- **Add a “Mid” class or adjust ranges** (e.g., 0–8, 9–16, 17–32, 33+), or make it continuous tuning rather than discrete.
- Use **available memory / memory pressure** as primary, and physical RAM as initial defaults only.
- Explicitly state whether `total_app_memory_budget_bytes` is **enforced** (and how), or is **observability-only**.

---

## 2) Is the LRU eviction algorithm correctly specified?

### High-level policy is good
- Prioritized triggers (count → time → pressure), plus hysteresis, is sensible.
- “Never evict focused or pinned” is the right UX default.
- Grace period + persisted state is a strong design.

### The provided scoring implementation has correctness bugs
1) **LRU ordering is reversed**
- You compute: `lru_score = last_active.elapsed().as_nanos()`
- Older last_active ⇒ **larger** elapsed.
- But you sort **ascending** and “lower = evict first”.
- Result: you evict the **most recently active** candidate first (wrong).

2) **Creation time tiebreaker is reversed**
- Same issue: `created_at.elapsed()` larger for older, but ascending would prefer newer.

3) **Non-evictable candidates can be selected**
- `eviction_score()` returns `None` for focused/pinned.
- Then sort compares `Option<...>` directly. In Rust ordering, `None < Some`.
- That means **focused/pinned candidates sort to the front**, and `first()` can return them.

4) **Memory-aware component is semantically confusing**
- The `memory_score = u64::MAX - rss` trick technically produces “bigger rss sorts earlier” under ascending, but it’s non-obvious and fragile (and combined with the reversed LRU, selection will still be wrong).

### Improvements (specific)
- Make the score values monotonic with your intended ordering, and **exclude non-evictables before sorting**.

Example approach:
```rust
// Pseudocode sketch
let mut candidates = apps
  .filter(|a| !a.is_focused && !a.is_pinned)
  .map(|a| {
     let state_priority = if a.state=="warm" { 1 } else { 2 };
     let lru = u128::MAX - a.last_active.elapsed().as_nanos(); // invert
     let mem = if under_pressure { u64::MAX - a.estimated_rss_bytes } else { 0 };
     let created = u128::MAX - a.created_at.elapsed().as_nanos();
     (state_priority, lru, mem, created, a.app_id)
  })
  .collect();

candidates.sort(); // ascending
return candidates.first().map(|x| x.app_id.clone());
```

- Split timestamps: `last_visible` vs `last_interacted`. Hidden apps can have misleading “activity”.
- Define what happens when:
  - **all apps are pinned** (you need an escape hatch policy)
  - **focused app causes critical pressure** (see section 6)

---

## 3) Are bridge rate limits appropriate?

### Likely OK as a starting point, but missing key scoping
- `50 rps sustained / 200 burst / 16 concurrent / 256KB payload` can be fine **per-app** for typical UI→shell interactions.
- But the spec does not clearly state whether limits are:
  - per-app webview
  - per-session
  - global across all apps
  - per-method (important)

### Risks
- **N apps × 50 rps** can overwhelm the shell if N grows (even if hot/warm is capped, cold apps could still spam if not actually destroyed).
- 256KB payload × 50 rps is potentially **12.8MB/s per app** of IPC traffic if abused.
- Concurrency 16 can amplify “expensive handler” DOS if bridge methods do any disk or crypto.

### Improvements
- Explicitly define **two-level limits**:
  1) per-app token bucket
  2) global shell token bucket (to preserve responsiveness)
- Add **per-method caps** (e.g., `resource.prepare_for_unload` max 64KB; other calls default 32–64KB).
- Add a **CPU time budget / timeout** for bridge handlers (fail fast under load).
- Consider **separate priority lanes**: lifecycle/resource-control traffic should not be starved by app spam.

---

## 4) Is the pressure signaling protocol complete?

### Good elements
- Levels: normal/constrained/critical.
- Includes memory/cpu/storage signals and recommended actions.
- Has a pre-eviction event with deadline and reason.

### Major ambiguity: who calls `prepare_for_unload` and how?
Your “Graceful Eviction Handshake” diagram implies:

- Shell emits `app://resource/evicting`
- App returns `resource.prepare_for_unload` response

But the TS SDK implementation of `prepareForUnload()` is **an app-side function** that calls `window.__postUrbitPrepareForUnload`. That’s not a shell→app RPC; it’s just the app calling itself.

So the protocol direction is unclear:
- Does the shell *invoke a bridge method on the app*?
- Or does the app *push a message back* upon receiving `evicting`?

### Improvements
- Make the eviction handshake explicit and testable:
  - Add `evicting` event fields: `sequence`, `issued_at`, `deadline_at`, `request_id`.
  - Define **required app behavior**: app must call `bridge.call('resource.submit_unload_state', {request_id, blob})` before deadline, or do nothing.
  - Alternatively define a shell-driven call: `shell -> app: bridge.request('resource.prepare_for_unload')` with timeout.
- Add **versioning** to payloads (`protocol_version`) to support evolution.
- Define **event delivery guarantees**:
  - best-effort vs guaranteed
  - throttling behavior (especially pressure events)
  - whether events are queued for warm vs cold
- Add explicit semantics for **Low-Resource Mode** signaling to apps (it changes eviction behavior drastically).

---

## 5) Are platform-specific memory reading implementations correct?

### Windows
- The per-process RSS function is roughly fine, but **leaks handles** (`OpenProcess` handle is never closed).
- Permissions may fail in some contexts; should handle errors explicitly.
- Bigger issue: WebView2 memory isn’t one PID. You must **attribute multiple processes** (browser/renderer/gpu/utilities) to each webview/profile.

**Improvements**
- Close handles (`CloseHandle(handle)`).
- Specify how you enumerate and attribute WebView2 processes:
  - Track WebView2 `BrowserProcessId` (CoreWebView2 has `BrowserProcessId`).
  - Enumerate child processes (Toolhelp snapshot) and/or use ETW/WebView2 process collection guidance.
  - Decide whether you measure **per-app** or only **global WebView2 footprint**.

### macOS
- `ps -o rss` works but is a **coarse approximation** and may not map to WKWebView’s actual web content processes (multi-process architecture).
- Shell/app association is hard; “per-app” RSS will be unreliable without deeper integration.

**Improvements**
- Prefer native APIs (`task_info`, `proc_pidinfo`) over shelling out to `ps`.
- Consider relying primarily on **system pressure** on macOS, and treating per-app memory as “unknown/estimated”.

### Linux
- `/proc/[pid]/statm` parsing is fine for RSS, but again: **mapping pid ↔ webview** is the hard part in WebKitGTK multi-process mode.
- You need a strategy for collecting the web process PID(s) belonging to a given app.

**Improvements**
- Define process correlation strategy (WebKit has web process models; you may need to instrument via WebKitGTK signals or run each app in a distinct data dir / process group if possible).
- If correlation is not feasible, state clearly that Linux uses **global pressure + count/time eviction**, not per-app hard caps.

---

## 6) Missing edge cases / failure modes

### Critical missing behaviors
1) **Focused app exceeds hard cap during critical system pressure**
- Spec says focused app is not evicted; that can lead to OS OOM kill / shell death.
- You need a “last resort” rule: if system is critical and only focused app remains, either:
  - prompt user and force reload anyway, or
  - enter “freeze/suspend focused app” if supported, or
  - shed resources (disable GPU acceleration, reduce rendering) if possible.

2) **All apps pinned**
- “Never evict pinned” can deadlock eviction. You need a policy:
  - pinned are “evict-last”, not “never”, under *critical* pressure; OR
  - limit number of pinned apps; OR
  - allow pinned to be demoted hot→warm but not warm→cold.

3) **Eviction handshake can hang the resource manager**
- Ensure eviction waits are non-blocking and timeboxed; app can be hung.
- Ensure you handle “webview already crashed” during handshake.

4) **Thrash detection is not integrated**
- You detect thrash but don’t specify what changes (cooldown, pin temporarily, increase warm timeout, etc.).

5) **Total budget is not used as a trigger**
- You define `total_app_memory_budget_bytes` but eviction triggers are based on count/time/system pressure. Add:
  - if `total_app_rss > total_budget` then evict warm LRU even if system pressure isn’t high.

6) **Storage quota enforcement is platform-dependent**
- IndexedDB/localStorage quotas are controlled by the embedded engine; enforcing “256MB per app” may require per-origin profile isolation and/or custom storage partitioning. The spec should state feasibility per platform and fallback behavior.

7) **Suspend/resume semantics**
- You mention WebView2 `TrySuspend()` but do not specify:
  - when it’s invoked
  - whether “Warm” implies suspended
  - how you handle resume latency and failure.

---

## Summary of top actionable changes
1) **Fix LRU scoring** (reverse elapsed ordering, exclude non-evictables before sort).
2) **Clarify and formalize the eviction handshake** (direction, request_id, submit method, delivery guarantees).
3) Add **global + per-app bridge rate limits**, and reduce default payload caps per-method.
4) Make budgets **internally consistent** (aggregate vs per-app vs max hot/warm).
5) Replace/augment platform memory readings with **correct process attribution strategies**, and explicitly document where only global pressure is reliable.
6) Add “last resort” policies for **critical pressure** (focused app, all pinned, inability to measure memory).

If you want, I can propose a revised “EvictionCandidate scoring + selection” spec block and a concrete “evicting → submit_unload_state” bridge message flow that matches your isolation model.
