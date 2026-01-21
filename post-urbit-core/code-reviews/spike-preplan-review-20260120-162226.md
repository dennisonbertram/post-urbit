## 1) Are the 4 spikes sufficient to prove feasibility?

They cover the *right four risk areas* (containment, CSP, perf/memory, IPC primitive), but **as written they are not sufficient** to make a confident go/no-go call for the architecture, mainly because two foundational assumptions are unproven:

1) **Custom-scheme viability as a “real origin”** across WKWebView/WebKitGTK/WebView2  
   You need to know whether `postapp://` behaves like a secure, origin-bearing scheme (Secure Context, storage availability, CSP applied to main frame, etc.). If it’s treated as an “opaque/insecure” origin (especially on WKWebView), your sandbox + storage + many web APIs may break or degrade.

2) **IPC design assumption (“MessagePort transfer cross-webview”)**  
   Cross-*webview* MessagePort transfer is unlikely to work on all engines because transfer typically requires a `WindowProxy` relationship (iframe/parent) or browser-managed channel primitives that aren’t exposed across separate embedded webviews. This is a likely fail, and you should treat it as an early decision point that may force **iframe-based isolation** or **Rust-mediated IPC**.

So: **good start**, but I’d add 2–3 small gating experiments (below) to actually “prove feasibility”.

---

## 2) Critical experiments missing (additions I recommend)

### Missing Spike A: “Custom protocol = Secure Context + storage works”
**Question:** Is `postapp://app-id/...` a secure context and does it support required web platform primitives?

**Why it’s critical:** If `isSecureContext` is false or IndexedDB/localStorage/cookies behave oddly, you’ll be forced into a different origin strategy (e.g., loopback HTTP, virtual host mapping, or Tauri’s asset protocol).

**Experiment checklist (per platform):**
- `window.isSecureContext === true`
- `crypto.subtle` exists and can generate a key
- `indexedDB.open()` works and persists across reload
- `localStorage` read/write persists across reload
- (Optional, but very useful) `CacheStorage` and `serviceWorker` availability (likely limited on custom schemes)

**Pass criteria suggestion:** At least Secure Context + IndexedDB + localStorage must work and persist.

---

### Missing Spike B: “Per-app origin separation actually isolates browser storage”
**Question:** Does `postapp://appA/...` have a different origin than `postapp://appB/...` and do they get isolated storage?

**Experiment:** Load two app webviews with different hosts (`postapp://app-a/` vs `postapp://app-b/`), write `localStorage["x"]=...` and IndexedDB records, confirm they cannot be read by the other.

**Pass criteria:** No cross-origin access; storage is partitioned by (scheme, host).

---

### Missing Spike C (optional but high value): “Webview crash/kill containment”
**Question:** If an app webview crashes/hangs, does the shell remain responsive and can you restart only that app?

**Experiment:** Deliberately crash/hang a webview (infinite loop / memory balloon / kill renderer process where possible). Verify shell survives and can recreate the app webview cleanly.

**Pass criteria:** Shell remains responsive; app can be restarted without full app restart.

---

## 3) Are the pass criteria correct and measurable?

They’re directionally correct, but **not yet measurable/operational**. Each spike needs:
- a deterministic test harness,
- explicit measurement method,
- and “what evidence counts as pass”.

Below are concrete fixes.

---

### Spike 0.1 (Sandbox Containment Proof) — tighten pass criteria
Current criteria are good but incomplete. Suggested measurable criteria:

**Pass Criteria (revised):**
- **No privileged IPC path accessible**
  - [ ] In the untrusted webview, `__TAURI__`, `__TAURI_INTERNALS__`, and any known IPC globals (`__TAURI_IPC__`, `window.ipc`, etc.) are **absent** *or non-functional*.
  - [ ] Any attempt to send messages via the underlying engine bridge (`chrome.webview.postMessage` on WebView2, WKScriptMessageHandler equivalent) results in **no privileged action** and is logged as rejected.
- **Navigation hard block**
  - [ ] Any top-level navigation to `http(s):`, `file:`, `data:`, `javascript:`, `tauri:` (or other internal schemes) is blocked (event observed and cancelled).
- **Popup hard block**
  - [ ] `window.open()`, `<a target=_blank>`, and scripted new-window requests do not create a new webview/window and return `null`/fail.

**Make it measurable:** implement a malicious test page that runs ~20 probes and prints a single JSON “RESULT” line to console; Rust parses that + counts nav/new-window events.

---

### Spike 0.2 (CSP Enforcement via Custom Protocol) — make it verifiable
The criteria “CSP headers applied” is ambiguous. You want to verify enforcement.

**Pass Criteria (revised):**
- [ ] Response headers for main document include the expected CSP (validated in protocol handler / debug log).
- [ ] At runtime, at least N expected violations occur and are observable via `securitypolicyviolation` events:
  - inline script blocked by `script-src 'self'` (no `'unsafe-inline'`)
  - external script `https://example.com/x.js` blocked
  - `fetch('https://example.com')` blocked when `connect-src 'none'`
- [ ] Same test passes on WebView2 + WKWebView + WebKitGTK.

**Make it measurable:** JS registers `window.addEventListener('securitypolicyviolation', ...)` and logs structured events; Rust asserts they happened.

---

### Spike 0.3 (Multi-Webview Memory Baseline) — define the metric precisely
The thresholds are fine as a first gate, but “memory usage” must be defined.

**Pass Criteria (revised):**
- [ ] Measure **Total RSS of app + all renderer/helper processes** attributable to the run.
- [ ] For N=5 app webviews loaded to a steady idle state (e.g., 30s after load):
  - [ ] Total RSS < 2GB on reference machines
  - [ ] Incremental per-webview overhead is recorded (delta from N=1)
- [ ] Webview creation time:
  - [ ] p95 time from “create requested” → “DOMContentLoaded + first paint signal” < 3s

**Make it measurable:** standardize:
- machine spec + OS build + WebView runtime version,
- steady-state waiting period,
- and include p50/p95 over 10 runs.

---

### Spike 0.4 (MessageChannel Transfer to Webview) — likely incorrect assumption
“Transfer a MessagePort to an isolated webview” is not a standard capability across separate embedded webviews.

**Pass Criteria (revised):**
- [ ] Determine conclusively whether **cross-webview** `MessagePort` transfer is possible on each platform.
- If **not possible** (likely):
  - [ ] Identify an alternative IPC primitive that still supports your security invariants (token binding, per-webview identity, replay protection).
  - [ ] Demonstrate request/response + event push with bounded backpressure.

This spike should be explicitly allowed to “fail” and still produce a viable path (e.g., Rust-mediated RPC channel).

---

## 4) What order should the spikes be run?

Recommended order (dependency-aware):

1) **Spike 0.2A (new): Custom scheme secure context + storage sanity**  
   If `postapp://` can’t be a proper origin, it impacts *everything*.

2) **Spike 0.2 (CSP via custom protocol)**  
   CSP enforcement is foundational to the sandbox.

3) **Spike 0.1 (Sandbox containment / navigation / popups)**  
   Now test the malicious app under the real loading + CSP conditions.

4) **Spike 0.4 (IPC primitive feasibility)**  
   This informs the Secure Bridge Protocol architecture (MessagePort vs Rust-mediated).

5) **Spike 0.3 (memory baseline)**  
   Run once you have the *same configuration you’d actually ship* (protocol + nav policy + whatever IPC plumbing), because those choices can affect process model and memory.

(0.3 can run in parallel earlier for a quick smell test, but the “official numbers” should be taken after the architecture knobs are set.)

---

## 5) Concrete implementation approaches (per spike)

### Spike 0.1 — Sandbox Containment Proof (implementation)
**Goal:** create one trusted “shell” webview + one untrusted “app” webview and prove containment.

**Rust-side actions (Tauri/Wry level):**
- Create untrusted webview with:
  - **no Tauri JS API injection** (global disabled if supported)
  - **no invoke handler** (or a handler that hard-rejects everything for that label)
  - strict **navigation handler**: allow only `postapp://{app-host}/...`
  - strict **new-window handler**: always deny
- Add logging counters:
  - `blocked_navigation_count`
  - `new_window_attempt_count`
  - `ipc_message_attempt_count` (if any path exists)
  - `invoke_attempt_count` (if invoke is reachable)

**Malicious app page probes (JS):**
- Check globals:
  - `typeof window.__TAURI__`
  - `typeof window.__TAURI_INTERNALS__`
  - `typeof window.__TAURI_IPC__` / `window.ipc`
- Attempt privileged calls (should fail):
  - call invoke if present
  - call known plugin commands if present
- Attempt escapes:
  - `location.href = "https://example.com"`
  - `location.href = "file:///etc/passwd"` (or Windows equivalent)
  - `window.open("https://example.com")`
  - `<a target=_blank href=...>.click()`

**Evidence collection:** log a single JSON line to console like:
```js
console.log("RESULT " + JSON.stringify({ tauriGlobal:false, invokeWorked:false, navBlocked:true, popupsBlocked:true }));
```
Rust parses console output (or another deterministic channel) and fails the run if any invariant is violated.

---

### Spike 0.2 — CSP Enforcement via Custom Protocol (implementation)
**Rust protocol handler:**
- Register `postapp://` handler that:
  - resolves request path to bundled app asset
  - sets correct `Content-Type`
  - injects headers:
    - `Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`
    - `X-Content-Type-Options: nosniff`
    - (optional) `Cross-Origin-Resource-Policy: same-origin`

**Test page design:**
- Use an external script `app.js` (so it’s allowed by `script-src 'self'`) that:
  - registers `securitypolicyviolation` listener
  - attempts:
    - `eval("1")` (should violate if `unsafe-eval` not allowed)
    - create `<script src="https://example.com/x.js">`
    - `fetch("https://example.com")`
  - logs the observed violation events

**Pass check:** required directives generate expected `securitypolicyviolation` events and the disallowed actions do not succeed.

---

### Spike 0.3 — Multi-webview Memory Baseline (implementation)
**Harness shape:**
- CLI flag or compile-time feature: `--webviews N`
- Create N app webviews, each loads a minimal “idle page”
- Record:
  - time to create each webview
  - time to DOMContentLoaded (JS logs timestamp)
  - memory totals at:
    - after all loaded
    - after 30s idle

**Memory measurement approach (practical):**
- Use a per-run unique identifier in the webview runtime data dir / user data dir if the platform allows it.
- Then sum RSS of:
  - main process
  - child/helper processes matching that data dir in command line (especially important on Windows WebView2).

**Output:** write JSON report:
```json
{ "platform":"windows", "n":5, "rss_total_mb":1234, "create_p95_ms":1800, "versions":{...} }
```

---

### Spike 0.4 — IPC Primitive Feasibility (implementation)
**Step 1: falsify/confirm MessagePort transfer**
- Attempt to create a `MessageChannel` in one context and transfer `port2` to the other.
- If you’re using **iframes**, do the canonical transfer via `iframe.contentWindow.postMessage(..., [port])`.
- If you’re using **multi-webview**, explicitly test whether any available bridge supports transferables (expect “DataCloneError” / unsupported).

**Step 2: if MessagePort transfer fails (likely), prove an alternative**
Concrete alternative to validate in this spike:
- Use a **single minimal IPC entrypoint** (engine’s host messaging / wry ipc) that accepts only:
  - `handshake.init`
  - `rpc.request` (CBOR)
- Bind identity to the webview label on the Rust side.
- Return responses only to that same webview.
- Demonstrate:
  - request/response
  - server->client event push
  - ability to disable/ignore any other message routes

**Pass evidence:** two webviews cannot spoof each other’s session; replay attempts rejected; latency measured.

---

## Bottom line / go-no-go clarity
- Keep the four spikes, but **amend Spike 0.4 expectations** and **add a “custom scheme secure context + storage + origin isolation” spike**.  
- Tighten pass criteria so each spike produces a machine-verifiable report (JSON) and is repeatable across all three platforms with version stamps.

If you want, I can rewrite the Phase 0 section into an “executable” spike plan format (per spike: harness, steps, instrumentation, expected outputs, and exact thresholds).
