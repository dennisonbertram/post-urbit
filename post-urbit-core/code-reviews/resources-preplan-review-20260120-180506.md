## 1) Resource limits to define (concrete, with recommended defaults)

### A. Global (shell + all app webviews)
Define **targets**, **warning thresholds**, and **hard actions**. You likely can’t hard-cap WebView memory on all platforms, so “hard” often means “begin forced eviction / restart flow”.

**Device-classed defaults (auto-derived from physical RAM):**

| Physical RAM | Max Hot | Max Warm | App Webviews Total RSS Budget (soft) | Memory pressure warn / critical (system) |
|---:|---:|---:|---:|---|
| ≤ 8 GB | 2 | 2 | 1.5 GB | 0.75 / 0.85 |
| 16 GB | 3 | 4 | 3.0 GB | 0.80 / 0.90 |
| ≥ 32 GB | 4 | 6 | 5.0 GB | 0.85 / 0.92 |

Notes:
- “system pressure” is `(system_used / system_total)` (best cross-platform signal).
- “app webviews total RSS budget” is *your* measured sum of associated processes (best-effort per platform).

**Shell budget (from your spec’s acceptance criteria):**
- **Shell RSS target**: ≤ **200 MB**
- **Shell warn**: 250 MB
- **Shell critical**: 350 MB → enter **low-resource mode** (disable warm pool, evict all warms, reduce hot max to 1 until recovery)

### B. Per-app (per webview)
Even if enforcement is best-effort, define numbers and the actions taken.

**Memory (per app webview):**
- **Soft cap**: **300 MB RSS** (warn app + deprioritize)
- **Hard cap**: **500 MB RSS** (start eviction countdown if not focused; if focused, show “App using too much memory” banner and offer reload)

Rationale: your stated range is 50–350MB; 300MB catches runaway cases without punishing normal apps; 500MB is an “abnormal” threshold.

**CPU (per app webview):**
Define quotas in terms of **sustained CPU** and **background CPU**.

- **Foreground (focused) sustained CPU**: warn if **> 120% of one core** for **> 30s**
- **Background (warm/hidden) CPU**: warn if **> 10% of one core** for **> 30s**
- **Critical CPU**: if background **> 25%** for **> 15s**, force demote to **Cold** (after grace period)

(These are heuristics; different OS scheduling makes “exact” enforcement hard, but the policy still matters.)

**IPC/Bridge limits (important for both perf + abuse control):**
- `postbridge_invoke` **payload max**: **256 KB** (hard reject)
- **rate limit**: **50 req/s sustained**, **200 burst** per app session (token bucket)
- **concurrent in-flight**: 16 (backpressure)

### C. Storage limits
Split into **installed package** vs **runtime data** vs **logs**.

**Installed app package (immutable UI bundle):**
- **Soft limit**: 75 MB
- **Hard limit**: 150 MB (reject install unless explicitly overridden by “developer mode”)

**Per-app runtime data (IndexedDB/localStorage/cache within app origin):**
- **Default quota**: **256 MB**
- **Warn at**: 80%
- **Hard action at**: 100% → mark app “storage constrained”, prompt user to “Clear data / Increase quota”, and emit storage-pressure event

**Per-app logs/diagnostics (Rust-side):**
- 20 MB per app (rotating)
- 100 MB global cap (rotating)

---

## 2) LRU eviction specification (states, triggers, algorithm)

You already have `Hot / Warm / Cold`. Make eviction deterministic and auditable.

### A. State semantics
- **Hot**: visible + interactive (exactly one hot is typically focused/visible; others hot only if you support side-by-side panes)
- **Warm**: webview exists but **hidden** (`set_visible(false)` / minimized), not rendering; should naturally throttle timers in most engines
- **Cold**: webview destroyed; only persisted state remains

### B. Triggers (ordered, with hysteresis)
Eviction should be triggered by **any** of:

1. **Count-based**
   - if `hot_count > max_hot`: demote LRU hot (excluding focused) → Warm
   - if `warm_count > max_warm`: evict LRU warm → Cold

2. **Time-based**
   - if warm for `> warm_timeout_secs` (default **300s**): Warm → Cold

3. **Memory-pressure**
   - if `system_pressure >= warn_threshold`: begin “soft” eviction (Warm → Cold until recovered)
   - if `system_pressure >= critical_threshold`: “hard” eviction (evict all warm; demote any non-focused hot)

**Hysteresis:** don’t stop evicting until pressure falls below `warn_threshold - 0.05` (prevents thrashing).

### C. Candidate selection (precise ordering)
Define an eviction score; simplest is lexicographic:

1. **Never evict**: focused webview; shell; permission prompt UI if it’s in an app surface you must keep (ideally prompts are shell overlays anyway)
2. **Prefer evicting**:
   - Warm over Hot
   - Not “pinned” (user pinned / system-required)
   - Largest memory footprint first *when under memory pressure* (size-aware LRU)
3. Primary key: `last_active` ascending (LRU)
4. Tie-breakers: `estimated_rss` descending, then `created_at` ascending

### D. Graceful eviction handshake (must be specified)
Before Warm → Cold, do:

1. Emit `app://resource/evicting` to that app with `{ deadline_ms: 1500, reason }`
2. Allow app to respond via bridge `resource.prepare_for_unload()` (optional; bounded size, e.g. **64 KB**)
3. After deadline, destroy webview regardless (“fail closed” / don’t let apps prevent eviction)

### E. Persistence requirements
Specify **what the shell persists** even if app doesn’t cooperate:
- window geometry, last URL (should always be `postapp://app_id/...`), scroll position if you can capture it, last focused element (optional), session id bookkeeping
- app-provided `prepare_for_unload` blob stored encrypted/integrity-protected (size capped)

---

## 3) Platform-specific considerations that matter

### Windows (WebView2)
- **Process model**: one “webview” implies multiple processes (renderer/GPU/etc). Memory must be summed across associated PIDs.
- **Best pressure signal**: system commit + working set; also listen for low-memory notifications if available.
- **Useful capability (if exposed via wry/WebView2 bindings)**: `TrySuspend()` for warm webviews (can materially reduce CPU/memory).
- **Optional hard enforcement**: Windows **Job Objects** can cap CPU rate / memory *if* you can reliably include all spawned WebView2 child processes (often non-trivial, but worth a spike).

### macOS (WKWebView)
- WKWebView can be terminated by the OS under memory pressure; you must handle `webContentProcessDidTerminate` as a normal lifecycle path.
- Memory accounting is harder (shared processes / jetsam behavior). Prefer **system pressure + your own eviction policy** rather than per-webview hard caps.
- “Warm but hidden” may not free as much as expected; warm counts may need to be lower on older Macs.

### Linux (WebKitGTK)
- Memory varies widely by distro/WebKitGTK version; measure in CI across target distros.
- You can read per-process RSS via `/proc`, but correlating subprocesses to a given webview may be less direct than WebView2.
- Optional enforcement: **cgroups v2** (CPU.max / memory.high) if you choose to support it; otherwise rely on visibility throttling + eviction.

---

## 4) Built-in metrics & monitoring (what to collect, and why)

### A. Core runtime telemetry (Rust authoritative)
Collect per **app session** and globally:

**Per app session**
- `state`: hot/warm/cold
- `estimated_memory_rss_bytes` (best-effort)
- `cpu_pct` (rolling average: 5s, 30s)
- `bridge_invoke_rate`, `bridge_bytes_in/out`, `bridge_errors`
- `eviction_count` and last `eviction_reason` (count / time / memory / cpu / user)
- `crash_count` / abnormal termination count
- `time_to_launch_ms` (create webview → first paint / “ready” signal)
- `time_in_hot/warm/cold`

**Global**
- shell RSS / CPU
- total app RSS / CPU
- hot/warm counts
- eviction throughput (evictions/min)
- memory pressure level (normal/warn/critical) with durations
- “thrash” detector: evict+relaunch same app within N minutes

### B. User-facing diagnostics
- A **Resource Dashboard** in shell: top memory apps, recent evictions, “why was my app unloaded?”
- **Exportable** JSON diagnostics bundle (for bug reports), with privacy redactions

### C. Monitoring hooks
Even if you don’t ship remote telemetry, define an internal interface compatible with OpenTelemetry-style spans:
- span: `app.launch`, `webview.create`, `protocol.serve_file`, `eviction.run`

---

## 5) Communicating resource pressure to apps (protocol + semantics)

Apps only have the bridge, so define resource signals as **events delivered to the app webview** (origin-local), plus query APIs.

### A. Event levels (simple and actionable)
Emit to each app:

- `app://resource/pressure` with:
  - `level`: `normal | constrained | critical`
  - `signals`: `{ memory: ..., cpu: ..., storage: ... }`
  - `budgets`: `{ memory_soft_bytes, memory_hard_bytes, storage_quota_bytes }`
  - `recommended_actions`: e.g. `["clearCaches","reduceAnimations","stopPolling","persistDrafts"]`

### B. Targeted vs broadcast
- **Broadcast** `constrained/critical` when system pressure rises (everyone should reduce load)
- **Targeted** warnings when a single app breaches per-app thresholds

### C. Mandatory behaviors expected from apps (spec requirements)
When receiving:
- `constrained`: reduce caches, stop nonessential work, lower animation rate
- `critical`: persist state, release large buffers, stop background timers, prepare for eviction
- `evicting`: must return quickly; no long blocking operations

---

## 6) Shell APIs to expose for resource management (concrete)

### A. Shell-only (privileged) commands
Add to your shell command set:

1. **Introspection**
   - `shell_get_resource_snapshot() -> ResourceSnapshot`
   - `shell_get_app_resource_usage(app_id) -> AppResourceUsage`
   - `shell_get_storage_usage(app_id) -> StorageUsage`

2. **Policy configuration**
   - `shell_set_resource_limits(patch: ResourceLimitsPatch)`
   - `shell_set_app_priority(app_id, priority: "pinned"|"normal"|"background")`
   - `shell_set_storage_quota(app_id, quota_bytes)`

3. **Control / actions**
   - `shell_evict_app(app_id, target_state: "warm"|"cold", reason)`
   - `shell_clear_app_data(app_id, scope: "cache"|"storage"|"all")`
   - `shell_enter_low_resource_mode(enabled: bool)`

4. **Events (Rust → shell UI)**
   - `shell://resources/pressure_changed`
   - `shell://resources/snapshot_changed` (throttled, e.g. 1Hz)
   - `shell://resources/eviction` (audit log)

### B. App-facing (bridge) APIs
Expose a minimal, capability-guarded resource namespace:

- `resource.get_budget() -> { memory_soft, memory_hard, storage_quota, current_level }`
- `resource.on_pressure(level, signals, budgets)` (event subscription)
- `resource.prepare_for_unload() -> bytes` (≤ 64KB, timeboxed)
- `resource.request_quota_increase(bytes, reason) -> { approved: bool }` (shell prompts user)
- `resource.get_storage_usage() -> { used_bytes, quota_bytes }` (so apps can self-regulate)

---

### Final recommendation: codify this as a `ResourceConstraintsConfig`
Extend your existing `WebviewLifecycleConfig` into a single authoritative Rust config (loaded from defaults + settings), with:
- device-class derived defaults
- explicit thresholds + hysteresis
- deterministic eviction ordering
- structured reasons for every action (for auditability and user trust)

If you want, I can draft a concrete Rust struct + JSON schema for `ResourceConstraintsConfig`, plus the exact event/command names to match your existing `shell://...` and `app://...` conventions.
