## 1) Implementation-ready with sufficient detail?

**Mostly “architecture-ready”, not “implementation-ready” yet.** The spec is strong on principles, boundaries, and key contracts (slices, events, commands, CSP intent), but several areas need concrete, buildable details to avoid teams filling gaps inconsistently.

### What’s implementation-ready
- Clear **ownership boundaries**: Rust authoritative vs shell UI-only.
- A workable **state model** (Zustand slices + Rust→event sync).
- A clear **component hierarchy** and major UI regions.
- A solid start on **command naming**, **event naming**, and **data models**.
- Explicit security stances (no `dangerouslySetInnerHTML`, no SVG from manifests, shell-only commands verify label).

### Gaps that will block/slow implementation
1) **Missing referenced ADRs**
   - Spec references ADR-003..ADR-008 but only ADR-001/002 exist. Those missing ADRs are exactly where key implementation decisions belong (navigation model, windowing paradigm, privilege boundary placement, theming contract, shortcuts, manifest rendering policy).
   - Without them, teams will diverge on routing, window focus rules, theming variables, and permission UX.

2) **Multi-webview integration is underspecified**
   - The hierarchy has an `{/* Multi-webview integration point */}`, but no concrete plan for:
     - How Tauri windows/webviews are created (e.g., `WebviewWindow` per app vs multiple webviews in one window).
     - Labeling strategy (`webviewLabel` format, uniqueness, lifecycle).
     - Focus plumbing between shell and app surfaces (what constitutes “focusedWindowId”? how to detect webview focus?).
     - Z-order and layout: are “windows” real OS windows, internal tiles, or surfaces in a single OS window?

3) **Rust↔TS type contract and versioning**
   - Payload types like `AppsPayload`, `WindowsPayload`, `ConnectivityPayload` are referenced but not defined.
   - Several TS fields use `Date` (`lastSync: Date | null`) while Rust events will almost certainly serialize timestamps as strings/numbers. You need a canonical wire format (ISO string recommended) and parsing rules.
   - Add an explicit “API schema version” for events/commands to support migrations.

4) **Settings persistence: where and how**
   - The TS interface is good, but implementation details are missing:
     - Is persistence done in Rust (recommended) or in the shell webview?
     - Merge semantics for `ShellSettingsPatch` (deep merge? partial paths? replace arrays?).
     - Concurrency: what happens if two updates occur quickly (debounce, last-write-wins, revision numbers)?

5) **Zustand store details that matter in production**
   - Using `Map`/`Set` is fine, but you must define update patterns to ensure React subscriptions re-render correctly (immutable replacement vs in-place mutation).
   - Define selectors and subscription granularity conventions (e.g., never select whole slices in components).
   - Devtools/logging middleware guidance is missing (and interacts with the “no secrets” requirement).

6) **Error containment / safe mode details**
   - “3 crashes in 5 minutes” needs:
     - How crashes are detected (shell webview crash vs JS error boundary vs app webview crash).
     - Where the crash counter is stored (Rust-side is more tamper-resistant than `localStorage`).
     - What “disable all third-party apps” means operationally (block auto-launch? hide? prevent creation?).

---

## 2) Are security measures comprehensive?

**Good foundations, but several important hardening items are missing or need tightening.** You correctly treat shell XSS as catastrophic and emphasize Rust authority + shell-only commands. The remaining gaps are mostly around *ensuring app webviews cannot reach privileged APIs* and around *navigation / origin control*.

### Strong points
- Runtime **webview label verification** in commands (critical).
- Tauri **capabilities** concept included (good).
- CSP intent prohibits eval, frames, objects; disallows base-uri; good.
- Clear manifest rendering policy (text only, SVG banned, size caps).
- Devtools disabled in production.

### Key gaps / tighten-ups
1) **Guarantee app webviews cannot access Tauri APIs**
   - Acceptance criteria says: “App webviews cannot access `__TAURI__` APIs” but spec doesn’t define *how*.
   - In Tauri v2, capabilities restrict `invoke`, but you should also specify:
     - App webviews run with **no capabilities** (or a strictly minimal capability set) by default.
     - App webviews should not be able to call shell-only commands even if they guess names (capabilities + runtime label check).
     - Whether `withGlobalTauri` / global injection is disabled for app webviews (implementation detail must be explicit).

2) **Navigation / origin control is missing**
   - You need a policy for:
     - Preventing shell webview navigation to arbitrary URLs (links should be intercepted and routed through `shell_open_external_url`).
     - Preventing app webviews from navigating to local `tauri://` or `asset://` paths they shouldn’t access.
   - Add explicit navigation allowlists (hostnames/schemes) per webview type.

3) **CSP: `'unsafe-inline'` for styles**
   - The spec allows `'unsafe-inline'` due to Tailwind. Tailwind itself doesn’t require inline styles; it uses classes. If you only need inline styles for a small set, prefer:
     - Remove `'unsafe-inline'` if possible, or
     - Use nonces/hashes (if feasible in your Tauri setup), or
     - Constrain inline styles to known safe patterns (hard to enforce reliably).
   - At minimum, document **why** inline styles are needed and how you’ll lint for `style={{...}}` usage.

4) **Event spoofing / trust model**
   - Shell listens to events like `shell://permissions/prompt`. Document who can emit them (Rust only) and ensure app webviews cannot emit events that the shell trusts.
   - In practice: avoid trusting events originating from any webview; only Rust backend should emit shell-trusted events.

5) **Manifest and asset validation should be Rust-side**
   - You have policy statements, but not the enforcement location. Make explicit:
     - Manifest parsing/validation happens in Rust.
     - Icon decoding/size/type validation happens in Rust.
     - Shell receives only sanitized, bounded data (e.g., already truncated strings).

6) **Security testing hooks**
   - Add implementation requirements for CI:
     - Grep/lint ban for `dangerouslySetInnerHTML`.
     - Dependency audit (npm + Rust crates).
     - CSP regression test (ensure CSP header/behavior is actually applied on all platforms).

---

## 3) Are accessibility requirements testable?

**Partially testable; needs concrete, verifiable acceptance tests and ARIA contracts.** The keyboard shortcut list is good, but the spec should define *exact focus order*, *landmarks*, and *automation strategy*.

### What’s good
- Lists required shortcuts (Tab, Esc, Cmd/Ctrl+K, F6).
- Mentions focus trap + restore, visible focus, reduced motion.
- Notes live regions for toasts.

### Gaps to make it truly testable
1) **Define landmark/region semantics**
   - “All regions labeled with aria-label” is vague. Specify:
     - Use `role="navigation"` for sidebar, `role="main"` for main area, `role="banner"` for title/top bars as appropriate.
     - Define the accessible names (“Sidebar”, “Top bar”, “Workspace”, etc.).

2) **App surface `role="application"` is risky**
   - `role="application"` can degrade screen reader behavior if overused. If you keep it, define when it applies and how users escape back to shell controls.
   - Consider `role="region"` + clear labeling unless you have a strong reason.

3) **Automation plan**
   - Add explicit tooling requirements:
     - Playwright for keyboard traversal tests.
     - axe-core scans for WCAG regressions (with known exception handling).
   - Add concrete test cases like:
     - “Press F6 cycles focus in order: Sidebar → TopBar → Workspace → Overlays (if open) → Sidebar…”
     - “Opening PermissionPromptModal moves focus to first actionable control; Esc closes; focus returns to previously focused element.”

4) **Contrast and motion requirements**
   - “High contrast” needs measurable criteria (e.g., minimum contrast ratios, token constraints).
   - Reduced motion should specify what’s disabled (transitions > X ms, parallax, etc.).

---

## 4) Missing platform-specific concerns?

**Yes—several Tauri/WebView platform quirks and titlebar/drag behaviors need explicit decisions.**

1) **Title bar / decorations mismatch**
   - Component hierarchy suggests a custom `TitleBar` with drag regions and window controls, but `tauri.conf.json` shows `"decorations": true`.
   - Decide per platform:
     - If using native decorations, don’t build custom window controls.
     - If using custom titlebar, define decorations false + drag region implementation details (Tauri uses `data-tauri-drag-region` patterns).

2) **CSP consistency across WebView2 / WKWebView / WebKitGTK**
   - CSP enforcement and edge cases differ. You should include:
     - A platform validation checklist (verify CSP actually blocks inline scripts, remote scripts, etc. on each engine).
     - Known limitations and compensating controls.

3) **Global shortcuts and reserved keys**
   - Cmd+K / F6 behavior can conflict with OS/app defaults on macOS/Linux.
   - Define precedence and whether shortcuts are global or only when shell is focused.

4) **Window focus + IME/input behavior**
   - For multi-webview, IME (CJK input), clipboard, and focus transitions can be flaky across platforms.
   - Add a platform QA matrix for: focus switching, copy/paste, drag-select, text input in app webviews vs shell inputs.

5) **Filesystem paths / storage locations**
   - Settings, logs, diagnostics export paths vary by OS; specify Rust-side locations and permissions.

---

## 5) Implementation readiness rating (1–10)

**6/10.**

- **Why not higher:** missing ADRs for core behavior, underspecified multi-webview/windowing model, incomplete type/payload contracts, and several security/a11y items need concrete enforcement/testing plans.
- **Why not lower:** strong architectural direction, good security instincts, and clear boundaries/command patterns provide a solid base.

---

## High-priority “make it buildable” checklist (suggested)
1) Add ADR-003..ADR-008 (or inline the missing decisions) covering:
   - navigation/routing, windowing, privilege boundary, theming token contract, shortcuts system, manifest/asset enforcement.
2) Define the **webview model** precisely (per-app window vs embedded webviews; label scheme; focus + z-order rules).
3) Formalize **Rust↔TS schemas** for every payload and include versioning + timestamp formats.
4) Specify **app webview capability set = none/minimal** and navigation allowlists.
5) Add an **a11y test plan** (Playwright + axe + manual SR steps) with explicit focus order contracts.
6) Resolve titlebar strategy per platform (decorations vs custom) and document drag region/window control implementation.

If you want, I can propose a concrete “Webview & Windowing Contract” section (labels, lifecycle, events, focus rules) that plugs directly into this spec.
