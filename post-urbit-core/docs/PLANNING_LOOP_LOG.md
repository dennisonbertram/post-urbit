# Post-Urbit Frontend Implementation Planning Loop

## Methodology

This document tracks the systematic planning process for the Post-Urbit frontend implementation. Each planning loop follows this structure:

### Loop Structure
1. **Pre-Work GPT-5.2 Review**: Before a subagent begins work, GPT-5.2 reviews the proposed plan/approach
2. **Subagent Execution**: Specialized subagent creates detailed specification
3. **Post-Work GPT-5.2 Review**: After completion, GPT-5.2 reviews the specification for completeness and correctness
4. **Documentation**: All outputs documented in this log and dedicated spec files

### System Review Cadence
- **Every 5 loops**: Comprehensive system review to ensure coherence and prevent drift
- Reviews check: cross-document consistency, architectural alignment, completeness

---

## Planning Domains (Revised)

| Loop | Domain | Subagent Type | Status |
|------|--------|---------------|--------|
| 0 | Planning Framework Setup | - | Complete |
| 1 | Phase 0 Gating Spikes | Plan | Complete |
| 2 | Shell Architecture | Plan | Complete |
| 3 | App Sandbox & Isolation | Plan | Complete |
| 4 | Resource Constraints | Plan | Complete |
| 5 | Secure Bridge Protocol | Plan | Complete |
| 5+ | **SYSTEM REVIEW #1** | Review | Complete |
| 6 | Protocol Registry | Plan | Complete |
| 7 | Permission System | Plan | Complete |
| 8 | SDK & Developer Experience | Plan | Complete |
| 9 | App Lifecycle Management | Plan | Complete |
| 10 | Package Trust & Updates | Plan | Complete |
| 10+ | **SYSTEM REVIEW #2** | Review | Complete |

---

## Loop Execution Log

### Loop 0: Planning Framework Setup
**Timestamp**: 2026-01-20 14:00
**Status**: Complete

#### Objective
Establish the planning framework, define all specification domains, and create documentation structure.

#### Existing Documents (Input Context)
1. `docs/FRONTEND_ARCHITECTURE.md` - Initial architecture design
2. `docs/SECURE_BRIDGE_PROTOCOL.md` - Bridge protocol specification
3. `docs/TAURI_INTEGRATION_PLAN.md` - Implementation roadmap
4. `docs/TAURI_MULTIWEBVIEW_RESEARCH.md` - Multi-webview research
5. `code-reviews/full-architecture-review-20260120-141102.md` - GPT-5.2 comprehensive review

#### Key Issues from GPT-5.2 Review to Address
1. **Sandbox contradiction**: iframe vs multi-webview - need concrete decision
2. **CSP enforcement**: Remove iframe attribute CSP, use headers only
3. **API naming**: Standardize method names across all docs
4. **Static capabilities**: Cannot rely on Tauri capability files for dynamic apps
5. **Security gaps**: Tauri IPC escape, shell XSS, token binding

#### Outputs
- `docs/PLANNING_LOOP_LOG.md` - This document
- `docs/specs/00-PLANNING_DOMAINS.md` - Domain definitions
- `docs/specs/REVIEW_CHECKLIST.md` - Standard review checklist
- `docs/adrs/ADR-000-TEMPLATE.md` - ADR template
- `docs/RISK_REGISTER.md` - Risk tracking

#### GPT-5.2 Review of Planning Methodology
**Location**: `code-reviews/planning-methodology-review-20260120-*.md`
**Key Feedback Incorporated**:
- Added Phase 0 Gating Spikes before domain specs
- Pulled security hardening and resource constraints forward
- Added Protocol Registry domain
- Added Package Trust & Updates domain
- Created ADR directory and Risk Register

---

### Loop 1: Phase 0 Gating Spikes Specification
**Timestamp**: 2026-01-20 15:00
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/spike-preplan-review-20260120-*.md`

**Key Feedback**:
1. Missing spike: Custom scheme must be tested as Secure Context
2. Missing spike: Per-app origin isolation must be verified
3. Cross-webview MessagePort transfer likely to fail
4. Pass criteria need machine-verifiable JSON output
5. Recommended execution order: scheme viability → CSP → containment → IPC → memory

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/PHASE_0_GATING_SPIKES.md`

**Specification Contents**:
- 7 spikes defined (0.1-0.7)
- Spike 0.1: Custom Scheme Secure Context (CRITICAL)
- Spike 0.2: Per-App Origin Isolation (CRITICAL)
- Spike 0.3: CSP Enforcement via Custom Protocol (CRITICAL)
- Spike 0.4: Sandbox Containment Proof (CRITICAL)
- Spike 0.5: IPC Primitive Feasibility (HIGH)
- Spike 0.6: Multi-Webview Memory Baseline (MEDIUM)
- Spike 0.7: Crash Containment (OPTIONAL)
- Go/No-Go decision matrix
- Platform test matrix (Windows/macOS/Linux)
- Implementation checklist

#### Post-Work Review
**Location**: `code-reviews/spike-spec-postwork-review-20260120-*.md`
**Rating**: 6/10

**Key Gaps Identified**:
1. Spike 0.2 probes don't actually validate cross-webview isolation (false PASS risk)
2. Spike 0.5 lacks concrete IPC experiment design under containment constraints
3. Missing harness design for result collection across platforms
4. CSP source syntax may be incorrect for custom schemes
5. Go/No-Go matrix has inconsistent blocking/non-blocking classifications

**Improvements Needed (for revision pass)**:
- Add Rust-coordinated multi-phase cross-read test for Spike 0.2
- Fully specify IPC mechanism for Spike 0.5
- Add harness design section
- Require `securitypolicyviolation` evidence for CSP tests
- Reconcile severity classifications

---

### Loop 2: Shell Architecture Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/shell-arch-preplan-review-20260120-*.md`

**Key Feedback**:
1. Need ADRs 001-008 for all major decisions
2. Component tree needs complete typing
3. Security hardening needs Tauri 2.x specifics
4. State management slices need clear definitions

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/01-SHELL_ARCHITECTURE.md`

**Specification Contents**:
- Shell as Tauri main window with multi-webview coordination
- Zustand slice-based state management (ADR-001)
- shadcn/ui + Tailwind component library (ADR-002)
- Security hardening (CSP, navigation blocking, devtools disabled)
- Shell-only Tauri commands for app lifecycle
- TypeScript interfaces for all state slices

#### Post-Work Review
**Location**: `code-reviews/shell-arch-postwork-review-20260120-*.md`
**Rating**: 6/10

**Key Gaps Identified**:
1. Missing ADRs 003-008 (referenced but not created)
2. Webview model underspecified for multi-webview
3. No mock/wireframe for component layout
4. Security section lacks specific Tauri 2.x APIs

#### Outputs
- `docs/specs/01-SHELL_ARCHITECTURE.md`
- `docs/adrs/ADR-001-state-management.md`
- `docs/adrs/ADR-002-component-library.md`

---

### Loop 3: App Sandbox & Isolation Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/sandbox-preplan-review-20260120-*.md`

**Key Feedback**:
1. Must define multi-webview vs iframe decision with ADR
2. Need concrete webview lifecycle API
3. CSP must be specified with exact headers
4. Navigation blocking at Rust layer, not JS
5. Capability files must be fully specified

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/02-APP_SANDBOX_ISOLATION.md`

**Specification Contents**:
- Multi-webview for ALL untrusted apps (ADR-003)
- Custom protocol `postapp://{app_id}/` with security headers
- Webview lifecycle API (AppRunState: Hot/Warm/Cold)
- CSP header injection via protocol handler
- Navigation hooks blocking external URLs
- Popup blocking via `on_new_window_request`
- Tauri capability files (shell + app-default)
- Single bridge command `postbridge_invoke`
- External interaction policy matrix
- Platform considerations table

#### Post-Work Review
**Location**: `code-reviews/sandbox-postwork-review-20260120-*.md`
**Rating**: 7/10

**Key Gaps Identified**:
1. **Origin isolation not proven** - `postapp://{app_id}` origin semantics are platform-dependent
2. **CSP `connect-src 'none'` too strict** - breaks same-origin fetch, should be `'self'`
3. **Capability wildcard `app-*`** - Tauri may not support wildcard label matching
4. **Missing `worker-src`** - CSP incomplete for modern apps
5. **WASM policy implicit** - should be opt-in per permission tier
6. **Command naming mismatch** - `shell_launch_app` vs `app_launch` across specs

**Improvements Needed (for revision pass)**:
- Add explicit cross-app origin isolation test in Phase 0 spikes
- Change CSP `connect-src` to `'self'` and add `worker-src`
- Verify Tauri capability wildcard support or use explicit per-webview registration
- Add bridge hardening (size limits, rate limits)
- Normalize command names across all specs

#### Outputs
- `docs/specs/02-APP_SANDBOX_ISOLATION.md`
- `docs/adrs/ADR-003-multiwebview-isolation.md`

---

## Review Summary

| Loop | Spec | Pre-Review | Post-Review | Rating |
|------|------|------------|-------------|--------|
| 0 | Planning Framework | N/A | N/A | N/A |
| 1 | Gating Spikes | Pass | Pass w/ Notes | 6/10 |
| 2 | Shell Architecture | Pass | Pass w/ Notes | 6/10 |
| 3 | App Sandbox & Isolation | Pass | Pass w/ Notes | 7/10 |
| 4 | Resource Constraints | Pass | Pass w/ Notes | 6/10 |
| 5 | Secure Bridge Protocol | Pass | Pass w/ Notes | 7/10 |
| 5+ | **SYSTEM REVIEW #1** | Complete | - | 7/10 |
| 6 | Protocol Registry | Pass | Pass w/ Notes | 6/10 |
| 7 | Permission System | Pass | Pass w/ Notes | 6/10 |
| 8 | SDK & Developer Experience | Pass | Pass w/ Notes | 6/10 |
| 9 | App Lifecycle Management | Pass | Pass w/ Notes | 6/10 |
| 10 | Package Trust & Updates | Pass | Pass w/ Notes | 6/10 |
| 10+ | **SYSTEM REVIEW #2** | Complete | - | 6/10 |

---

### Loop 4: Resource Constraints Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/resources-preplan-review-20260120-*.md`

**Key Feedback**:
1. Define device-classed defaults based on physical RAM
2. Shell budget: 200MB target, 250MB warn, 350MB critical
3. Per-app limits: 300MB soft, 500MB hard, CPU thresholds
4. Bridge limits: 256KB payload, 50 rps sustained, 200 burst
5. LRU with hysteresis to prevent thrashing
6. Graceful eviction handshake with 1500ms deadline

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/03-RESOURCE_CONSTRAINTS.md`

**Specification Contents**:
- DeviceClass enum (Constrained/Standard/Performance)
- ResourceLimitsConfig with all thresholds
- BridgeLimitsConfig with rate limiting
- StorageLimitsConfig with quotas
- LRU eviction algorithm with scoring
- Graceful eviction handshake
- Pressure signaling protocol
- Shell commands for resource management
- App bridge APIs for budget/pressure
- Platform-specific memory reading (Win/macOS/Linux)
- Low-resource mode
- Comprehensive test cases

#### Post-Work Review
**Location**: `code-reviews/resources-postwork-review-20260120-*.md`
**Rating**: 6/10

**Key Gaps Identified**:
1. **LRU scoring is buggy** - reversed ordering, non-evictables can be selected
2. **Eviction handshake ambiguous** - direction of prepare_for_unload unclear
3. **Budget inconsistency** - per-app hard caps × max webviews can exceed total budget
4. **Platform memory attribution** - WebView2 multi-process not handled, handle leak in Windows code
5. **Missing edge cases** - focused app under critical pressure, all apps pinned
6. **Total budget not used as trigger** - only count/time/system pressure trigger eviction

**Improvements Needed (for revision pass)**:
- Fix LRU scoring (reverse elapsed ordering, exclude non-evictables before sort)
- Clarify eviction handshake direction with request_id and submit method
- Add global + per-app bridge rate limits
- Define process attribution strategy per platform
- Add last-resort policies for critical pressure

#### Outputs
- `docs/specs/03-RESOURCE_CONSTRAINTS.md`

---

### Loop 5: Secure Bridge Protocol Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/bridge-preplan-review-20260120-*.md`

**Key Feedback**:
1. Define strict CBOR profile with depth/length/payload limits
2. Session binding to webview label (not just app_id)
3. Anti-replay: LRU request cache returning cached response
4. Method namespaces with default deny
5. Error collapsing for auth failures
6. Long-poll for event subscriptions (single IPC command)
7. TOCTOU: two-step flow for prompted actions
8. Key rotation with `kid` support

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/04-SECURE_BRIDGE_PROTOCOL.md`

**Specification Contents**:
- Transport binding (single `postbridge_invoke` command)
- Strict CBOR profile with validation rules
- CDDL schema for request/response envelopes
- Session lifecycle state machine
- Token format with key rotation
- Anti-replay with cached response semantics
- Method registry with authorization table
- Error taxonomy with 9 codes
- Timeout and idempotency semantics
- Event subscription long-poll
- Chunked transfer protocol
- Explicit threat mitigations

#### Post-Work Review
**Location**: `code-reviews/bridge-postwork-review-20260120-*.md`
**Rating**: 7/10

**Key Gaps Identified**:
1. **Replay cache before auth** - Should authenticate before returning cached response
2. **No singleflight** - Concurrent duplicates can both execute
3. **CBOR duplicate keys** - Standard decoders don't detect during parse
4. **Token delimiter ambiguity** - HMAC payload uses `:` which could be in fields
5. **Event poll busy-wait** - DoS risk, should use async notify
6. **app_id empty allowed** - `label="app-"` yields empty app_id

**Improvements Needed (for revision pass)**:
- Move auth validation before replay cache check
- Add singleflight/in-flight dedupe with request digest
- Use streaming CBOR validator or canonical CBOR
- Use structured HMAC payload (CBOR-encoded)
- Replace poll busy-wait with async notify
- Enforce app_id charset/length constraints

#### Outputs
- `docs/specs/04-SECURE_BRIDGE_PROTOCOL.md`

---

### System Review #1
**Timestamp**: 2026-01-20
**Status**: Complete
**Rating**: 7/10 coherence

#### Review Scope
All specifications from Loops 1-5:
- Phase 0 Gating Spikes
- Shell Architecture + ADR-001, ADR-002
- App Sandbox & Isolation + ADR-003
- Resource Constraints
- Secure Bridge Protocol

#### Review Location
`code-reviews/system-review-1-20260120-*.md`

#### Key Findings

**Coherent Aspects**:
- Clear privilege boundary (shell vs apps)
- Multi-webview isolation consistently specified
- Single IPC entrypoint (`postbridge_invoke`)
- Defense-in-depth layering (capabilities, label checks, CSP, rate limits)
- Resource model is thorough and operationally realistic

**Critical Inconsistencies**:
1. **Command naming drift**: `shell_*` vs `app_*` for lifecycle commands
2. **Event delivery contradiction**: `03` implies push events, `04` specifies long-poll only
3. **Type representation**: `AppRunState` enum vs string literals in different specs
4. **Cross-reference errors**: ADR-003 to ADR-008 referenced but don't exist

**Critical Gaps**:
1. **Session bootstrap mechanism** - How apps obtain session/token not specified
2. **App IPC surface** - How apps call `postbridge_invoke` without full Tauri JS API unclear
3. **Storage quota enforcement** - Mechanism for browser-managed storage not defined

#### Remediation Plan (Priority Order)

1. **Bridge Bootstrap Specification** (Critical)
   - Add section to bridge spec: session injection, minimal globals exposed
   - Update Spike 0.4 to test expected minimal surface

2. **Naming Conventions Document**
   - Create `docs/specs/00-NAMING_CONVENTIONS.md`
   - Standardize: Tauri commands (`shell_*`), bridge methods (`domain.action`)

3. **Event Model Unification**
   - Change `app://resource/*` to `events.subscribe` topics
   - Remove push event language from resource spec

4. **Schema Source of Truth**
   - Create `/schemas/` with CDDL + Rust/TS type definitions
   - Replace string enums with real enums

5. **Fix Cross-References**
   - Add missing ADR stubs or remove references
   - Add CI check for doc link validity

---

## Risk Updates

Based on Loop 1 findings, the following risks require attention:

| Risk | Update |
|------|--------|
| RISK-001 (IPC Escape) | Spike 0.4 will validate; spec needs Tauri API confirmation |
| RISK-003 (CSP Inconsistency) | Spike 0.3 tests this; syntax needs verification |
| RISK-007 (Static Capabilities) | Architecture confirmed: single `bridge_request` command |

---

### Loop 6: Protocol Registry Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/registry-preplan-review-20260120-*.md`

**Key Feedback**:
1. Define module vs method distinction clearly
2. Namespace ownership rules and conflict resolution
3. Version negotiation (protocol, module, schema axes)
4. Extension module security boundaries
5. Schema validation enforcement strategy
6. Introspection APIs for capability discovery

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/05-PROTOCOL_REGISTRY.md`

**Specification Contents**:
- ModuleSpec, MethodSpec, SchemaSpec, CapabilitySpec data models
- Namespace ownership (core namespaces + reverse-DNS for extensions)
- Registration lifecycle (Pending → Active → Inactive/Updating)
- Version negotiation on 3 axes (protocol, module, schema)
- Authorization contract with PermissionTier enum
- Introspection APIs (bridge.get_server_info, list_methods, get_method_spec)
- Extension module system with signing requirements
- Backward compatibility policy
- Schema validation with CDDL
- Registry integrity (hash computation, audit log)
- 8-phase implementation checklist

#### Post-Work Review
**Location**: `code-reviews/registry-postwork-review-20260120-*.md`
**Rating**: 6/10

**Key Gaps Identified**:
1. **Method naming inconsistency** - Bridge uses `storage.get`, Registry mandates `storage.v1.get`
2. **Response schema mismatch** - Registry adds `warnings` field not in Bridge envelope
3. **Extension security boundary underspecified** - Signing, trust roots, execution model missing
4. **Registration lifecycle incomplete** - Update/remove/rollback/atomicity not defined
5. **Schema validation underspecified** - CDDL library, validation ordering, strict mode unclear
6. **Introspection APIs incomplete** - Filtering ambiguous, no pagination, no capability listing
7. **Registry integrity weak** - Hash alone not tamper-resistant without signed root
8. **Rate limit class values missing** - No numeric parameters per class
9. **Error mapping inconsistent** - 404 vs NOT_FOUND vs UNAUTHORIZED
10. **Namespace field duplication** - module_id vs namespace_prefix can diverge

**Improvements Needed (for revision pass)**:
- Unify method naming across Bridge + Registry (version optional or required)
- Add `warnings` field to Bridge response CDDL if needed
- Fully specify extension signing (what's signed, trust roots, revocation)
- Complete lifecycle (update, remove, rollback, atomic commit)
- Specify validation order and strict mode precisely
- Clarify introspection contract (all public vs currently invokable)
- Add signed/anchored trust mechanism for registry integrity
- Define per-class rate limit numeric parameters
- Normalize error-hiding rules consistently

#### Outputs
- `docs/specs/05-PROTOCOL_REGISTRY.md`

---

### Loop 7: Permission System Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/permissions-preplan-review-20260120-*.md`

**Key Feedback**:
1. Define all core capabilities with semantics and risk levels
2. Permission tier semantics must align with Registry
3. TOCTOU flow with params_cbor_sha256 binding
4. Shell-rendered prompts only (anti-spoofing)
5. Rate limiting and anti-spam mechanisms
6. Persistence model for different scopes
7. Escalation rules for app updates
8. Revocation effects on pending actions

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/06-PERMISSION_SYSTEM.md`

**Specification Contents**:
- PermissionDecision, GrantScope, GrantSource enums
- CapabilityConstraint typed enum
- PermissionRecord and PendingAction structs
- Core capability catalog (6 capabilities defined)
- Permission tier semantics table
- Permission state machine with transitions
- 7-step authorization pipeline
- CDDL schemas for all permission bridge methods
- Shell prompt queue and rate limiting
- SQLite persistence schema
- Escalation detection on app updates
- Audit event taxonomy
- 13 test scenarios
- 8-phase implementation checklist

#### Post-Work Review
**Location**: `code-reviews/permissions-postwork-review-20260120-*.md`
**Rating**: 6/10

**Key Gaps Identified**:
1. **Cross-spec tier conflict** - Bridge spec shows permission methods as PromptAlways, should be AlwaysGranted
2. **Tier model ambiguity** - Method-level vs capability-level tier unclear
3. **AlwaysGranted pipeline bug** - Skips required_capabilities check
4. **Multi-capability TOCTOU missing** - No spec for methods with multiple required capabilities
5. **No "prompt required" error code** - Bridge error taxonomy doesn't signal TOCTOU flow
6. **ShellOnly invocation unclear** - How shell calls bridge methods not specified
7. **PromptAlways persistence conflict** - "Once (not stored)" but SQLite has UNIQUE constraint
8. **Revocation edge cases** - Behavior during in-flight execute_action unclear
9. **Escalation incomplete** - Optional capabilities and denied re-prompt rules missing
10. **Encryption not specified** - "Stored encrypted" params_cbor lacks scheme

**Improvements Needed (for revision pass)**:
- Reconcile permission method tiers across specs (04/05/06)
- Clarify method-tier precedence over capability-tier
- Fix authorization pipeline for AlwaysGranted with required_capabilities
- Define multi-capability TOCTOU flow (single prompt, single token)
- Add PROMPT_REQUIRED error details to bridge taxonomy
- Specify shell invocation path for ShellOnly methods
- Correct PromptAlways persistence model (audit-only)
- Define revocation atomicity and timing
- Complete escalation rules for optional and denied capabilities
- Specify AEAD encryption for pending action params

#### Outputs
- `docs/specs/06-PERMISSION_SYSTEM.md`

---

### Loop 8: SDK & Developer Experience Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/sdk-preplan-review-20260120-*.md`

**Key Feedback**:
1. Package architecture: sdk / protocol / devtools split
2. Bootstrap flow with shell-injected object
3. Transport layer wrapping single Tauri command
4. CBOR codec with client-side limits
5. Typed namespace APIs with TOCTOU helpers
6. Type generation from CDDL schemas
7. CLI toolchain: init, dev, build, package, install, typegen, doctor
8. Security constraints (no Tauri API, no token persistence)

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/07-SDK_DEVELOPER_EXPERIENCE.md`

**Specification Contents**:
- Package structure (@posturbit/sdk, protocol, devtools)
- PostUrbitBootstrap interface with validation
- Transport class with session credentials
- CBOR codec with limits (256KB, depth 32, 1000 items)
- ProtocolClient with InvokeOptions
- Error class hierarchy (BridgeError, RateLimitedError, etc.)
- Storage, System, Events, Permission, Resource, Blob namespaces
- TOCTOU withPermission() helper with polling
- Type generation pipeline (CDDL → JSON Schema → TS)
- CLI command specifications
- Templates (hello-world, events, permission-toctou)
- Bridge Inspector (dev mode only)
- Security invariants list
- 6 test scenario categories
- 9-phase implementation checklist

#### Post-Work Review
**Location**: `code-reviews/sdk-postwork-review-20260120-*.md`
**Rating**: 6/10

**Key Gaps Identified**:
1. **Method naming inconsistency** - Bridge uses `storage.get`, SDK uses `storage.v1.get`
2. **Envelope fields wrong** - SDK puts `idempotency_key` in params not envelope
3. **TOCTOU helper brittle** - Polls by string matching error message
4. **PromptAlways not enforced** - Transport accepts any method string
5. **Codec validation incomplete** - String length in chars not bytes, Uint8Array handling
6. **Error handling inconsistent** - Code samples don't typecheck
7. **Bootstrap validation gaps** - Missing app_id format, timestamp skew checks
8. **Type generation underspecified** - bstr mapping, schema merging unclear
9. **CLI integration gaps** - .postapp format, manifest schema, signing unspecified
10. **Testing gaps** - No conformance tests, no fuzz tests, no dev-mode gating tests

**Improvements Needed (for revision pass)**:
- Unify method naming across all specs
- Fix envelope to put idempotency_key/trace_id/deadline_ms at top level
- Add permission.get_action_status or events topic for TOCTOU
- Add PromptAlways enforcement at SDK layer (or unsafe* naming)
- Fix CBOR codec for UTF-8 bytes and TypedArrays
- Normalize error mapping with fromBridgeResponseError()
- Make typegen reproducible with exact mappings
- Specify .postapp format and CLI integration
- Add conformance and security regression tests

#### Outputs
- `docs/specs/07-SDK_DEVELOPER_EXPERIENCE.md`

---

### Loop 9: App Lifecycle Management Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/lifecycle-preplan-review-20260120-*.md`

**Key Feedback**:
1. Define complete state machine (install states + runtime states)
2. Installation pipeline with atomicity guarantees
3. Session lifecycle with bootstrap injection
4. Eviction handshake with graceful degradation
5. Data management and cleanup policies
6. Uninstall side effects (permissions, storage, sessions)
7. Update/upgrade strategies

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/08-APP_LIFECYCLE_MANAGEMENT.md`

**Specification Contents**:
- InstallState enum (Uninstalled/Installed/Disabled/Corrupted)
- AppRunState enum (Cold/Warm/Hot)
- Compound lifecycle state diagram
- InstalledApp struct with complete metadata
- Installation pipeline (Acquire→Verify→Stage→Commit→Post-install)
- Source types (Marketplace, LocalFile, Developer)
- Launch flow with session creation and bootstrap injection
- Close flow with CloseReason taxonomy
- Session lifecycle with Policy A (UNAUTHORIZED forces reload)
- Data management (uninstall, update, export)
- Shell commands and bridge methods
- SQLite schema for installed_apps and app_sessions
- 7 test scenario categories
- 10-phase implementation checklist

#### Post-Work Review
**Location**: `code-reviews/lifecycle-postwork-review-20260120-*.md`
**Rating**: 6/10

**Key Gaps Identified**:
1. **Missing transient states** - No Installing/Updating/Uninstalling/Launching/Closing states
2. **Rust→app event delivery conflicts** - Uses `app://lifecycle/*` events but bridge is app-initiated only
3. **Session expiry reload loop** - SDK reload on UNAUTHORIZED reloads same expired bootstrap
4. **Eviction handshake incomplete** - `resource.prepare_for_unload` not in Registry
5. **State machine semantic issues** - `show()` from Cold should be launch or error
6. **Single vs multi-instance unclear** - Webview label implies single-instance
7. **In-flight requests during close unspecified** - What happens to pending bridge calls
8. **Eviction vs Uninstall race conditions** - Need serialization/precedence rules
9. **Type mismatch** - `create_session()` signature vs return value inconsistent
10. **Permission cleanup incomplete** - Missing PendingAction and prompt queue cleanup on uninstall

**Improvements Needed (for revision pass)**:
- Add transient states or per-app operation locks
- Unify event delivery: use `events.subscribe(topic="lifecycle")` or injected dispatcher
- Define session refresh or rely on webview recreation
- Add `resource.prepare_for_unload` to Registry and SDK
- Constrain show() to Warm/Hot only, launch() for Cold→Hot
- Decide single-instance per app (recommended) or multi-instance with instance IDs
- Define closing policy for in-flight requests
- Add per-app lifecycle mutex with precedence (Uninstall > Upgrade > Close > Eviction)
- Fix create_session() signature
- Add PendingAction + prompt queue cleanup to uninstall flow

#### Outputs
- `docs/specs/08-APP_LIFECYCLE_MANAGEMENT.md`

---

### Loop 10: Package Trust & Updates Specification
**Timestamp**: 2026-01-20
**Status**: Complete

#### Pre-Work Review
**Location**: `code-reviews/package-trust-preplan-review-20260120-*.md`

**Key Feedback**:
1. Define normative manifest.json schema
2. Define what-is-signed and canonicalization rules
3. Specify trust store + key rotation + revocation mechanism
4. Design marketplace update metadata with downgrade/freeze resistance
5. Add install-time archive safety limits (zip bombs, symlinks, duplicates)
6. Coherent policy for permission escalation on update + prompt timing
7. Rollback vs downgrade prevention + revoked-installed-app behavior
8. Unify signing across apps (.postapp) and extensions (.postmod)

#### Subagent Work
**Agent**: Plan subagent
**Output**: `docs/specs/09-PACKAGE_TRUST_UPDATES.md`

**Specification Contents**:
- ZIP profile constraints (symlinks, traversal, bombs, duplicates)
- Canonical directory layout for .postapp and .postmod
- Normative PackageManifest with AppMetadata, PublisherBinding, CapabilitiesConfig
- FILES manifest for per-file integrity hashes
- SIGNATURE document with multi-signature support (publisher + marketplace)
- PublisherKeyId format and PublisherRecord for trust store
- Identity binding rules and key rotation mechanism
- TrustStore with platform roots, marketplace certificates, publishers
- Trust store SQLite schema
- Signature verification pipeline (5 phases)
- Canonicalization rules for signing
- Source-specific verification policy (Marketplace, LocalFile, Developer)
- UpdateManifest and VersionInfo for marketplace updates
- Anti-downgrade protection (highest_installed_version tracking)
- Anti-freeze protection (staleness checking)
- CapabilityChangeDetector for update-time escalation
- Publisher key change handling policy
- Revocation types (PackageVersion, PublisherIdentity, PublisherKey)
- Offline revocation policy
- Installed-but-revoked app handling
- RollbackPolicy and RollbackManager
- Launch-time integrity verification
- Measured install record SQLite schema
- ZIP bomb defense with compression ratio checks
- Symlink/hardlink rejection
- Pre-verification content isolation
- Shell commands for trust and update management
- 33 security-focused test scenarios
- 8-phase implementation checklist

#### Post-Work Review
**Location**: `code-reviews/package-trust-postwork-review-20260120-*.md`
**Rating**: 6/10

**Key Gaps Identified**:
1. **Canonicalization underspecified** - Custom JSON rules likely to diverge; use RFC 8785 JCS or canonical CBOR
2. **Signing payload fragile** - Colon-delimited format ambiguous; use structured CBOR/JSON encoding
3. **Timestamp backdating vulnerability** - Revoked keys can still sign with past timestamps; need compromise_time semantics
4. **Base64 encoding not normative** - Padded vs unpadded, standard vs URL-safe unspecified
5. **Identity/DHT trust chain unclear** - IID derivation and verification not end-to-end specified
6. **Marketplace attestation format undefined** - What is signed, by which root, not specified
7. **Publisher identity continuity** - Update from different IID not explicitly blocked
8. **Trust store integrity_hash not anchored** - Hash alone not tamper-resistant
9. **Path normalization edge cases** - Backslashes, drive letters, case-insensitive collisions
10. **RevocationSource schema missing** - Referenced but not defined
11. **Revocation authority ambiguous** - Who signs revocations for key compromise unclear
12. **UpdateManifest replay/freeze protection insufficient** - Need TUF-like timestamp/snapshot chain
13. **Rollback vs anti-downgrade conflict** - Allow only local backups, keep highest_version unchanged
14. **Equal version handling** - Same semver different content attack not addressed
15. **Package format not coherent with Domain 6** - FILES not mentioned in 08-APP_LIFECYCLE

**Improvements Needed (for revision pass)**:
- Adopt RFC 8785 JCS or canonical CBOR for canonicalization
- Define signature input as structured encoding with all relevant fields
- Add freshness requirements for signature timestamps
- Specify base64 encoding normatively
- Add "Identity Document Verification" section with IID derivation + rotation chain
- Define marketplace certificate signature payload and verification
- Enforce publisher IID continuity across updates (unless ownership transfer)
- Anchor trust store with compiled-in roots or OS keystore MAC
- Add comprehensive path normalization for cross-platform safety
- Define RevocationSource schema with types, signing requirements, update cadence
- Clarify revocation authority per RevocationReason (platform root for key compromise)
- Add TUF-like timestamp/snapshot or persist monotonic metadata timestamp
- Define rollback only to local backups, not revoked/vulnerable versions
- Require manifest_hash match for same-version reinstall
- Update Domain 6 to reference FILES and SIGNATURE JSON structure

#### Outputs
- `docs/specs/09-PACKAGE_TRUST_UPDATES.md`

---

### System Review #2
**Timestamp**: 2026-01-20
**Status**: Complete
**Rating**: 6/10 coherence

#### Review Scope
All specifications from Loops 1-10:
- Phase 0 Gating Spikes
- Shell Architecture + ADR-001, ADR-002
- App Sandbox & Isolation + ADR-003
- Resource Constraints
- Secure Bridge Protocol
- Protocol Registry
- Permission System
- SDK & Developer Experience
- App Lifecycle Management
- Package Trust & Updates

#### Review Location
`code-reviews/system-review-2-20260120-*.md`

#### Key Findings

**Coherent Aspects**:
- Multi-webview isolation consistently specified across all specs
- Single IPC entrypoint (`postbridge_invoke`) maintained
- Defense-in-depth layering (capabilities, label checks, CSP, rate limits)
- Resource model thorough and operationally realistic
- Comprehensive package trust model with signature verification

**Critical Inconsistencies (P0)**:
1. **Event delivery model mismatch** - 04/07 use long-poll, 02/03/06/08 describe push events to apps
2. **Method naming inconsistency** - 04 uses `storage.get`, 05/07 use `storage.v1.get`
3. **SDK envelope fields wrong** - 07 puts `idempotency_key` in params, not envelope
4. **Shell command naming drift** - 02 uses `app_launch`, 01/08 use `shell_launch_app`
5. **ADR numbering conflicts** - 01 claims ADR-003 is "Shell Navigation", 02/03 claim it's "Multi-webview"

**P1 Issues (Security/UX)**:
6. **Permission tier vs UI scope mismatch** - PromptAlways offers "Always allow" but tier forbids persistence
7. **TOCTOU error signaling missing** - No stable error code for "pending action not confirmed"

**P2 Issues (Package/Lifecycle)**:
8. **Package format misalignment** - 08 missing FILES manifest required by 09
9. **Shell event names inconsistent** - 01 vs 08 use different event naming

**P3 Issues (Process)**:
10. **ADR files referenced but not present** - Missing actual ADR documents

#### Remediation Plan (Priority Order)

**P0 - Must fix before coding core runtime:**
1. **Unify event delivery model** - Choose `events.*` long-poll for all app events, rewrite push event references as topics
2. **Make Protocol Registry (05) authoritative** - Update 04 to use versioned methods, remove duplicate MethodRegistry
3. **Fix SDK envelope correctness** - Generate UUIDv7 request IDs, put envelope fields at top level
4. **Normalize shell command names** - Standardize on `shell_*` prefix everywhere

**P1 - Security/UX correctness:**
5. **Align permission tiers with allowed grant scopes** - Add allowed_scopes to CapabilitySpec
6. **Add stable TOCTOU error signaling** - Define specific error code for pending action status

**P2 - Package/lifecycle hardening:**
7. **Update 08 to include 09 requirements** - Add FILES, canonicalization, ZIP constraints
8. **Unify shell event names** - Choose one taxonomy

**P3 - Process hygiene:**
9. **ADR reconciliation** - Create ADR directory with actual docs or remove references

---

## Specification Summary

### Completed Specifications

| Spec | File | Key Contents |
|------|------|--------------|
| Phase 0 Spikes | `PHASE_0_GATING_SPIKES.md` | 7 spikes for architecture validation |
| Shell Architecture | `01-SHELL_ARCHITECTURE.md` | Zustand state, shadcn/ui, security hardening |
| App Sandbox | `02-APP_SANDBOX_ISOLATION.md` | Multi-webview, postapp:// protocol, CSP |
| Resource Constraints | `03-RESOURCE_CONSTRAINTS.md` | Memory/CPU limits, LRU eviction, rate limiting |
| Bridge Protocol | `04-SECURE_BRIDGE_PROTOCOL.md` | CBOR encoding, session tokens, anti-replay |
| Protocol Registry | `05-PROTOCOL_REGISTRY.md` | Method specs, namespaces, version negotiation |
| Permission System | `06-PERMISSION_SYSTEM.md` | TOCTOU flow, capability catalog, prompt UI |
| SDK | `07-SDK_DEVELOPER_EXPERIENCE.md` | TypeScript SDK, CLI tooling, templates |
| App Lifecycle | `08-APP_LIFECYCLE_MANAGEMENT.md` | State machines, install pipeline, sessions |
| Package Trust | `09-PACKAGE_TRUST_UPDATES.md` | Signatures, trust store, revocation, rollback |

### Overall Review Scores

| Review | Rating | Notes |
|--------|--------|-------|
| System Review #1 | 7/10 | Good privilege boundary, naming drift |
| System Review #2 | 6/10 | Event delivery + method naming need unification |

### Next Steps
1. **Revision Pass**: Address P0/P1 issues across all specs
2. **ADR Creation**: Create actual ADR documents
3. **Schema Source of Truth**: Create `/schemas/` with CDDL + type definitions
4. **Implementation**: Phase 0 spikes first, then core runtime

---
