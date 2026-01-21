# ADR-003: Multi-webview Architecture Decision

## Status
Accepted

## Context
Post-Urbit needs to run untrusted third-party apps with strong isolation from each other and the shell. The key architectural question is: **iframes vs multi-webview**.

Key requirements:
- Apps are untrusted by default
- Must prevent renderer-level attacks
- Must prevent cross-app data access
- Should contain crashes
- Must work on Windows, macOS, and Linux

## Decision
Use **multi-webview for ALL untrusted/third-party apps**. Iframes are only acceptable for trusted same-team micro-UI components within the shell.

### Implementation
- Each app runs in a separate webview with label `app-{app_id}`
- All app content served via `postapp://{app_id}/` custom protocol
- Apps have minimal Tauri capabilities (bridge command only)
- Shell manages webview lifecycle with LRU unloading

## Options Considered

### Option 1: Single Webview + Iframes (REJECTED)
**Description**: One webview for shell, apps in sandboxed iframes.

**Pros:**
- Simple implementation
- Instant app switching
- Lower memory footprint
- Familiar web model

**Cons:**
- Shared renderer process - renderer exploit = game over
- CSP-based isolation is weak boundary
- App crash = shell crash
- Complex to achieve true API isolation

### Option 2: Multi-webview (CHOSEN)
**Description**: Separate webview per app, managed by shell.

**Pros:**
- Real OS-level process isolation on all platforms
- Native capability enforcement via Tauri
- Crash containment (app crash doesn't affect others)
- Strong security boundary

**Cons:**
- Higher memory (~50-350MB per webview)
- Creation latency (500ms-5s)
- Requires LRU management for many apps
- More complex webview lifecycle

## Consequences

### Security Implications
- Full process isolation prevents renderer-level attacks
- Memory corruption in one app cannot affect others
- Capability files provide definitive API boundaries
- Webview label becomes trusted source of app identity

### Performance Implications
- Memory overhead requires capping concurrent hot webviews (3-5)
- LRU policy moves inactive apps to warm/cold state
- App switching has slight delay for cold apps
- Platform differences in memory/latency must be documented

### Developer Experience
- No change for app developers
- SDK abstracts the isolation layer
- Apps may notice slight delay on first launch

## Resource Management Strategy
- **Hot**: Actively visible, rendered (max 3-5)
- **Warm**: Hidden but in memory (max 5)
- **Cold**: Webview destroyed, state persisted

## Rollback Plan
If multi-webview proves unviable on any platform:
1. Evaluate iframe-based isolation for "marketplace-approved" apps only
2. Treat iframes as "semi-trusted" with weaker security guarantees
3. Keep multi-webview for apps requiring strong isolation

## Related
- [Domain 2: App Sandbox & Isolation](../specs/02-APP_SANDBOX_ISOLATION.md)
- [Tauri Multi-webview Research](../TAURI_MULTIWEBVIEW_RESEARCH.md)
- [Phase 0: Gating Spikes](../specs/PHASE_0_GATING_SPIKES.md)
