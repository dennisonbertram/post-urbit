# Post-Urbit Frontend Risk Register

## Purpose
Track identified risks, their mitigations, and retirement criteria throughout the planning and implementation process.

---

## Risk Severity Matrix

| Likelihood / Impact | Low | Medium | High |
|---------------------|-----|--------|------|
| **High** | Medium | High | Critical |
| **Medium** | Low | Medium | High |
| **Low** | Low | Low | Medium |

---

## Active Risks

### RISK-001: Tauri IPC Escape (Sandbox Bypass)
**Severity:** Critical
**Status:** Open

**Description:** Untrusted app content could potentially access `__TAURI__` APIs or invoke privileged commands, completely bypassing the sandbox model.

**Owner:** TBD

**Mitigation Plan:**
1. Gating Spike 0.1 must prove containment
2. Apps served via `postapp://` protocol without Tauri API injection
3. All privileged commands require shell webview label validation
4. E2E malicious app test suite

**Retired When:**
- [ ] Spike 0.1 passes on all 3 platforms
- [ ] Malicious test app cannot invoke any Tauri APIs
- [ ] E2E security test in CI

---

### RISK-002: Multi-Webview Memory Explosion
**Severity:** High
**Status:** Open

**Description:** With multi-webview architecture, 5-10 concurrent apps could consume multiple GB of RAM, degrading system performance.

**Owner:** TBD

**Mitigation Plan:**
1. Gating Spike 0.3 establishes baseline numbers
2. LRU unloading policy specification (Domain 2.5)
3. Memory monitoring and thresholds
4. User-visible indicators when approaching limits

**Retired When:**
- [ ] Spike 0.3 shows 5 webviews < 2GB on all platforms
- [ ] LRU policy implemented and tested
- [ ] Memory pressure handling functional

---

### RISK-003: CSP Enforcement Inconsistency
**Severity:** High
**Status:** Open

**Description:** CSP headers may not be consistently enforced across WebView2, WKWebView, and WebKitGTK, leaving exfiltration paths open on some platforms.

**Owner:** TBD

**Mitigation Plan:**
1. Gating Spike 0.2 tests CSP on all platforms
2. Custom protocol handler injects headers
3. Platform-specific CSP variations documented
4. Exfiltration test suite per platform

**Retired When:**
- [ ] Spike 0.2 passes on all 3 platforms
- [ ] `connect-src 'none'` verified blocking fetch/XHR
- [ ] Platform test matrix complete

---

### RISK-004: Session Token Leakage
**Severity:** Medium
**Status:** Open

**Description:** If session tokens leak (logging, shell XSS, clipboard), they could be reused by attackers.

**Owner:** TBD

**Mitigation Plan:**
1. Tokens bound to webview label (Domain 3)
2. Short TTL with rotation
3. Token never logged or displayed
4. Shell XSS hardening (Domain 1)

**Retired When:**
- [ ] Token binding implemented
- [ ] Token never appears in logs
- [ ] Shell CSP prevents XSS

---

### RISK-005: Scope Creep
**Severity:** Medium
**Status:** Open

**Description:** Attempting to build full platform + marketplace + CRDT + messaging simultaneously could delay delivery indefinitely.

**Owner:** TBD

**Mitigation Plan:**
1. Vertical slice milestones defined (Domain 10)
2. Slice A must work before expanding scope
3. Each domain spec is bounded

**Retired When:**
- [ ] Slice A implemented and working
- [ ] Scope explicitly bounded per phase

---

### RISK-006: Navigation Exfiltration
**Severity:** Medium
**Status:** Open

**Description:** Apps could exfiltrate data via `window.location`, `window.open`, or external URL schemes even if `connect-src` is blocked.

**Owner:** TBD

**Mitigation Plan:**
1. External interaction policy matrix (Domain 2)
2. Webview navigation handler blocks external URLs
3. `window.open` disabled
4. Scheme handlers for `mailto:`, `tel:`, etc.

**Retired When:**
- [ ] Navigation blocking tested on all platforms
- [ ] External scheme handling implemented
- [ ] E2E test for navigation exfiltration

---

### RISK-007: Static Tauri Capabilities vs Dynamic Apps
**Severity:** Medium
**Status:** Open

**Description:** Tauri capability files are build-time static. Cannot generate per-app capability files for dynamically installed apps.

**Owner:** TBD

**Mitigation Plan:**
1. Single exposed command (`bridge_request`) for apps
2. All permission enforcement in Rust
3. Do not rely on Tauri capability files for app isolation

**Retired When:**
- [ ] Architecture uses single bridge command
- [ ] Permission enforcement in Rust verified

---

## Retired Risks

*Risks move here when all retirement criteria are met.*

| Risk ID | Title | Retired Date | Evidence |
|---------|-------|--------------|----------|
| - | - | - | - |

---

## Risk Review Log

| Date | Reviewer | Actions |
|------|----------|---------|
| 2026-01-20 | Initial | Created register with 7 initial risks |
