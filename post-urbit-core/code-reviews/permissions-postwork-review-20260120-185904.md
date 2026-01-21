## Rating: 6/10

## Strengths
- **Clear intent and threat framing**: Trust boundaries, non-goals, and key threats (spoofing, TOCTOU, spam) are explicit and mostly well-mitigated at a conceptual level.
- **Good core primitives**: `PermissionRecord`, `PendingAction`, audit events, and a concrete SQLite schema provide a solid foundation.
- **TOCTOU concept is correct**: Binding execution to a stored `params_cbor_sha256`, single-use `action_token`, and short expiry is the right shape for preventing parameter substitution.
- **UX requirements are actionable**: Shell-rendered prompts, queueing constraints, and “App says…” separation are implementable and align with a “trusted UI” model.

## Critical Gaps
1. **Spec conflicts with Bridge/Registry regarding permission method tiers**
   - In **04-SECURE_BRIDGE_PROTOCOL.md**, `permission.prepare_action` and `permission.execute_action` are registered as `PromptAlways`, which would recursively require prompting to prompt.
   - In **06**, they are `AlwaysGranted` (which is the only workable choice).
   - This needs to be reconciled across **04/05/06** or implementers will build incompatible behavior.

2. **Ambiguity: permission tier is method-level (Registry), but 06 treats it like capability-level**
   - **05**: `MethodSpec.permission_tier` is a property of the *method*.
   - **06**: pipeline prompts based on `method_spec.permission_tier`, but then fetches `CapabilitySpec.default_tier` and includes tier semantics that read like they apply to *capabilities*.
   - You must define precedence and meaning:
     - Is tier decided per method only (recommended, consistent with 05)?
     - Or can capabilities have tiers independent of method tier?

3. **Authorization pipeline incorrect/incomplete for `AlwaysGranted` methods with required capabilities**
   - Step 3 in §7 returns `Proceed` immediately for `AlwaysGranted`, skipping `required_capabilities`.
   - But other specs explicitly include required caps even for auto-granted domains (e.g., `storage.get` requires `storage:app` in 04).
   - You need either:
     - A rule that `AlwaysGranted` implies “all required capabilities are implicitly granted without persistence”, **and still must exist in registry**, OR
     - Remove required capabilities from AlwaysGranted methods (but that contradicts 04/05’s modeling).

4. **TOCTOU flow is incomplete for multi-capability and “method selection”**
   - `permission.prepare_action` returns a single `capability`, but methods can require **multiple** capabilities (current models allow `required_capabilities: Vec<String>`).
   - Missing rules:
     - If a method requires multiple capabilities, does prepare_action create one prompt containing all? one token per capability? how is partial approval handled?
     - How does the system compute `capability` in the result if there are multiple?
   - Also missing critical validation in prepare step:
     - It must verify the requested `method` is **not ShellOnly**, exists in registry, and is eligible for TOCTOU prompting (typically PromptAlways).
     - It should validate `params` against the method’s schema *before* showing a prompt to prevent prompt spam with garbage payloads.

5. **No defined bridge-level error contract for “prompt required / use TOCTOU”**
   - Bridge error taxonomy (04) has `PERMISSION_DENIED`, but no standardized “requires user prompt / requires prepare_action” signal.
   - 06’s runtime sequence says “Error: Requires permission.prepare_action”, but does not define:
     - Which `BridgeErrorCode` is returned (`PERMISSION_DENIED` vs `UNAUTHORIZED` vs `INVALID_REQUEST`) and
     - A stable `error.details` payload shape (e.g., `{ required_capabilities, promptable: true, recommended_flow: "permission.prepare_action" }`).
   - Without this, app developers can’t implement a consistent fallback.

6. **Shell-only execution path is not coherent with “only app webviews can call postbridge_invoke”**
   - 04’s `postbridge_invoke` explicitly rejects non-`app-` labels, implying shell cannot call bridge methods at all.
   - But 06 defines `permission.grant/revoke/list` as **bridge methods** (ShellOnly).
   - You must specify *how the shell invokes ShellOnly methods* (separate command? separate bridge entrypoint for shell? bypass bridge entirely?), otherwise ShellOnly APIs are underspecified.

7. **Persistence model conflicts with PromptAlways and with the SQLite uniqueness constraint**
   - 06 says PromptAlways grants are “Once (not stored)”, but:
     - `PermissionRecord` includes `Once`, and
     - `permission_records` has `UNIQUE(app_id, capability)`, which cannot represent repeated per-invocation “once” decisions anyway.
   - You need a clear statement:
     - PromptAlways decisions are **audit-only** (not in `permission_records`), or
     - Stored elsewhere with a timestamped history table.

8. **Revocation edge cases are underspecified**
   - What happens when a capability is revoked while:
     - An app session is active with a session-scoped grant?
     - A `PendingAction` exists for that capability and is already user-confirmed?
     - An `execute_action` is in-flight?
   - You need explicit ordering and atomicity rules (e.g., “revocation invalidates pending actions immediately; execute_action re-checks revocation at execution time”).

9. **Escalation logic incomplete for optional capabilities, denied records, and versioning**
   - The update check only considers `new_manifest.capabilities.required`, but:
     - Optional capabilities exist in UI model (`required: bool`) and are mentioned in implementation checklist.
     - Denied-but-previously-requested capabilities need rules (re-prompt on update? never re-prompt unless user initiates?).
   - No defined behavior for capability removal/downgrade (keep grants? auto-revoke? hide?).

10. **Security details missing for “stored encrypted params”**
   - `PendingAction.params_cbor` is “stored encrypted”, but no encryption scheme is specified:
     - key derivation / rotation
     - per-user vs per-install keying
     - nonce/AEAD mode, associated data (should bind app_id/session_id/method)
   - Without this, implementations will diverge and may store sensitive data in plaintext.

## Recommendations
1. **Resolve cross-spec inconsistencies (highest priority)**
   - Update 04/05 so `permission.prepare_action` and `permission.execute_action` are `AlwaysGranted`.
   - Define explicitly how shell invokes ShellOnly methods given 04’s app-only `postbridge_invoke` guard.

2. **Clarify the tier model (method-tier vs capability-tier)**
   - Recommended: tier is **method-level only** (as in 05), and capabilities provide metadata/risk/UI only.
   - If you keep `CapabilitySpec.default_tier`, specify precedence rules and whether method tier can override it.

3. **Fix the authorization pipeline to match Registry + required_capabilities**
   - Even for `AlwaysGranted`, verify required capabilities exist and are considered “implicitly granted” (no records), or remove caps from AlwaysGranted methods and document that as a registry invariant.

4. **Make TOCTOU fully spec’d**
   - Define:
     - Multi-capability prompting behavior (single prompt containing all missing caps, single action_token bound to full set).
     - Validation steps in `prepare_action` (method exists, not ShellOnly, correct tier, params schema valid, rate limit).
     - Exact `execute_action` checks: session/webview binding, expiration, revocation re-check, single-use semantics, and handler dispatch rules.

5. **Standardize the “prompt required” error response**
   - Choose a stable pattern, e.g.:
     - `code = PERMISSION_DENIED`, `retryable=false`
     - `details = { reason: "PROMPT_REQUIRED", capability: "...", flow: "permission.prepare_action" }`
   - Ensure it doesn’t become an oracle for sensitive capability existence beyond what registry introspection already allows.

6. **Specify revocation semantics and atomicity**
   - Define explicit behavior and ordering:
     - Revocation invalidates session grants immediately.
     - Revocation invalidates all matching `PendingAction`s (even confirmed ones).
     - `execute_action` must re-check permission state at execution time.
   - Define an events topic/payload for revocations consistent with the Events system (04), rather than `app://...` placeholders.

7. **Correct persistence for PromptAlways + add history if needed**
   - Keep `permission_records` as the *current effective state* (grant/deny for GrantOnce/Session/Persistent only).
   - Log all PromptAlways decisions in `permission_audit_log` (or add a `permission_decisions` append-only table).

8. **Complete escalation rules**
   - Specify behavior for:
     - Optional capability additions
     - Previously denied capabilities on update (auto-deny vs re-prompt policy)
     - Capability removal/downgrade (retain record but mark inactive? auto-revoke?).

9. **Define encryption-at-rest for pending actions (or remove the claim)**
   - Either fully specify AEAD encryption (recommended) or explicitly defer it and remove “stored encrypted” from the normative text to avoid a false sense of security.

If you want, I can propose concrete, cross-spec consistent method specs for `permission.*` (tiers, caps, schemas) and a precise TOCTOU state machine that fits the Bridge replay-cache behavior and the Registry model.
