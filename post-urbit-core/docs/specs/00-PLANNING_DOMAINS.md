# Post-Urbit Frontend Specification Domains (Revised)

## Overview

This document defines the specification domains for the Post-Urbit frontend implementation. The structure incorporates GPT-5.2 feedback on domain ordering, missing concerns, and review methodology.

---

## Revised Domain Order

Based on GPT-5.2 review, the planning has been restructured:

| Phase | Domain | Description | Status |
|-------|--------|-------------|--------|
| **0** | Gating Spikes | Prove feasibility before committing | Pending |
| **1** | Shell Architecture | Tauri shell and system UI | Pending |
| **2** | App Sandbox & Isolation | Webview isolation model | Pending |
| **2.5** | Resource Constraints (Early) | Memory budgets and LRU policy | Pending |
| **3** | Secure Bridge Protocol | IPC and message passing | Pending |
| **3.5** | Protocol Registry | Single source of truth for ABI | Pending |
| **4** | Permission System | Capability-based access control | Pending |
| **5** | SDK & Developer Experience | TypeScript SDK | Pending |
| **5+** | **SYSTEM REVIEW #1** | Coherence check | Pending |
| **6** | App Lifecycle Management | Install, launch, update | Pending |
| **6.5** | Package Trust & Updates | Signing, revocation, update security | Pending |
| **7** | State & Storage Architecture | Data persistence | Pending |
| **8** | Observability & Diagnostics | Logging, metrics, crash reporting | Pending |
| **9** | Security Hardening | Defense-in-depth consolidation | Pending |
| **10** | Testing & Validation Strategy | Test pyramid and CI/CD | Pending |
| **10+** | **SYSTEM REVIEW #2** | Final coherence check | Pending |

---

## Supporting Artifacts (Cross-Cutting)

These artifacts span multiple domains and are updated throughout:

| Artifact | Location | Purpose |
|----------|----------|---------|
| Protocol Registry | `docs/specs/PROTOCOL_REGISTRY.yaml` | Single ABI source of truth |
| Risk Register | `docs/RISK_REGISTER.md` | Track and retire risks |
| ADR Directory | `docs/adrs/` | Architecture Decision Records |
| Review Checklist | `docs/specs/REVIEW_CHECKLIST.md` | Standard review invariants |

---

## Phase 0: Gating Spikes (MUST PASS BEFORE PROCEEDING)

### Objective
Prove the fundamental architecture is viable before committing to full specification.

### Spike 0.1: Sandbox Containment Proof
**Question:** Can untrusted app content be prevented from accessing Tauri APIs?

**Experiment:**
1. Create minimal Tauri 2.x app with multi-webview (unstable feature)
2. Create "malicious" app that attempts:
   - Direct `invoke()` calls
   - Access to `__TAURI__` global
   - Access to Tauri plugins (shell, fs, dialog)
   - Navigation to external URLs
   - `window.open()` popups
3. Verify all attempts are blocked

**Pass Criteria:**
- [ ] Untrusted webview cannot call `invoke()`
- [ ] `__TAURI__` is undefined in app context
- [ ] External navigation is blocked
- [ ] Popups are blocked

### Spike 0.2: CSP Enforcement via Custom Protocol
**Question:** Can we reliably inject CSP headers via custom protocol handler?

**Experiment:**
1. Register `postapp://` protocol in Tauri
2. Serve app content with CSP headers
3. Verify CSP is enforced in webview

**Pass Criteria:**
- [ ] CSP headers applied on Windows (WebView2)
- [ ] CSP headers applied on macOS (WKWebView)
- [ ] CSP headers applied on Linux (WebKitGTK)
- [ ] `connect-src 'none'` blocks fetch/XHR

### Spike 0.3: Multi-Webview Memory Baseline
**Question:** What is the memory overhead of multi-webview on each platform?

**Experiment:**
1. Create app with 1, 3, 5, 10 webviews
2. Measure memory usage on each platform
3. Measure webview creation time

**Pass Criteria:**
- [ ] 5 webviews < 2GB RAM on all platforms
- [ ] Webview creation < 3s on all platforms
- [ ] Documented baseline numbers

### Spike 0.4: MessageChannel Transfer to Webview
**Question:** Can we transfer a MessagePort to an isolated webview for secure IPC?

**Experiment:**
1. Create MessageChannel in shell
2. Transfer port2 to app webview
3. Verify bidirectional communication works

**Pass Criteria:**
- [ ] MessagePort transfer works cross-webview
- [ ] Communication is functional
- [ ] Original postMessage route can be blocked

---

## Domain Specifications

### Domain 1: Shell Architecture

#### Scope
The Tauri application shell that hosts all apps and provides system UI.

#### Required Deliverables
- [ ] Shell component hierarchy (React + shadcn)
- [ ] Window management (single window, multi-webview layout)
- [ ] App container component specification
- [ ] Navigation/routing within shell
- [ ] System UI: app launcher, sidebar, status bar, notifications
- [ ] Shell state management (Zustand recommended)
- [ ] Shell CSP and XSS hardening
- [ ] `tauri.conf.json` specification
- [ ] Accessibility requirements (keyboard nav, screen reader)
- [ ] Theming architecture

#### Key Decisions (Require ADR)
- [ ] ADR-001: State management approach
- [ ] ADR-002: Component library constraints

#### Security Invariants (Must be enforced in Rust)
- Shell cannot dynamically load remote code
- Shell CSP prevents inline scripts
- All HTML rendering is sanitized

#### Acceptance Criteria
- [ ] Shell launches in < 2s on all platforms
- [ ] Keyboard navigation works for all system UI
- [ ] No XSS vectors in shell code

---

### Domain 2: App Sandbox & Isolation

#### Scope
How third-party apps are isolated from the shell and each other.

#### Required Deliverables
- [ ] **Final decision: multi-webview vs iframe** (ADR required)
- [ ] Webview creation/destruction API
- [ ] Process isolation guarantees per platform
- [ ] `postapp://` custom protocol specification
- [ ] CSP header injection (exact headers)
- [ ] Navigation policy (allowlist/blocklist)
- [ ] Sandbox attribute specification (if iframe)
- [ ] Tauri IPC lockdown specification
- [ ] External interaction policy matrix

#### Key Decisions (Require ADR)
- [ ] ADR-003: Multi-webview vs iframe isolation model
- [ ] ADR-004: Navigation and external URL handling

#### External Interaction Policy Matrix
| Vector | Policy | Enforcement |
|--------|--------|-------------|
| `window.location` | Block external | Webview nav handler |
| `window.open()` | Block | sandbox/webview config |
| `<a href>` external | Block or intercept | CSP + handler |
| `mailto:`, `tel:` | Intercept, require permission | Scheme handler |
| File downloads | Block or route through permission | CSP + handler |
| Clipboard write | Require permission | Bridge API |
| Clipboard read | Require permission | Bridge API |
| Drag-and-drop out | Block | Platform config |

#### Security Invariants (Must be enforced in Rust)
- Apps cannot access `__TAURI__` APIs
- Apps can only communicate via MessagePort bridge
- Navigation to non-`postapp://` origins is blocked

#### Acceptance Criteria
- [ ] Malicious test app cannot escape sandbox
- [ ] CSP enforced on all 3 platforms
- [ ] Navigation blocking works on all 3 platforms

---

### Domain 2.5: Resource Constraints (Early)

#### Scope
Define memory budgets and resource limits before detailed design.

#### Required Deliverables
- [ ] Memory budget per app (target: 100-200MB average)
- [ ] Maximum concurrent webviews (target: 5-10)
- [ ] Total memory budget (target: < 2GB with 5 apps)
- [ ] LRU unloading policy (thresholds)
- [ ] Webview creation time budget (target: < 2s)
- [ ] Bridge request latency budget (target: < 50ms p95)

#### Platform Deltas
| Platform | Expected Overhead | Notes |
|----------|------------------|-------|
| Windows (WebView2) | 50-100MB/webview | Higher initial |
| macOS (WKWebView) | 100-300MB/webview | Variable |
| Linux (WebKitGTK) | 50-100MB/webview | Lower baseline |

---

### Domain 3: Secure Bridge Protocol

#### Scope
The secure communication channel between apps and the Rust backend.

#### Required Deliverables
- [ ] Complete handshake flow (sequence diagram)
- [ ] Session token format (HMAC-SHA256, fields, TTL)
- [ ] Token binding to caller identity (webview label)
- [ ] MessageChannel/MessagePort lifecycle
- [ ] CBOR message schema (all types)
- [ ] Request ID format (UUID v7 recommended)
- [ ] Replay prevention (TTL map, bounded size)
- [ ] Rate limiting specification (per-session, per-method)
- [ ] Error codes and error handling
- [ ] Subscription/event system specification
- [ ] Backpressure handling

#### Key Decisions (Require ADR)
- [ ] ADR-005: Message encoding (CBOR vs JSON)
- [ ] ADR-006: Request ID generation strategy

#### Security Invariants (Must be enforced in Rust)
- Token validation on every request
- Caller webview label checked against session
- Replay protection enforced
- Rate limits enforced

#### Acceptance Criteria
- [ ] Handshake completes in < 100ms
- [ ] Bridge request latency < 50ms p95
- [ ] Replay attacks rejected
- [ ] Rate limiting functional

---

### Domain 3.5: Protocol Registry (ABI Source of Truth)

#### Scope
Machine-readable registry of all platform APIs for consistency.

#### Required Deliverables
- [ ] `PROTOCOL_REGISTRY.yaml` schema
- [ ] All method definitions (name, params, returns, permissions)
- [ ] All event definitions (name, payload)
- [ ] All error codes
- [ ] Version negotiation rules
- [ ] Deprecation rules
- [ ] Code generation for Rust structs
- [ ] Code generation for TypeScript types

#### Schema Example
```yaml
version: "1.0.0"
methods:
  storage.get:
    permission: storage:read
    params:
      key: string
    returns:
      value: bytes | null
    errors: [not_found, permission_denied]
  storage.set:
    permission: storage:write
    params:
      key: string
      value: bytes
    returns: void
    errors: [quota_exceeded, permission_denied]
events:
  storage.changed:
    payload:
      key: string
      new_value: bytes | null
```

---

### Domain 4: Permission System

#### Scope
Capability-based permission enforcement for app API access.

#### Required Deliverables
- [ ] Complete permission registry (all permissions)
- [ ] Permission tiers and rules
- [ ] Grant/denial persistence schema
- [ ] Permission prompt UI specification
- [ ] Prompt UX rules (when to show, cooldowns)
- [ ] Audit log schema and storage
- [ ] Permission checking flow (sequence diagram)
- [ ] App manifest permission declaration
- [ ] Permission migration strategy

#### Permission Tiers
| Tier | Behavior | Example |
|------|----------|---------|
| AlwaysGranted | No prompt, always allowed | `storage:read` (own data) |
| PromptOnce | Prompt once, persist decision | `contacts:read` |
| PromptAlways | Prompt every time | `clipboard:read` |
| SystemOnly | Never granted to apps | `shell:execute` |

#### Security Invariants (Must be enforced in Rust)
- Permission check happens in Rust, not UI
- Grants persisted to tamper-resistant storage
- Audit log append-only

#### Acceptance Criteria
- [ ] All permissions documented with tier
- [ ] Prompt UI accessible (keyboard, screen reader)
- [ ] Audit log captures all permission decisions

---

### Domain 5: SDK & Developer Experience

#### Scope
The TypeScript SDK that app developers use to interact with the platform.

#### Required Deliverables
- [ ] Package structure (`@post-urbit/sdk`)
- [ ] Complete API surface (generated from Protocol Registry)
- [ ] React hooks (`useStore`, `useIdentity`, `useMessaging`, etc.)
- [ ] Error types and handling patterns
- [ ] TypeScript type definitions (generated)
- [ ] Subscription lifecycle management
- [ ] Version negotiation client-side
- [ ] Documentation and examples
- [ ] App theming hooks (constrained)

#### Key Decisions (Require ADR)
- [ ] ADR-007: SDK state management approach

#### Acceptance Criteria
- [ ] SDK types match Protocol Registry
- [ ] All hooks properly clean up subscriptions
- [ ] Documentation covers all methods

---

### SYSTEM REVIEW #1 (After Domain 5)

#### Review Checklist
- [ ] All ADRs documented (001-007)
- [ ] Protocol Registry complete and consistent
- [ ] No naming contradictions across specs
- [ ] Security invariants clear ("must be enforced in Rust")
- [ ] Platform deltas documented (Win/macOS/Linux)
- [ ] Acceptance criteria testable
- [ ] Gating spike results incorporated

---

### Domain 6: App Lifecycle Management

#### Scope
How apps are discovered, installed, launched, updated, and uninstalled.

#### Required Deliverables
- [ ] App package format (`.postapp`) specification
- [ ] Manifest schema (`manifest.json`) complete definition
- [ ] Installation flow (sequence diagram)
- [ ] Launch flow (webview creation, handshake)
- [ ] Update flow (version checking, migration)
- [ ] Uninstall flow (data handling options)
- [ ] App registry data model
- [ ] Offline-first considerations

#### Acceptance Criteria
- [ ] Install completes in < 5s for 10MB app
- [ ] Launch to interactive < 3s
- [ ] Update preserves app data

---

### Domain 6.5: Package Trust & Updates

#### Scope
Cryptographic verification and secure updates.

#### Required Deliverables
- [ ] Signature format (Ed25519 recommended)
- [ ] Trust store format (developer public keys)
- [ ] Key rotation mechanism
- [ ] Revocation mechanism (revocation list or transparency log)
- [ ] Downgrade attack prevention
- [ ] Update metadata signing
- [ ] First-install trust UX
- [ ] Rollback policy

#### Key Decisions (Require ADR)
- [ ] ADR-008: Signature algorithm and key management

#### Security Invariants (Must be enforced in Rust)
- Signature verification before installation
- Downgrade blocked (version must increase)
- Revoked keys rejected

#### Acceptance Criteria
- [ ] Invalid signature rejected
- [ ] Revoked key rejected
- [ ] Downgrade rejected

---

### Domain 7: State & Storage Architecture

#### Scope
How app and system state is persisted and synchronized.

#### Required Deliverables
- [ ] Per-app storage isolation mechanism
- [ ] Storage API specification (key-value + structured)
- [ ] Data encryption at rest specification
- [ ] Storage quotas and limits
- [ ] Backup/restore specification
- [ ] Data export format
- [ ] CRDT integration points (if applicable)
- [ ] Migration strategy for schema changes

#### Acceptance Criteria
- [ ] App A cannot read App B's data
- [ ] Data encrypted at rest
- [ ] Backup/restore functional

---

### Domain 8: Observability & Diagnostics

#### Scope
Logging, metrics, crash reporting, and debugging.

#### Required Deliverables
- [ ] Structured log schema (with correlation IDs)
- [ ] Per-app/per-session log isolation
- [ ] Log redaction rules (PII, secrets)
- [ ] Crash reporting strategy (local + optional remote)
- [ ] Performance metrics (webview creation, bridge latency, memory)
- [ ] Diagnostics bundle export ("export logs")
- [ ] Developer debugging tools
- [ ] User-facing health indicators

#### Acceptance Criteria
- [ ] Logs include session/app correlation IDs
- [ ] PII redacted from logs
- [ ] Diagnostics export functional

---

### Domain 9: Security Hardening (Consolidation)

#### Scope
Defense-in-depth security measures consolidation and audit.

#### Required Deliverables
- [ ] Threat model document
- [ ] Attack surface inventory
- [ ] All security invariants (consolidated from all domains)
- [ ] Malicious test app suite specification
- [ ] Security audit checklist
- [ ] Incident response runbook
- [ ] Security testing automation

#### Acceptance Criteria
- [ ] All threat vectors documented with mitigations
- [ ] Malicious test suite defined
- [ ] Audit checklist complete

---

### Domain 10: Testing & Validation Strategy

#### Scope
How the implementation will be tested and validated.

#### Required Deliverables
- [ ] Unit testing strategy
- [ ] Integration testing strategy
- [ ] E2E testing strategy
- [ ] Security regression tests
- [ ] Performance benchmarks
- [ ] Platform test matrix (Windows, macOS, Linux)
- [ ] CI/CD pipeline specification

#### Vertical Slice Milestones
| Slice | Components | Status |
|-------|------------|--------|
| A | Install → Launch → Handshake → storage.get/set → Permission prompt → Audit log | Pending |
| B | App-to-app messaging with permission mediation | Pending |
| C | App update with signature verification + rollback | Pending |

#### Acceptance Criteria
- [ ] Test coverage > 80% for critical paths
- [ ] Security tests pass on all platforms
- [ ] Performance benchmarks documented

---

### SYSTEM REVIEW #2 (After Domain 10)

#### Review Checklist
- [ ] All 10+ domains have complete specifications
- [ ] All ADRs finalized
- [ ] Protocol Registry generates valid code
- [ ] Risk Register shows all risks retired or accepted
- [ ] Vertical slice milestones are achievable
- [ ] Ready for implementation phase

---

## Definition of Done (Per Domain)

Each domain specification must include:

1. **Sequence diagrams** for key flows
2. **Data models** with field types and constraints
3. **Rust interfaces** (trait/struct signatures)
4. **TypeScript interfaces** (type definitions)
5. **Security invariants** ("must be enforced in Rust")
6. **Platform deltas** (Win/macOS/Linux differences)
7. **Acceptance criteria** (testable conditions)
8. **Test cases** (specific scenarios)
