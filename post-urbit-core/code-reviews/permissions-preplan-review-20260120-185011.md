## 1) Required Sections (MUST cover)

### 1.1 Overview & Goals
- Purpose: govern **capabilities/permissions** for sandboxed apps invoking backend bridge methods.
- Non-goals (explicitly): OS-level permissions (camera/mic) are disabled via `Permissions-Policy` and not exposed unless later introduced.
- Trust boundaries: shell (trusted), backend (trusted), app webviews (untrusted).

### 1.2 Terminology & Normative Language
Define (and use consistently):
- **Capability** (string token minted by registry; e.g., `clipboard:write`)
- **Permission** (user’s decision for an app + capability, possibly constrained)
- **MethodSpec** (from Protocol Registry; includes `required_capabilities` and `permission_tier`)
- **Grant scope**: once / session / persistent
- **Constraint**: parameterized limitation on a capability (e.g., only allow certain URL origins)
- RFC-style MUST/SHOULD/MAY language

### 1.3 Capability Catalog & Semantics (Source of truth + meaning)
The spec must define:
- Where capability definitions live: **Protocol Registry `CapabilitySpec`** is canonical for identifiers + UI strings + risk level + default tier.
- The **semantic meaning** of each capability (what it allows the backend to do), including:
  - Data access scope (app-scoped vs user-scoped vs system)
  - Side effects (opening external URLs, writing clipboard, etc.)
  - Whether constraints exist and what they mean

Actionable structure:
- A table for **core capabilities** (initial platform set), including at minimum those referenced today:
  - `storage:app` (app-scoped storage CRUD)
  - `system:identity:read`
  - `external:open_url`
  - `clipboard:write`
  - plus placeholders for future (`contacts:read`, `files:read`, etc.) with “not implemented” status.
- For each capability define: **risk**, **default tier**, **prompt text**, **audit fields**, **allowed constraints**.

### 1.4 Permission Tiers: Contract with `PermissionTier` (Registry)
Specify exact semantics of the existing enum:
- `AlwaysGranted`: never prompts; always available to apps (but still enforced by backend capability checks).
- `GrantOnce`: user decision is persisted (allow or deny) until changed.
- `PromptAlways`: requires user consent **per execution** (no silent reuse).
- `ShellOnly`: never granted to apps; only callable from shell sessions/webviews.

Also specify:
- How tier interacts with constraints (e.g., `external:open_url` may be “PromptAlways” but consent may produce a constrained one-time allowance).

### 1.5 Permission State Model (What can be stored)
Define:
- Possible states: `Unknown` (not yet decided), `Granted`, `Denied`.
- Scope dimension: `Once`, `Session`, `Persistent` (even if some tiers disallow persistence, the model should represent it).
- Precedence rules (deny vs allow; constraints vs unconstrained; “most restrictive wins” vs “most recent wins”).

### 1.6 Request / Grant / Revoke APIs (Bridge methods)
The spec must define the **bridge-facing** permission APIs (these are listed in the registry docs and should be nailed down):
- `permission.check`
- `permission.grant` (likely shell-only or guarded; define who can call)
- `permission.revoke` (likely shell-only or guarded)
- `permission.prepare_action`
- `permission.execute_action`

For each: params/result schemas, error codes, idempotency, rate limits, and who is authorized to call.

### 1.7 User Consent UX Requirements (Prompting)
Define **non-bypassable** UX constraints:
- Prompts are rendered by **shell** (trusted UI), never by apps.
- Prompt must display:
  - App identity (app_id, display name, publisher if available)
  - Capability display name + description (from `CapabilitySpec`)
  - The **exact action parameters** being authorized (e.g., the URL host)
  - Choice set (Allow once / Allow this session / Always allow / Deny) depending on tier & risk policy
- Anti-spam requirements: prompt rate-limits, prompt deduplication, and cancellation behavior.

### 1.8 Persistence, Storage, and Migration
Define:
- Where decisions live (backend-controlled store, integrity protected).
- How decisions are keyed (user/profile + app_id + capability + constraint hash).
- Migration/versioning strategy when registry updates change capability metadata.

### 1.9 Inheritance & Escalation Rules
You must explicitly define:
- Whether capabilities **imply** other capabilities (default: no implicit inheritance).
- How upgrades (app update requesting more capabilities) are handled.
- Whether constraints can be broadened without a new prompt (generally: **no**).

### 1.10 Auditing & Observability
Define required logs/events:
- Every grant/revoke/deny/prompt decision logged with correlation IDs (bridge request id/trace_id).
- Optional user-facing “consent receipts”.

---

## 2) Data Models (key structs/enums)

These should be “backend truth” models; keep UI models separate.

### 2.1 Core Types

```rust
pub type AppId = String;         // reverse-DNS
pub type Capability = String;    // e.g., "clipboard:write"
pub type UserId = String;        // if multiple local profiles exist
pub type SessionId = String;
pub type ActionId = String;      // UUID
```

### 2.2 PermissionTier (imported from Registry)
Use the registry’s enum; don’t redefine semantics elsewhere.

### 2.3 Permission Decision + Scope

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
  Granted,
  Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantScope {
  Once,       // single execution (consumed)
  Session,    // valid for session_id lifetime
  Persistent, // stored until revoked
}
```

### 2.4 Constraints (capability-specific)
Make constraints explicit and typed; avoid “freeform JSON” without validation.

```rust
#[derive(Debug, Clone)]
pub enum CapabilityConstraint {
  ExternalOpenUrl { allowed_hosts: Vec<String>, allowed_schemes: Vec<String> },
  ClipboardWrite { max_bytes: u32 },
  // Future: FilesRead { roots: Vec<PathBuf> }, ContactsRead { fields: ... }
}
```

### 2.5 Stored Permission Record

```rust
#[derive(Debug, Clone)]
pub struct PermissionRecord {
  pub user_id: UserId,
  pub app_id: AppId,
  pub capability: Capability,

  pub decision: PermissionDecision,
  pub scope: GrantScope,

  pub constraint: Option<CapabilityConstraint>, // None = unconstrained

  pub granted_at_ms: u64,
  pub expires_at_ms: Option<u64>, // required for Once/Session; optional for Persistent

  pub granted_by: GrantSource, // install-time, runtime prompt, admin policy
  pub registry_hash_at_decision: String, // tie decision to registry state
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantSource {
  InstallPrompt,
  RuntimePrompt,
  ShellSettings,
  AdminPolicy,
}
```

### 2.6 Pending Action (TOCTOU-safe execution token)
This is central to `prepare_action` / `execute_action`.

```rust
#[derive(Debug, Clone)]
pub struct PendingAction {
  pub action_id: ActionId,
  pub session_id: SessionId,
  pub app_id: AppId,

  pub capability: Capability,
  pub method: String,                 // e.g., "clipboard.write"
  pub params_cbor_sha256: [u8; 32],   // bind exact params

  pub created_at_ms: u64,
  pub expires_at_ms: u64,             // short TTL (e.g., 60s)

  pub status: PendingActionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingActionStatus {
  WaitingForUser,
  Approved,   // approval stored but not yet executed
  Denied,
  Consumed,   // executed exactly once
  Expired,
  Revoked,    // permission revoked while pending
}
```

### 2.7 Policy / Inheritance (explicit, registry-controlled)
If you support implication, put it in registry-controlled specs:

```rust
pub struct CapabilityPolicy {
  pub capability: Capability,
  pub implies: Vec<Capability>, // default empty
  pub allow_persistent: bool,   // per capability
  pub allow_session: bool,      // per capability
  pub allow_constraints: bool,  // per capability
}
```

---

## 3) Critical Flows (lifecycle + prompt flows)

### 3.1 Authorization Pipeline (where permission sits)
Define a single, testable order of operations for every bridge request:

1. **Derive app identity** from webview label (`app-{app_id}`) (already required).
2. Validate session/token (already required).
3. Parse CBOR + validate envelope + schema (registry schemas).
4. Look up `MethodSpec` in **Protocol Registry**.
5. Reject `ShellOnly` if not shell session.
6. Check **capabilities** required by method:
   - If missing/denied → `PERMISSION_DENIED`.
7. If tier requires prompting, enforce **TOCTOU**:
   - Must use `permission.prepare_action` + shell prompt + `permission.execute_action`.
8. Execute handler; audit.

### 3.2 Install-time permission granting (GrantOnce)
When an app is installed or updated:
- Collect requested capabilities from the app manifest (or registry mapping).
- For each capability:
  - `AlwaysGranted`: auto-add to app’s granted set (no prompt).
  - `GrantOnce`: prompt user during install/update OR mark as “needs decision on first use” (choose one approach and standardize).
  - `PromptAlways`: no install-time prompt (decision occurs per action).
- Record decisions in Permission Store (persistent grants/denials).

### 3.3 Runtime permission request (PromptAlways / first-use GrantOnce)
**Do not** allow an app to trigger a raw OS dialog; everything must go through shell.

Recommended two-step for any prompted action:

**Step A: prepare**
- App calls: `permission.prepare_action` with:
  - `capability`
  - `method` it intends to call (or a platform-defined action type)
  - exact `params`
  - optional UI-safe “reason” field (strictly limited and sanitized, or omit entirely)
- Backend:
  - validates session, method, and that method is eligible for prepare/execute (PromptAlways or undecided GrantOnce).
  - creates `PendingAction` binding `(session_id, app_id, params hash, TTL)`.
  - emits shell event: `app://permission/prompt_requested` containing `action_id` and a **backend-generated** prompt model (no app-controlled strings beyond safe fields).

**Step B: prompt**
- Shell displays prompt, user chooses:
  - Allow once (consumes action only)
  - Allow session (if policy allows)
  - Always allow (only for `GrantOnce` or capabilities that permit persistence)
  - Deny
- Shell calls backend `permission.execute_action` with `(action_id, user_decision, chosen_scope)`.

**Step C: execute**
- Backend verifies:
  - action exists, not expired, belongs to `(session_id, app_id)`, not consumed
  - decision is valid for tier/policy
  - for “Always allow” update Permission Store (persistent)
  - for “Session allow” create session-scoped grant
  - then executes the underlying method/action with the **original prepared params** (not caller-provided).
- Mark PendingAction consumed and emit audit entry.

### 3.4 Revocation
Define revocation effects precisely:
- User revokes a persistent grant in shell settings:
  - Permission Store updated immediately.
  - Any session-scoped grants for that capability are removed.
  - Any PendingActions depending on that capability transition to `Revoked`.
- Subsequent calls fail with `PERMISSION_DENIED`.

### 3.5 Escalation on update (new capabilities)
When app version changes:
- Newly requested `GrantOnce` capabilities require explicit user decision (prompt).
- Previously denied capabilities remain denied unless user changes settings.
- Broader constraints (e.g., more allowed hosts) require a new prompt and new record version.

---

## 4) Integration Points (Bridge, Registry, Sandbox)

### 4.1 Bridge Protocol integration
- Permission methods are just normal bridge methods, subject to:
  - CBOR strict validation
  - session binding (webview label)
  - replay cache
  - rate limiting
- `permission.execute_action` must be **non-idempotent** and protected against replay:
  - either exclude it from replay caching OR ensure replay returns cached “already consumed” safely (explicitly specify behavior).

### 4.2 Protocol Registry integration
- Registry provides:
  - `MethodSpec.required_capabilities`
  - `MethodSpec.permission_tier`
  - `CapabilitySpec` (display name/description/risk/default tier)
- Permission System must **not** invent capabilities at runtime; it only enforces registry-defined ones.
- Introspection:
  - `bridge.list_methods` should only list methods the session can actually call:
    - must reflect current granted capabilities
    - must never expose `ShellOnly` to apps (already required in registry spec)

### 4.3 App Sandbox integration
- Prompts must be displayed by shell webview only (trusted), not in app webview:
  - prevents in-app prompt spoofing
- External effects:
  - `external.open_url` must route through shell-controlled external intent handling (as described in Sandbox spec).
- Navigation/CSP limitations are not a permission mechanism; permissions must still be enforced in backend.

---

## 5) Security Considerations (attack vectors to address)

### 5.1 Prompt Spoofing / UI Redress
- App cannot render “native-like” permission prompts that the user might confuse for shell prompts.
Mitigations:
- Shell prompts must be visually distinct and include trusted chrome.
- Consider OS-native dialogs for critical permissions.

### 5.2 TOCTOU and Parameter Substitution
Risk: app requests permission for benign params then executes with different params.
Mitigation:
- Strict prepare/execute with `params_cbor_sha256` binding; execute uses stored params only.

### 5.3 Confused Deputy
Risk: app tricks shell/backend into performing privileged action under shell authority.
Mitigation:
- All permission checks use app identity from infrastructure (webview label/session).
- Shell-only methods remain `ShellOnly` regardless of any app capability state.

### 5.4 Consent Phishing via App-Controlled Text
Risk: app supplies misleading “reason” strings in prompts.
Mitigation:
- Prefer registry-controlled descriptions only.
- If any app-supplied rationale is allowed, constrain length, sanitize, and label clearly as “App says: …”.

### 5.5 Prompt Spam / Denial-of-UX
Mitigation:
- Rate-limit `permission.prepare_action` per session/app.
- Coalesce identical pending prompts.
- Enforce “must be foreground” policy: only allow prompting when app webview is active (tie into lifecycle state).

### 5.6 Permission Store Tampering
Mitigation:
- Store integrity: hash + OS keystore-encrypted key, or signed records.
- Audit log with append-only semantics (at least best-effort).

### 5.7 Inheritance / Escalation Bugs
Mitigation:
- Default deny; no implicit inheritance unless registry explicitly defines `implies`.
- Deny overrides allow when conflicts exist (specify precedence).

### 5.8 Replay / Duplicate Execute
Mitigation:
- `execute_action` must be single-consumption; replays return safe error (`CONFLICT` or `PERMISSION_DENIED`) without re-executing side effects.

---

## 6) Test Scenarios (acceptance criteria)

Organize as: **Given / When / Then**. Include at minimum:

### 6.1 Core Authorization
1. **Missing capability**
   - Given session lacks `clipboard:write`
   - When app calls `clipboard.write`
   - Then `PERMISSION_DENIED` (no side effects)

2. **ShellOnly enforcement**
   - Given app session
   - When app calls `shell.launch_app`
   - Then `UNAUTHORIZED`

3. **AlwaysGranted**
   - Given app session
   - When app calls `storage.set` requiring `storage:app`
   - Then success without prompt

### 6.2 PromptAlways (TOCTOU)
4. **Prepare creates pending action**
   - When app calls `permission.prepare_action` for `external.open_url` with url A
   - Then PendingAction exists, shell prompt event emitted

5. **Execute binds params**
   - Given PendingAction prepared with url A
   - When app attempts to execute with url B (or any mismatch)
   - Then execution is rejected (no external open)

6. **Execute single-use**
   - Given a consumed action_id
   - When `permission.execute_action` is replayed
   - Then no re-execution occurs; deterministic error or cached safe response

### 6.3 Persistence vs Session
7. **GrantOnce persists**
   - User chooses “Always allow” for `system:identity:read`
   - Restart app/session
   - Call `system.get_identity`
   - Should succeed without prompting

8. **Session grant expires**
   - User chooses “Allow this session”
   - New session created
   - Call same method
   - Prompt required again (or denied if policy)

### 6.4 Revocation
9. **Revocation immediate effect**
   - Given persistent grant exists
   - When user revokes in settings
   - Then subsequent calls return `PERMISSION_DENIED`
   - And any pending actions are revoked

### 6.5 Update / Escalation
10. **New capability requires new consent**
   - App update adds `clipboard:write`
   - Then previous grants remain; new one is `Unknown` and must be prompted on first use/install

11. **Constraint broadening requires consent**
   - Previously allowed host `example.com`
   - App requests `*.example.com` or `other.com`
   - Must prompt again; old constrained grant does not authorize broader request

### 6.6 Abuse Resistance
12. **Prompt spam limited**
   - App calls `permission.prepare_action` 100x rapidly
   - Must trigger rate limiting (`RATE_LIMITED`) and not freeze shell UI

13. **Background prompting blocked**
   - When app is not foreground (Warm/Cold/Hidden)
   - Then `permission.prepare_action` is rejected or queued by policy (choose and specify)

---

If you want, I can also propose a concrete initial **capability catalog** (names + tiers + constraints) aligned with the current bridge methods (`external.open_url`, `clipboard.write`, `system.get_identity`, storage APIs) so the subagent has a ready-made table to drop into the spec.
