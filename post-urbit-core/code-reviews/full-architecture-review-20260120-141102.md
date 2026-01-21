## 1. COHERENCE CHECK (consistency / contradictions / gaps)

Overall the documents are directionally aligned (Tauri shell + sandboxed apps + capability/permission enforcement + bridge), but there are several **hard contradictions** and a few **important gaps** that need reconciliation before implementation.

### A. Sandbox model contradiction: **iframe vs multi-webview**
- **FRONTEND_ARCHITECTURE.md** is built around **single webview shell + sandboxed iframes** for apps.
- **TAURI_MULTIWEBVIEW_RESEARCH.md** recommends **multi-webview** because iframes share renderer/process and therefore are not sufficient for untrusted apps.
- **TAURI_INTEGRATION_PLAN.md** continues with the **iframe + MessageChannel** approach (AppContainer uses iframe).

**Action:** pick a primary isolation model:
- If apps are *untrusted third-party code*, multi-webview should be the default (or at least for third-party apps).
- If iframes remain, treat them as “semi-trusted / marketplace-curated” only, because they do not provide crash containment nor strong isolation.

### B. Bridge protocol mismatch: insecure `postMessage` vs secure MessagePort
- **FRONTEND_ARCHITECTURE.md** shows a global `window.addEventListener('message')` router with `postMessage(..., '*')` responses. That is the classic insecure pattern.
- **SECURE_BRIDGE_PROTOCOL.md** correctly replaces this with a **one-time handshake + MessageChannel/MessagePort** and CBOR envelopes.
- **TAURI_INTEGRATION_PLAN.md** uses MessageChannel + `bridge_request`, consistent with the secure doc.

**Action:** update FRONTEND_ARCHITECTURE.md to remove the global postMessage router pattern entirely and reflect the MessagePort + CBOR envelope approach.

### C. API naming inconsistencies (will cause real integration churn)
Examples:
- Methods in FRONTEND_ARCHITECTURE.md: `storage:get`, `identity:get`, `messaging:send`
- Methods in SECURE_BRIDGE_PROTOCOL.md and TAURI_INTEGRATION_PLAN.md: `storage.get`, `system.get_identity`, `messaging.send`
- Session command naming differs: `create_app_session` vs `apps_create_session`

**Action:** standardize:
- Use one method namespace (`storage.get`, `storage.set`, `messaging.send`, etc.).
- Use one command naming convention (`apps_create_session` or `create_app_session`, not both).
- Treat this as a public ABI for SDK/apps: version it, don’t bikeshed later.

### D. CSP / embedding contradiction: `frame-ancestors 'self'` will break iframe embedding
In **TAURI_INTEGRATION_PLAN.md** CSP builder includes:
- `frame-ancestors 'self'`

If the shell is `tauri://localhost` (or whatever Tauri uses on the platform), and the app is `postapp://com.example.app`, then `frame-ancestors 'self'` **does not include the shell origin**, so the app cannot be iframed by the shell. That directly conflicts with the iframe-based architecture.

**Action options:**
- If using iframes: set `frame-ancestors` to include the shell origin explicitly (and ideally only that origin).
- If using multi-webview (no iframe): set `frame-ancestors 'none'` to prevent embedding entirely.

### E. Invalid/ineffective CSP placement in FRONTEND_ARCHITECTURE.md
FRONTEND_ARCHITECTURE.md shows an iframe attribute like:
```html
<iframe ... csp="..." />
```
That is **not a standard enforcement mechanism** in browsers/webviews. CSP must come from **HTTP headers** or a `<meta http-equiv="Content-Security-Policy">` inside the document (which you cannot trust apps to include).

**Action:** enforce CSP in the **custom protocol response headers** (as shown in TAURI_INTEGRATION_PLAN.md) and remove the iframe-attribute CSP concept.

### F. Tauri “capabilities per app” vs dynamic installed apps
TAURI_MULTIWEBVIEW_RESEARCH.md leans heavily on Tauri v2 capability files like:
```
capabilities/app-todo.json
```
But your platform goal is **install arbitrary apps after the shell is shipped**. Tauri capabilities are typically **packaging-time/static**. That means:
- you likely **cannot generate new capability files at runtime** for newly installed apps (without rebuilding the desktop binary).

**Action:** do not rely on static per-app capability files as your primary enforcement for third-party apps. Use:
- a single exposed command surface (e.g., only `bridge_request`) and
- enforce permissions/session binding in Rust using caller context (window/webview label) + session tokens.

---

## 2. SECURITY ASSESSMENT (1–10 + remaining attack vectors)

### Rating: **6.5 / 10** as written; **8 / 10** achievable with targeted fixes

You have good building blocks:
- MessagePort isolation + CBOR envelopes
- HMAC session tokens, replay window, timestamps
- “connect-src 'none'” concept
- path traversal prevention
- explicit permission registry and prompting concept
- audit logging concept

But there are still major risks not fully addressed.

### Key remaining vulnerabilities / attack vectors

#### A. **Tauri IPC escape / plugin access from untrusted content**
If untrusted app code can access `__TAURI__` APIs (or invoke arbitrary commands/plugins), it can bypass your bridge and jump straight into native capabilities.

This is the single biggest “desktop app platform” footgun.

**Mitigations (must do):**
- Ensure app webviews/iframes cannot access Tauri APIs except what you explicitly allow.
- Enforce “caller identity” on every command:
  - Accept a `Window`/`WebviewWindow` handle parameter in Tauri commands and check `window.label()` is the shell vs an app.
  - Expose only `bridge_request` to app contexts; everything else should be shell-only.
- If possible, disable global Tauri API injection for app contexts (implementation depends on Tauri v2 specifics; verify early with a spike).

#### B. **Shell compromise = platform compromise**
Your threat model says shell is trusted, but the shell is a big React app. Any XSS in shell becomes “root”.

**Mitigations:**
- Treat the shell as a hardened component: strict CSP, no remote content, no `dangerousDisableAssetCspModification`, no dynamic code loading, lock down navigation, sanitize any HTML rendering, etc.
- Move as much authorization logic into Rust as possible (don’t let shell “decide” permissions; shell should only render prompts).

#### C. Session token not bound to *where it came from*
The secure protocol validates `(session_id, token)`, but there is no binding to:
- a specific webview label / iframe instance, or
- a specific transferred MessagePort, or
- a specific origin.

So if a token leaks (via a bug, logging, shell XSS, clipboard, etc.) it is reusable.

**Mitigation:**
- Bind session to a concrete runtime identity:
  - include `caller_webview_label` (derived from command context) in validation; session manager stores expected label.
  - optionally rotate per-session secrets frequently; keep sessions short-lived.

#### D. CSP realism across WebViews
“connect-src 'none'” is good, but real exfil paths remain unless you also handle:
- navigation (`window.location`, top-level navigation, `window.open`)
- custom schemes / external app handlers (`mailto:`, `tel:`)
- downloads / `<a download>` handling
- clipboard write (you currently allow it in examples)
- print / PDF export (platform dependent)
- drag-and-drop bridging to shell

**Mitigation:**
- In iframe mode: use sandbox flags to block popups and top navigation (do **not** add `allow-popups`, do **not** add `allow-top-navigation`).
- Add webview-level navigation handlers (Tauri/Wry hooks) to deny non-`postapp://` navigations.
- Add explicit policies for clipboard, file pickers, external open.

#### E. DoS vectors against Rust backend
- Replay protection map (`seen_request_ids`) can become an unbounded memory sink if not TTL-pruned.
- Rate limiting is described but not integrated into the bridge path.
- Subscription backpressure described but not wired to UI/SDK behavior.

**Mitigations:**
- Use a bounded TTL cache (e.g., `lru_time_cache`/custom) for request IDs.
- Enforce per-session/per-app quotas: requests/sec, max in-flight, max payload size, max subscriptions.

#### F. Supply-chain / update trust is not implemented
Docs mention signing, but there’s no concrete trust model:
- Who are “known developers”?
- Where are keys stored?
- How is revocation handled?
- How do you prevent downgrade/replay of old signed versions?

**Mitigation:** you need a real package signature + transparency/revocation story (see “Critical Missing Pieces”).

---

## 3. FEASIBILITY ASSESSMENT (is it realistic? hardest blockers)

It’s realistic **if you narrow scope and resolve the sandbox/IPC enforcement details early**.

### Hardest technical challenges / potential blockers

1. **Reliable isolation model on all platforms**
   - iframe isolation is not sufficient for hostile code.
   - multi-webview is promising, but:
     - it’s marked “unstable” in Tauri 2.x in your doc,
     - behavior differs across WebView2/WKWebView/WebKitGTK,
     - lifecycle and memory management get complex fast.

2. **Preventing untrusted apps from accessing Tauri native APIs**
   - This is usually where Electron/Tauri “app platforms” fail.
   - You must prove (with a spike) that installed app content cannot call privileged Tauri commands/plugins.

3. **Dynamic app permissions vs static Tauri capabilities**
   - If Tauri capabilities are static at build time, you cannot map “installed apps” → “capability file” straightforwardly.
   - Therefore, your enforcement must primarily be in Rust, keyed by caller context + session, not by static allowlists.

4. **Performance of bridge_request round-trips**
   - In the current AppContainer, every app request goes:
     `app -> MessagePort -> shell -> invoke('bridge_request') -> Rust -> shell -> MessagePort -> app`
   - That’s workable, but for chatty APIs (storage, CRDT subscriptions) it can become a bottleneck. You’ll likely need batching/subscriptions quickly.

---

## 4. CRITICAL MISSING PIECES (must-have for production launch)

1. **App package format + signing + verification + revocation**
   - Define `.postapp` precisely: manifest, assets, signatures, hashes.
   - Implement verification in Rust: signature validation, hash tree/manifest digest.
   - Trust store: developer public keys, key rotation, revocation list, “first install trust” UX.
   - Update security: prevent downgrade attacks, enforce monotonically increasing version/timestamp, signed update metadata.

2. **A complete permission persistence + prompting flow (end-to-end)**
   - Docs show a prompt UI and a registry, but you need:
     - a durable store for grants/denials (per-app, per-capability, per-tier)
     - UX rules for PromptOnce vs PromptAlways
     - audit log persistence + viewer UI
     - migration/versioning of permissions

3. **Concrete decision and implementation for app isolation**
   - Production needs a clear answer: multi-webview vs iframe, and what class of apps get which.
   - If you ship with iframes for third-party code, you are effectively accepting “one renderer exploit compromises everything.”

4. **Lockdown of navigation/external open/download/file access**
   - Central policy + hooks.
   - Test matrix per OS/webview engine.

5. **SDK/runtime compatibility contract**
   - Version negotiation: platform_version, protocol versioning, deprecation strategy.
   - App compatibility rules (manifestVersion, required platform APIs).

---

## 5. IMPLEMENTATION RISKS (top 3 derailers)

1. **Untrusted app escapes sandbox (Tauri IPC/plugin exposure or webview misconfig)**
   - If an app can invoke privileged native APIs, the platform’s security model collapses.
   - This is existential: it will block release.

2. **Multi-webview memory/performance becomes unacceptable**
   - 5–10 apps open could push memory into multiple GB depending on platform.
   - If UX becomes sluggish or the OS kills processes, you’ll fight fires instead of building platform features.

3. **Scope creep: platform + marketplace + CRDT + messaging + permissions all at once**
   - You need a vertical slice that proves the model early (load app → secure bridge → storage → permissions) before building “ecosystem”.

---

## 6. RECOMMENDED CHANGES (3 changes I would make)

### 1) Make **multi-webview the default for third-party apps**, keep iframes only for “system apps”
- Treat iframes as a convenience for trusted/first-party UI modules.
- For marketplace apps: multi-webview (crash containment + stronger isolation).
- This aligns with your own research doc and avoids shipping a known-weak isolation boundary.

### 2) Collapse the native surface area exposed to apps to **one command: `bridge_request`**
- Do not expose “storage_get”, “apps_install”, “shell”, plugin commands, etc. to app contexts.
- Everything goes through `bridge_request` with:
  - session validation
  - permission check
  - rate limit
  - audit log
  - payload size limits
- Enforce **caller window/webview label** in Rust for all other commands (shell-only).

This reduces “oops we forgot to protect that command” failures.

### 3) Fix CSP/embedding rules and explicitly model **who is allowed to embed whom**
- Decide: iframe embedding or not.
- If embedding: `frame-ancestors` must include only the shell origin.
- Add a webview-level navigation denylist/allowlist (not just CSP).
- Remove the non-standard iframe `csp="..."` idea from the architecture.

---

## 7. GO / NO-GO ASSESSMENT (proceed? conditions)

### Recommendation: **GO, with conditions.**
The core direction (Tauri + Rust core + capability-based APIs + sandboxed apps + secure bridge) is solid. But you should not start broad implementation until you satisfy these gating conditions:

#### Gating conditions (do these first as spikes / proofs)
1. **Prove untrusted content cannot access privileged Tauri APIs**
   - Create a “malicious test app” and verify it cannot:
     - invoke arbitrary commands
     - call plugins (shell/file/dialog/http)
     - navigate externally
     - exfiltrate via window.open/download/etc.
   - Automate this as an E2E security regression test.

2. **Choose and validate the sandbox model**
   - Decide: multi-webview for third-party apps (recommended).
   - Benchmark memory on Windows/macOS/Linux with 1/3/5/10 apps.
   - Implement LRU unloading policy early if multi-webview.

3. **Unify the protocol + naming + CSP rules into one canonical spec**
   - One set of method names, one session command name.
   - One canonical “how CSP is enforced”.
   - One canonical “how permissions are persisted and checked”.

If you meet those, implementation is viable and you can confidently proceed to a vertical slice:
- install app → launch → handshake → `storage.get/set` → permission prompt → audit log → update app.

If you want, I can propose a “Phase 0” spike plan (1–2 weeks) with exact experiments and acceptance criteria to de-risk the above before you commit to full platform build-out.
