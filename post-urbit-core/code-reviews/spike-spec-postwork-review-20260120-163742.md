## 1) Implementation-ready? (per spike)

### Cross-cutting readiness gaps (affects **all** spikes)
- **Missing test harness spec**: You show page probes that `console.log()` JSON, but you do not specify *how Rust captures results* reliably on all platforms (WebView2/WKWebView/WebKitGTK). Implementation-ready needs:
  - How to subscribe to console messages (`on_console_message`-equivalent) or an explicit *result exfil path* (e.g., `postapp://results?...` beacon, `fetch("postapp://__results__/")`, or `window.location = postapp://__result__?...`).
  - Timeouts, retries, and deterministic completion detection.
  - Where/when the harness writes `spikes/results/*.json`.
- **Tauri API surface uncertainty**: Several Rust snippets use APIs that may not exist as-written in Tauri 2.x (e.g., `WebviewBuilder::new("label", "url")`, `disable_tauri_api()`, and the exact `register_uri_scheme_protocol` signature/types). The spec must pin to *concrete* Tauri 2.x APIs (or cite exact crate versions and modules) to be implementation-ready.
- **Network dependency / false positives**: CSP tests that “pass” because the machine is offline or DNS is blocked are not acceptable. You need a design that can distinguish “blocked by CSP” vs “failed due to network.”
- **Multi-webview “unstable feature” prerequisite is not asserted**: You assume multi-webview is available and behaves similarly across platforms; the spec should include a tiny “pre-flight” check or explicitly list required Tauri feature flags + platform constraints.
- **Result normalization**: You need a rule for platform-specific error names/messages (e.g., `TypeError` vs `NetworkError`) so the harness doesn’t misclassify.

### Spike 0.1 (Custom Scheme Secure Context)
**Close to implementation-ready**, but not fully:
- Good: probes for `isSecureContext`, `crypto.subtle`, IndexedDB, localStorage.
- Gaps:
  1. **Persistence isn’t actually tested across navigation/restart**. Your “persistence” probe writes then reads in the same page session. You need at least:
     - write → reload (or close/reopen webview) → read, and ideally write → app restart → read.
  2. **Service Worker requirement mismatch**: You list SW as a reason this matters, but you don’t test SW registration. Either:
     - Add SW probe as required (if you truly depend on it), or
     - Remove SW from the “Why Critical” section.
  3. **Origin could be `null` / opaque** on some custom schemes. You record origin, but don’t fail explicitly if `origin === "null"`; you should.
  4. `crypto.randomUUID()` is used later; it’s generally available, but for maximum determinism use `crypto.getRandomValues` to avoid edge compatibility surprises.

### Spike 0.2 (Per-App Origin Isolation)
**Not implementation-ready; the current test logic can yield false PASS even when storage is shared.**
- The JS probes are **single-webview self-consistency checks**. If storage is shared, *both* webviews can still pass because each writes then reads its own value immediately.
- To prove isolation, you need **cross-webview orchestration** (Rust-coordinated) such as:
  - Phase A: webview A writes value `A`.
  - Phase B: webview B reads key and must *not* see `A`; then writes `B`.
  - Phase C: webview A reads key and must *not* see `B`.
- For IndexedDB, same issue: last writer wins; each webview reading immediately after its own write doesn’t detect sharing.
- Also add coverage for:
  - **Cookie jar isolation** (`document.cookie` where applicable).
  - **CacheStorage / SW cache partitioning** if you ever plan to use SW.
  - **Storage partitioning under same scheme but different host** is exactly what differs across engines; you need to explicitly capture cases where host is ignored.

### Spike 0.3 (CSP Enforcement via Custom Protocol)
**Partially specified, but has correctness and flakiness risks.**
Key gaps/corrections:
- **CSP syntax likely wrong/fragile for custom scheme sources**:
  - In CSP, a scheme source is `postapp:` not `postapp://host`. Host-specific sources are unusual and may be ignored. You already have `'self'`; that should cover same-origin loads. Keep it simple:
    - `script-src 'self'`
    - `connect-src 'none'`
- **False positives due to offline/DNS**:
  - Require evidence of CSP enforcement, not just request failure. E.g., require `securitypolicyviolation` events that match the attempted directive (`connect-src`, `script-src`) for each blocked action.
- **WebSocket endpoint is unreliable**: `wss://echo.websocket.org` is commonly dead. Use a stable endpoint or remove reliance on a successful remote host entirely and validate via CSP violation event.
- **Header application scope**:
  - You must test CSP header on the **main document and subresources** served via the custom protocol (JS file, image, etc.). Some engines treat custom protocol responses differently for subresources.
- Add a probe that ensures **inline script blocking** works *when configured*, because later you list “inline scripts” in security goals but your test page itself uses a script tag. If you keep external JS, test inline by injecting an inline `<script>` dynamically and requiring it to be blocked (with a violation).

### Spike 0.4 (Sandbox Containment Proof)
**Conceptually solid, but not implementation-ready due to Tauri API uncertainties and missing escape vectors.**
- Biggest issue: you rely on `.disable_tauri_api()` as if it exists. The spec must define the *actual* mechanism to prevent Tauri API injection in child webviews for Tauri 2.x:
  - If the answer is “not possible,” this spike needs an explicit fallback design (and might become an earlier STOP).
- Navigation/popup blocking:
  - `window.open()` returning null is not a reliable universal signal. The authoritative signal is whether the webview emits a new-window event and whether the handler blocks it.
  - `on_navigation` may not catch all navigation vectors consistently (meta refresh, `location.replace`, link clicks, redirects). You need additional probes:
    - `<meta http-equiv="refresh">`
    - `<a target=_blank>` click
    - `location.replace()`, `location.assign()`
- Missing critical containment probes:
  - Attempt navigation to **another app’s origin**: `postapp://other-app/index.html` (must be blocked by policy).
  - Attempt loads of `data:` and `blob:` top-level navigations (often used for code injection).
  - Confirm **no access to Tauri IPC bridge endpoints** (some builds expose internal IPC objects beyond `__TAURI__` / `__TAURI_INTERNALS__`).
- You should also specify how to verify “no privileged browser/OS APIs” (right now it’s asserted, not tested). Even a minimal set helps (clipboard APIs, file picker APIs, etc.), or clarify that those are out of scope for Phase 0.

### Spike 0.5 (IPC Primitive Feasibility)
**Not implementation-ready; it lacks experiment design detail.**
- You list pass criteria, but you do not define:
  - The concrete IPC mechanism(s) to test in Tauri 2.x **when Tauri APIs are disabled in the untrusted webview**.
  - How the webview sends a request to Rust (what JS API exists in that context?).
  - How identity binding is enforced/measured (what is the “webview label” source of truth and how to prevent spoofing?).
  - How latency p95 is measured: sample size, payload sizes, warmup, clock source, and how results are collected.
- As written, this spike is the highest-risk spec gap because the entire platform depends on an IPC path that remains undefined under the containment constraints of Spike 0.4.

### Spike 0.6 (Multi-Webview Memory Baseline)
**Not implementation-ready; measurement methodology is missing.**
- You need to specify:
  - What “memory” means per platform (RSS? private bytes? working set? proportional set size?).
  - How to attribute memory to the app when webviews spawn multiple helper processes (WebView2 in particular).
  - When to sample (after load + idle, after GC, after N minutes).
  - Debug vs release build requirement (debug builds skew memory/time heavily).
  - Whether you measure “cold start” vs “warm start” creation time.

### Spike 0.7 (Crash Containment Optional)
**Not implementation-ready** (but optional):
- Needs a defined way to induce crash/hang per engine:
  - Hang: infinite loop on main thread.
  - Crash: memory exhaustion, deliberate WebView process kill (if possible), or malformed content.
- Needs “shell remains responsive” measurement criteria (UI input responsiveness? heartbeat?).

---

## 2) Are pass criteria measurable and correct?

### Measurable
- 0.1 and 0.3 have mostly machine-verifiable probes, but **must remove ambiguity** (see offline false positives and persistence issue).
- 0.2 criteria are stated in a machine-verifiable way, but the **implemented probes don’t actually measure them** (needs orchestration).
- 0.4 criteria are mostly measurable, but two conditions are shaky:
  - `window.open() returns null` is not robust across platforms.
  - Navigation “blocked” based on `location.href` staying the same is timing-sensitive and can be flaky.

### Correctness issues
- **Spike 0.2 fail mitigation “SDK-level storage partitioning” is not a security fix** against malicious apps. It only helps honest apps. If origins are shared, a malicious app can still read “other app” keys/DBs regardless of SDK conventions. This should be treated as much closer to CRITICAL unless you have an enforceable isolation mechanism (separate profiles/user data dirs, true origin partitioning, or virtualization enforced by Rust).
- **Spike 0.6 is labeled “NOT a hard blocker” but all criteria are marked required=true and “must pass on all 3 platforms.”** That’s internally inconsistent. If it’s non-blocking, required flags and overall pass condition should reflect that (or reclassify as HIGH/CRITICAL).

---

## 3) Missing edge cases / platform-specific concerns

### Custom scheme / secure context realities (major platform delta risk)
- **WKWebView** often treats custom schemes as *not secure contexts* unless specifically configured; you may be forced into:
  - `https://{app-id}.localhost` with a virtual host mapping, or
  - a loopback server, or
  - `tauri://localhost` asset protocol if it gives secure context behavior.
Your spec mentions alternatives, but does not define how to configure them on each platform (which matters for implementation readiness).

### Storage partitioning nuances
- Some engines partition by **scheme only**, ignoring host for custom schemes; others treat them as opaque. You should explicitly record:
  - `origin`, `site`, `storageKey`-equivalent signals where available,
  - whether host-based partitioning occurs,
  - whether two hosts share cookies/localStorage/IndexedDB.

### CSP enforcement on non-HTTP schemes
- CSP headers may be ignored or partially applied on non-HTTP responses in some webviews.
- Subresource loads might not inherit expected CSP if the engine treats the custom scheme differently.

### Navigation handling differences
- WebView2/WKWebView/WebKitGTK differ on which navigation callbacks fire for:
  - same-document navigations,
  - redirects,
  - `window.open` / target=_blank,
  - downloads.
You need to broaden navigation escape attempts and validate via the *native handler’s decision logs*, not just JS observations.

### Process isolation assumptions
- Multi-webview may or may not imply separate renderer processes. If you later depend on crash isolation, you should at least record process model observations during spikes (even if optional).

---

## 4) Is the Go/No-Go matrix correct?

Partially, but there are critical misclassifications:

- **0.1 / 0.3 / 0.4 as STOP conditions**: reasonable.
- **0.2 currently treated as “implement SDK-level storage partitioning”**: not adequate if the threat model includes malicious apps (it does). Recommend:
  - If you cannot guarantee per-app storage isolation *enforced by the platform*, treat as **CRITICAL STOP** (or require a robust mitigation like separate storage partitions/profiles per app).
- **0.5 marked HIGH, but if no IPC mechanism works you can’t build the platform**: that is effectively **CRITICAL**. The matrix should say STOP if neither MessagePort nor a defined Rust-mediated mechanism meets requirements.
- **0.6 labeled non-blocking but required on all platforms**: matrix and criteria conflict. Decide:
  - Either make it HIGH/CRITICAL (if memory limits are truly gating), or
  - Keep it MEDIUM and relax “must pass on all platforms” into “must produce baseline numbers; failures require design adjustment.”

Also: there’s a **spike numbering mismatch** between `00-PLANNING_DOMAINS.md` and `PHASE_0_GATING_SPIKES.md` (planning doc’s 0.1/0.2/0.3/0.4 are different topics than Phase 0 doc’s 0.1–0.4). That’s a spec governance issue: Go/No-Go decisions become ambiguous if stakeholders refer to “Spike 0.1” differently.

---

## 5) Implementation readiness rating (1–10)

**Rating: 6/10**

### Why not higher
- Spike **0.2** does not actually validate isolation as written (high risk of false PASS).
- Spike **0.5** lacks a concrete, testable experiment design under the containment constraints (biggest architectural unknown).
- Spike **0.6** lacks defined measurement methodology and has inconsistent “non-blocking” vs “required” language.
- Several Rust/Tauri API calls are presented as if they exist, but aren’t pinned/verified (implementation friction risk).

### What would get it to ~8–9/10 quickly
1. Add a **harness design section**: how results are collected from webviews, timeouts, orchestration, and file outputs.
2. Rewrite Spike **0.2** with a Rust-coordinated multi-phase cross-read test (and include IndexedDB/cookie/cache variants).
3. Fully specify Spike **0.5** with at least one *implementable* Rust-mediated IPC route that still holds when Tauri APIs are disabled in untrusted webviews, plus a clear latency benchmark method.
4. Fix CSP source syntax and remove network flakiness by requiring `securitypolicyviolation` evidence per attempted violation.
5. Reconcile Go/No-Go classifications and resolve spike numbering inconsistencies across docs.

If you want, I can propose concrete revised probe sequences for 0.2/0.3/0.4 and a minimal harness architecture that works across all three engines.
