## Rating: **7/10** (strong foundation, but a few critical “gotchas” to resolve)

The specs are directionally solid (multi-webview + protocol + capability minimization + label-derived identity). The main risks are (a) **whether `postapp://{app_id}` actually yields strong per-app origin separation on all platforms**, and (b) **whether Tauri capabilities apply per-*webview* the way the spec assumes** (vs per-window or “default capability leakage” for runtime-created webviews). Those two items determine whether the isolation story is truly robust or mostly “best effort + Rust label checks”.

Below is a structured review against your questions, with concrete improvements.

---

# 1) Is multi-webview isolation properly specified?

### What’s good
- Clear decision: **no iframes for untrusted apps**.
- App identity tied to infrastructure (`app-{app_id}` label) and enforced in Rust.
- Lifecycle concepts (hot/warm/cold) and LRU resource management are specified.
- Navigation/popup blocking in Rust is the right layer (renderer-side controls are bypassable).

### Key gaps / risks
1) **“OS-level process isolation” is overstated / underspecified.**  
   - WebView2/WKWebView/WebKitGTK are multi-process, but the degree of isolation depends on engine policy (process-per-site-instance, shared process pools, etc.). You should specify what you rely on (e.g., “separate renderer process per webview where supported”), and what you *don’t* assume.

2) **Storage/process pool isolation is not specified.**  
   If all app webviews share the same engine “profile” / data store:
   - origin separation becomes absolutely critical (see §3),
   - and any custom-scheme-origin quirks become catastrophic (cross-app localStorage/IndexedDB/cookies).

3) **Multi-webview inside a single native window may interact with the capability model (see §5).**  
   If Tauri scopes permissions at the window level (or defaults), app webviews could unintentionally inherit shell privileges.

### Actionable improvements
- Add a dedicated section: **“Webview engine configuration per app”**:
  - For WebView2: clarify whether you use one Environment or per-app Environment (per-app is stronger but heavier).
  - For WKWebView: clarify whether you use separate `WKProcessPool` / `WKWebsiteDataStore` per app (stronger; may impact memory).
- Add explicit requirement + test: **each app must have a unique origin** and must not share storage.

---

# 2) Are CSP headers comprehensive enough?

### What’s good
- `default-src 'none'` baseline is excellent.
- No external network by default (intended).
- `frame-ancestors 'none'`, `object-src 'none'`, `base-uri 'none'` are correct.
- Adds important security headers (`nosniff`, `no-referrer`, COOP/CORP).

### CSP issues / likely breakage / future footguns
1) **`connect-src 'none'` is very strict and may break common “local-only” apps.**  
   Many apps fetch same-origin JSON/config/assets via `fetch()`/XHR even if they never touch the network. With `connect-src 'none'`, *even same-origin fetch is blocked*.

   **Recommendation:** change to:
   - `connect-src 'self'` (still blocks external), and optionally explicitly include your scheme host if needed.

2) **Missing directives that commonly matter in modern apps:**
   - `worker-src` (many frameworks use workers; without it you’ll get pressure to weaken CSP)
   - `manifest-src` (if you allow PWA-style manifests)
   - `frame-src` / `child-src` explicit deny (defense-in-depth; though `default-src 'none'` mostly covers)
   - `script-src-attr 'none'` and `style-src-attr ...` (more explicit hardening)
   - `require-trusted-types-for 'script'` (best for shell; may be too strict for apps, but consider for shell at least)

3) **`script-src 'self' 'wasm-unsafe-eval'`**  
   This is sometimes necessary, but it increases exploit surface (WASM JIT paths). Consider making WASM opt-in by permission tier:
   - default: no `wasm-unsafe-eval`
   - allow per-app if they request “WASM runtime” capability

4) **COOP without COEP**  
   You set COOP, but not `Cross-Origin-Embedder-Policy`. That’s fine if you don’t need crossOriginIsolated, but document the intent. If you ever want SharedArrayBuffer, you’ll need COEP + careful resource policy.

### Actionable improvements (CSP)
- Change baseline to (example):
  ```
  default-src 'none';
  base-uri 'none';
  form-action 'none';
  frame-ancestors 'none';
  object-src 'none';
  script-src 'self';
  script-src-attr 'none';
  style-src 'self' 'unsafe-inline';
  img-src 'self' data: blob:;
  font-src 'self';
  media-src 'self' blob:;
  connect-src 'self';
  worker-src 'self' blob:;
  ```
- Add **CSP violation reporting** (even if only in dev/nightly) via `report-to` / `report-uri`, routed to Rust logs for auditing.

---

# 3) Is identity enforcement from webview label bulletproof?

### What’s good
- Correct principle: **identity from infrastructure**, never payload.
- `postbridge_invoke` derives `app_id` from `webview.label()` and cross-checks session `app_id`.
- Shell-only commands verify `webview.label() == "shell"`.

### Where it’s not “bulletproof” yet
1) **Webview label format constraints are not addressed.**  
   You plan `label = "app-{app_id}"` where `app_id` is reverse-DNS with dots. Tauri label constraints can be stricter than URLs (platform-dependent). If `.` or length or Unicode causes normalization issues, you risk:
   - inability to create webviews
   - inconsistent parsing
   - edge-case label collisions

   **Fix:** define a “safe label encoding”, e.g.:
   - `app-{base32(sha256(app_id))}` or
   - `app-{app_id.replace('.', '_')}` with a canonical mapping stored in Rust.

2) **Label-only checks are necessary but not sufficient as a *security boundary* unless you guarantee app webviews cannot create other privileged webviews.**  
   You mostly cover this via popup blocking + capability minimization, but you should explicitly add:
   - Apps must not have any API to create new windows/webviews.
   - Devtools disabled in prod (already stated).

3) **DoS and parser abuse against `postbridge_invoke`**
   - No maximum size for `request_bytes`
   - CBOR decode could allocate heavily

### Actionable improvements (identity + bridge hardening)
- Validate derived `app_id` (or its decoded form) against the same `is_valid_app_id()` regex before using it.
- Enforce strict limits:
  - `request_bytes.len() <= MAX_BRIDGE_MSG` (e.g., 256KB or 1MB)
  - rate limit per webview/session
- Tie sessions to **(webview label + webview internal id)** in Rust state (not just label string), so even if labels ever collide, you fail closed.

---

# 4) Are all attack vectors addressed (renderer exploits, IPC escape, navigation, popups)?

### Covered well
- Renderer exploits: mitigated by process separation and crash containment (good).
- IPC escape: capabilities + label checks + single bridge command is a solid model.
- Navigation + popups: blocking at Rust layer is correct.

### Important missing/under-specified vectors
1) **Cross-app origin/storage isolation is the biggest missing “attack vector class”.**  
   Your whole model assumes:
   - `postapp://appA/` and `postapp://appB/` are different origins everywhere,
   - CSP `'self'` behaves consistently for custom schemes,
   - localStorage/IndexedDB partition correctly.

   If any platform treats `postapp://*` as an opaque origin shared across hosts, you get:
   - cross-app localStorage/IndexedDB access
   - cross-app cache/cookie weirdness
   - cross-app CSRF-like interactions

   **You must explicitly specify and test origin semantics per platform.**

2) **Subresource requests vs navigation hooks**
   - `on_navigation` controls top-level navigations, not necessarily subresource loads.
   - CSP must be the primary control for cross-app subresource loading. That makes “origin correctness” critical again.

3) **Clipboard, file picker, downloads, printing, drag/drop**
   The policy matrix claims “clipboard read blocked by Permissions-Policy” but:
   - `Permissions-Policy` support varies and doesn’t reliably govern `navigator.clipboard` across engines.
   - WebViews may allow download prompts or file pickers unless explicitly disabled/intercepted.

4) **Service workers / persistent execution**
   If allowed, they can create persistence inside the app origin. Might be acceptable, but should be explicitly decided:
   - allowed (and how to clear on uninstall)
   - or disabled (if possible)

### Actionable improvements (attack surface hardening)
- Add platform-specific webview hardening checklist:
  - Disable/limit downloads (intercept download events if available)
  - Disable autofill/password manager integrations (WebView2 settings)
  - Disable or restrict file access, `file://` navigation (already mostly blocked)
  - Disable service workers if you can, or document lifecycle + clearing rules
- Add explicit **cross-app origin isolation tests**:
  - App A attempts to read `localStorage` keys set by App B (must fail)
  - App A tries `<img src="postapp://appB/...">` (must violate CSP)
  - App A tries `fetch("postapp://appB/...")` (must fail by CSP / SOP)

---

# 5) Is the Tauri capability configuration correct?

### What’s good
- Principle: apps get only the bridge permission; shell gets full permissions.
- Avoids `core:default` in app capabilities (good, assuming it behaves as intended).

### High-risk ambiguities / likely incorrect assumptions
1) **Wildcard window labels (`"app-*"`): verify support.**  
   Tauri capability `windows` matching may not support globbing the way the spec assumes. If wildcards don’t work, you risk:
   - app webviews getting *no* capability (breaking bridge), or
   - inheriting some default capability you didn’t intend.

2) **Are capabilities scoped per window or per webview?**  
   If you run multiple webviews inside the single `"shell"` window, and capabilities are window-scoped, then all app webviews may inherit `"shell"` capabilities.

   Even if your Rust label checks prevent command execution, you lose defense-in-depth and may unintentionally enable plugin APIs/events.

3) **Permission identifiers like `postbridge:allow-shell-*`**
   Tauri permissions are typically exact identifiers, not wildcards (unless you implement wildcard handling in your plugin permission system, which is nonstandard). This reads like a spec placeholder but should be made concrete.

### Actionable improvements (capabilities)
- Replace wildcard assumptions with explicit strategy:
  - If Tauri supports matching on webview labels: document the exact mechanism and test it.
  - If it does not: enforce in Rust **and** consider per-webview registration with restricted IPC (if available).
- Make permissions explicit, no `*`:
  - `postbridge:allow-invoke`
  - `postbridge:allow-app-launch`
  - etc.
- Add a “capability verification test harness” (Spike-level):
  - From app webview, enumerate and attempt to call *every* registered command/plugin API; assert only bridge works.

---

# 6) Gaps between shell architecture and sandbox specs

### Notable inconsistencies
1) **Command naming mismatch**
   - Shell spec: `shell_launch_app`, `shell_close_app`, etc.
   - Sandbox spec: `app_launch`, `app_show`, etc.
   This is minor but will cause implementation drift and security review mistakes.

2) **Capability file naming mismatch**
   - Shell spec uses `capabilities/shell-only.json`
   - Sandbox spec uses `capabilities/shell.json`
   Align naming and identifiers to avoid accidentally shipping the wrong set.

3) **“Apps cannot access `__TAURI__` APIs” vs “apps can call `postbridge_invoke`”**
   If apps truly cannot access `__TAURI__`/`invoke`, how do they call the bridge?
   - If the plan is to ship a minimal injected JS bridge API (recommended), specify it.
   - If the plan is to allow Tauri invoke but only for one command, clarify that “apps can invoke, but only bridge is permitted”.

### Actionable improvements (spec alignment)
- Normalize command names and capability IDs across both documents.
- Add an explicit section: **“App JS bridge surface”**
  - what global is exposed (if any),
  - how it maps to `postbridge_invoke`,
  - how you prevent access to other Tauri APIs (capabilities + packaging + no `@tauri-apps/api` for apps, etc.).

---

# Priority actionable improvements (short list)

## P0 (must fix before claiming “strong isolation”)
- **Specify and test per-app origin isolation for `postapp://{app_id}` on Win/macOS/Linux.**  
  If uncertain, consider switching to a “virtual HTTPS host per app” model (e.g. `https://{app_id}.postapp.local/`) using engine-supported host mapping, because HTTPS origins are well-defined.
- **Confirm capability scoping for runtime-created webviews** (per-webview vs per-window) and remove wildcard assumptions unless verified.
- Add bridge hardening: **size limits, rate limits, strict label/app_id validation**.

## P1 (defense-in-depth & preventing future CSP weakening)
- Adjust CSP `connect-src` to `'self'` (and add `worker-src`).
- Decide WASM policy (default deny vs opt-in).
- Add CSP violation reporting in dev/nightly.

## P2 (hardening & UX security)
- Explicitly handle/disable downloads, file pickers, clipboard expectations, service worker policy.
- Document platform-specific webview settings hardening knobs (WebView2/WKWebView/WebKitGTK).

---

If you want, I can propose a concrete “origin strategy” that is cross-platform predictable (including WebView2 virtual host mapping details, WKWebView scheme handler nuances, and how to keep CSP and storage partitioning consistent), plus a Spike test plan that definitively proves cross-app storage isolation.
