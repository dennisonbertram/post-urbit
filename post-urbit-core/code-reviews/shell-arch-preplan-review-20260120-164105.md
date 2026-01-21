## 1) Key decisions that need ADRs (Domain 1–specific)

The planning doc lists ADR-001 (state mgmt) and ADR-002 (component library constraints). Those are necessary but not sufficient for a *shell* that is the privileged UI surface. I’d add these ADRs explicitly because they affect security, maintainability, and feasibility:

### ADR: Shell rendering + navigation model
- **Decision:** `react-router` vs `@tanstack/router` vs “no URL routing” (state-based).
- **Why it matters:** deep-linking, back/forward behavior, multi-pane layouts, and settings/app pages.
- **Implementation note:** If apps are hosted in separate webviews, “route” changes often become “focus window/pane X” instead of changing URL paths.

### ADR: Windowing / layout paradigm
- **Decision:** single-window with panes vs tabs vs multiple OS windows (and whether “multi-webview in one OS window” is a hard requirement).
- **Why it matters:** Tauri multi-webview-in-one-window feasibility varies; it impacts focus, accessibility, and memory/resource constraints.
- **Deliverable:** layout constraints (min sizes), z-order rules, drag/resize model, keyboard focus traversal rules.

### ADR: Privilege boundary placement (shell JS vs Rust)
- **Decision:** Does the shell webview JS directly broker app messages (like `window.addEventListener('message')` in `FRONTEND_ARCHITECTURE.md`) or is *all* app↔platform IPC terminated in Rust?
- **Why it matters:** Any XSS in the shell becomes a full platform compromise if shell JS can call `invoke()` on behalf of apps.
- **Recommendation:** Treat shell webview as *UI only*; terminate and validate app IPC in Rust wherever possible.

### ADR: Theming contract (tokens + constraints)
- **Decision:** CSS variables design-token system vs Tailwind theme switching only; what parts apps can inherit/override.
- **Why it matters:** shell/app visual coherence, accessibility contrast guarantees, and avoiding “theme injection” vectors.
- **Deliverable:** token list + dark/light/high-contrast + reduced-motion handling.

### ADR: Command system & global shortcuts
- **Decision:** command palette architecture + shortcut precedence (shell vs app).
- **Why it matters:** focus handling across multiple app surfaces/webviews and preventing apps from spoofing system UI via keybindings.

### ADR: Asset and manifest rendering policy
- **Decision:** what formats are allowed for app icons (PNG only? SVG banned?) and how manifest fields are sanitized.
- **Why it matters:** SVG/script hazards, HTML injection via names/descriptions, “lookalike” spoofing.

---

## 2) Critical security considerations for the shell (implementation-focused)

The shell is the **most privileged** renderer. Domain 1’s security invariants are good, but you’ll want stricter, testable rules.

### A. Lock down Tauri command surface to *shell only*
In Rust, ensure every command checks the caller **webview label** (or equivalent identifier) and rejects if not the system shell.

- **Rule:** “Privileged commands callable only from shell webview label = `shell`.”
- **Why:** If an app webview can ever access `invoke()`, it’s game over.

### B. Don’t implement the “bridge” in shell JS (or minimize it)
`FRONTEND_ARCHITECTURE.md` shows the shell listening to `message` events and calling `invoke()` based on app-provided `type`. That creates two big problems:
1) **Confused deputy:** app tells shell which privileged operation to perform.
2) **XSS amplification:** any shell XSS can mint privileged calls.

**Recommendation:** terminate app IPC in Rust (Domain 3), not in React. If you temporarily must do it in JS for a spike:
- require strict `event.origin` checking (`postapp://{appId}`),
- require strict `event.source === iframe.contentWindow`,
- validate messages against a schema (zod/io-ts),
- and **never** use `postMessage(..., '*')` for responses (use the specific origin).

### C. Shell CSP hardening (realistic for Tauri)
Shell CSP should be “paranoid by default”:
- `default-src 'self';`
- `script-src 'self';` (no inline; no eval)
- `style-src 'self';` (avoid `'unsafe-inline'` in shell; if Tailwind inline styles appear, fix the pipeline)
- `img-src 'self' data: blob:;`
- `connect-src 'self' http://127.0.0.1:*` only if you truly need local dev connections; otherwise none
- `base-uri 'none'; object-src 'none'; frame-ancestors 'none';`

Also: in the architecture doc, an `iframe csp="..."` attribute is shown—**that is not CSP enforcement** in browsers. CSP must be delivered via **HTTP header** or `<meta http-equiv="Content-Security-Policy" ...>`. Your Domain 0.2 spike (“CSP via custom protocol”) is the right approach.

### D. Treat all app-provided metadata as untrusted
- Manifest `name`, `description`, “reasons”, etc. must be rendered as plain text.
- Icons:
  - Prefer PNG/WebP.
  - If you allow SVG, sanitize it server-side (Rust) and serve with restrictive headers; otherwise ban it to reduce risk.

### E. Navigation / external URL policy in shell UI
Even if apps are sandboxed, the shell itself will have links (settings/help).
- Centralize “open external URL” behind a single Rust command that:
  - allowlists schemes (`https:` maybe `mailto:` only via prompt),
  - blocks file/unknown schemes,
  - logs/audits if needed.

### F. Disable devtools & remote debugging in production builds
This is often overlooked; devtools can expose internals, storage, or messaging.
- Provide a build-time flag: devtools enabled only in dev/nightly builds.

### G. Clipboard, drag/drop, and OS integrations must be shell-mediated
If the shell supports clipboard or drag/drop features, ensure app surfaces cannot trigger them without going through permission checks (Domain 4), ideally in Rust.

---

## 3) Component hierarchy recommendations (practical + scalable)

Keep the shell’s React tree focused on **system UI**, not app logic. Suggested top-level hierarchy:

```
<AppRoot>
  <ErrorBoundaryRoot>
    <A11yProviders>               // focus-visible, reduced motion, high-contrast
      <ThemeProvider>             // CSS variables / tokens
        <QueryClientProvider?>    // if you use TanStack Query for shell-only data
          <ShellLayout>
            <TitleBar />          // optional custom titlebar
            <Sidebar>
              <AppLauncherButton />
              <NavItems />        // Home, Apps, Settings
              <RunningAppsList /> // windows/sessions
            </Sidebar>

            <MainArea>
              <TopBar>
                <BreadcrumbsOrTabs />
                <GlobalSearchOrCommandPaletteTrigger />
                <StatusIndicators />  // sync, connectivity, identity
              </TopBar>

              <Workspace>
                <WindowManager>   // renders “frames” and manages focus/z-order
                  <AppSurfaceSlot windowId=... />  // placeholder for webview/iframe
                </WindowManager>
              </Workspace>
            </MainArea>

            <SystemOverlays />    // portal root
              <NotificationCenter />
              <Toasts />
              <PermissionPromptModal />
              <ConfirmDialog />
              <AppInstallFlowModal />
              <DiagnosticsModal />
          </ShellLayout>
        </QueryClientProvider?>
      </ThemeProvider>
    </A11yProviders>
  </ErrorBoundaryRoot>
</AppRoot>
```

Key implementation guidance:
- **One portal root** for all overlays to avoid stacking-context bugs (Radix/shadcn friendly).
- **WindowManager is the integration seam**: whether you use iframes or multi-webview, everything routes through this layer.
- **SystemOverlays must never render app-controlled HTML.** Only render sanitized plain text.

---

## 4) State management approach for a multi-webview app

Zustand is a fine choice, but the critical part is *where the source of truth lives*.

### Recommended split: Rust = authoritative runtime state, React/Zustand = UI state
- **Rust owns**: app lifecycle (installed/running), webview creation/destruction, permissions state, bridge sessions/tokens, resource usage.
- **Shell store owns**: layout, focused window, sidebar state, UI preferences, transient prompts.

Reason: if you ever reload the shell webview, you don’t want to lose authoritative “what is running” state. Also reduces the blast radius of a shell bug.

### Zustand store structure (slice model)
Use slices to keep concerns isolated:

- `uiSlice`: theme, sidebar collapsed, command palette open, reduced motion.
- `windowsSlice`: window list, focus/z-order, layout geometry.
- `appsSlice`: installed apps list + running sessions (mirrors Rust).
- `permissionsSlice`: pending prompts queue + last decision (UI only).
- `notificationsSlice`: toasts + notification center items.
- `connectivitySlice`: sync status, online/offline.

Use `subscribeWithSelector` to avoid rerender storms when many windows exist.

### Event-driven syncing from Rust → shell
Have Rust emit events like:
- `shell://apps/installed_changed`
- `shell://apps/running_changed`
- `shell://windows/metrics_changed`
- `shell://permissions/prompt`

Shell listens once at startup and updates Zustand.

### Multi-webview focus + keyboard routing
Define a clear policy:
- Shell owns global shortcuts (Cmd+K, Cmd+, etc.)
- When an app webview is focused, certain keys still belong to shell (Esc to exit full-screen, etc.)
- This needs a dedicated “input routing” module; don’t scatter key handlers across components.

---

## 5) Missing concerns / gaps in Domain 1 (worth adding)

### A. Shell ↔ app boundary clarity (currently mixed in FRONTEND_ARCHITECTURE.md)
The existing doc shows the shell brokering app messages and calling `invoke()`. Domain 1 should explicitly state:
- whether shell JS is allowed to translate app requests to privileged operations, and
- if yes, what mitigations prevent confused-deputy and XSS escalation.

### B. Focus management & accessibility across embedded app surfaces
Domain 1 mentions accessibility broadly, but the hard parts are:
- focus traversal between sidebar/topbar and app surfaces,
- screen reader labeling of “App area” vs “System UI,”
- keyboard access to permission prompts even when app surface is focused,
- visible focus rings not lost when switching webviews.

Add explicit acceptance criteria like:
- “User can open permission prompt and operate it fully via keyboard while an app surface has focus.”

### C. Error containment strategy
Define:
- shell ErrorBoundaries for React,
- app-surface failure modes (webview crash / iframe load error),
- “recover / reload app” UX,
- safe-mode boot (disable third-party apps) if shell repeatedly crashes.

### D. Shell settings persistence & migration
Where do you store:
- theme selection,
- window layout restore,
- last opened apps,
- feature flags?
Define a schema + migration approach (even if minimal).

### E. Versioning and compatibility with protocol registry
Even though the registry is Domain 3.5, the shell depends on it:
- shell UI should display meaningful errors when protocol version negotiation fails.
- include “platform version” display + diagnostics export hooks.

### F. Build-time hardening and supply-chain controls
Domain 1 should include:
- dependency policy (pinning/lockfile, audit),
- disabling remote fonts/CDNs,
- bundler config constraints (no dynamic `import()` from URL).

### G. `tauri.conf.json` and plugin allowlist specifics
Domain 1 says “`tauri.conf.json` specification” but doesn’t list what must be decided:
- which plugins are enabled at all,
- which windows/webviews can access which APIs,
- navigation allowlist rules for the shell webview itself.

---

### Bottom line recommendations
1) Add ADRs for window/layout model, routing, privilege boundary placement, theming contract, shortcuts, and manifest/icon rendering policy.
2) Move “bridge termination” out of shell JS and into Rust as soon as possible; never use `postMessage('*')` in production.
3) Make `WindowManager` the seam: it should work whether the app surface is an iframe (single webview) or a separate webview.
4) Use Zustand slices for shell UI state, but keep authoritative lifecycle/permission/session state in Rust and sync via events.
5) Expand Domain 1 to explicitly cover focus/a11y across app surfaces, crash recovery, settings persistence, and Tauri plugin allowlisting.

If you want, I can propose a concrete “Domain 1 spec skeleton” (data models + event names + Rust trait surfaces + TS types) that matches your Definition of Done checklist.
