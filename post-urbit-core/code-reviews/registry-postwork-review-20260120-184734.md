## Rating: 6/10

## Strengths
- Clear **core invariants** (no dynamic method registration by apps, fail-closed, namespace ownership, shell-only filtering) that are implementable and security-relevant.
- Good separation of concerns in the model: `ModuleSpec`, `MethodSpec`, `SchemaSpec`, `CapabilitySpec`, plus lifecycle state (`Pending/Active/Inactive/Updating`).
- Strong direction on **namespace governance** (reserved prefixes + reverse-DNS for extensions) and explicit non-override conflict behavior.
- Integrates key platform needs into method specs (timeouts, rate-limit class, payload caps, schema IDs), which is the right “single source of truth” shape.
- Registry integrity concept (hashing + audit log) is a solid starting point for tamper-evidence.

## Critical Gaps
1. **Bridge Protocol coherence: method naming/versioning is inconsistent**
   - Bridge spec examples/registers methods like `storage.get`, `storage.set`, etc., while the registry mandates versioned names like `storage.v1.get`.
   - This breaks:
     - dispatch consistency,
     - schema lookup by `params_schema_id/result_schema_id`,
     - deprecation/version coexistence claims.
   - You need one canonical method naming scheme across *all* specs.

2. **Bridge response schema mismatch (deprecation warnings not representable)**
   - Registry spec shows adding `response.warnings.push(...)`, but the Bridge Protocol’s `BridgeResponse` CDDL/Rust structs do **not** include `warnings`.
   - This is a hard correctness/implementability issue: either add `warnings` to the bridge envelope schema or remove the behavior.

3. **Extension “security boundary” is not fully specified (trust + execution model)**
   - The spec references signatures, Ed25519, and a future `handlers/handler.wasm`, but it does not define:
     - what exactly is signed (manifest only? manifest+schemas+wasm? whole archive?),
     - canonicalization rules for signing (JSON canonicalization, file ordering, newline normalization),
     - trust roots / public key distribution (marketplace CA? pinned keys? TOFU?),
     - revocation, key rotation, and author identity verification semantics,
     - how extension handlers execute (WASM sandbox permissions, host functions, resource limits, filesystem/network access, upgrade safety).
   - Without this, “extension modules” are not implementable securely—either they are metadata-only, or they execute code; both need explicit boundaries.

4. **Registration lifecycle is incomplete (update/remove/rollback/atomicity)**
   - The lifecycle diagram includes update/deactivate/remove, but only `install_extension()` is sketched.
   - Missing required semantics:
     - atomic update (all-or-nothing) and rollback on validation/activation failure,
     - what happens to in-flight requests during module deactivation/updating,
     - dependency resolution behavior (blocking activation vs partial activation),
     - persistence format/location and crash consistency guarantees,
     - compatibility handling when a module is deactivated but sessions still exist.

5. **Schema validation enforcement is underspecified**
   - The runtime validation flow is described, but critical implementation details are missing:
     - which CDDL/CBOR validation library and its security properties,
     - how `ciborium::Value` maps to CDDL types (esp. tags, byte/text distinctions),
     - strict-mode “unknown keys” enforcement is hand-wavy (`check_unknown_keys(&value, ...)` references `value` that isn’t defined in the snippet),
     - whether validation is performed **before** authorization/permission prompts (ordering matters for TOCTOU and prompt spoofing),
     - performance constraints/caching strategy for compiled schemas.

6. **Introspection APIs are incomplete / ambiguous**
   - `bridge.list_methods` claims “methods available to this session”, but the sample implementation only filters shell-only—not capability/permission-tier inaccessible methods.
   - This creates ambiguity:
     - Is listing meant to be “all public platform surface” or “currently invokable surface”?
   - Also missing practical necessities:
     - pagination/limits for method lists (extensions could add many),
     - ability to fetch capabilities and their prompt text (`CapabilitySpec`) directly,
     - module listing and module metadata retrieval,
     - schema retrieval APIs (or a statement that schemas are only accessible via `get_method_spec`).

7. **Registry integrity is tamper-evident, not tamper-resistant**
   - A hash alone doesn’t prevent an attacker with filesystem write access from modifying registry data and recomputing the hash.
   - The spec says “integrity-protected,” but does not specify:
     - a signed root hash anchored in an OS keystore,
     - an append-only hash chain for audit entries,
     - verification procedure at startup (what is trusted? where is the trusted value stored?).

8. **Rate limit class values are incomplete vs Bridge Protocol**
   - Bridge Protocol provides concrete limits (e.g., token bucket 50 rps sustained / 200 burst, max 16 concurrent).
   - Registry has `RateLimitClass` but does not define:
     - exact per-class numeric parameters (sustained/burst/concurrency),
     - whether limits are per-session, per-app, per-method, or global,
     - conflict resolution when method spec has per-method limits but class also implies defaults.

9. **Error mapping and “404” wording is inconsistent**
   - Registry spec says “returns 404 for shell-only from app sessions,” but Bridge Protocol error taxonomy expects `NOT_FOUND` or collapsed `UNAUTHORIZED` depending on policy.
   - You need one rule: hide existence (`NOT_FOUND`) vs deny (`UNAUTHORIZED`) and apply consistently.

10. **Method/Module namespace rules have small logical hazards**
   - `check_namespace_conflicts()` checks `method.method.starts_with(&format!("{}.", manifest.module_id))`; but `ModuleSpec` separately has `namespace_prefix`.
   - If those can diverge, you can end up with confusing/incorrect ownership rules. Either:
     - remove one field, or
     - define a strict invariant `namespace_prefix == module_id` for extensions, etc.

## Recommendations
1. **Unify naming/versioning across Bridge + Registry**
   - Decide: *all* methods are versioned (`storage.v1.get`) or versioning is optional.
   - Update:
     - Bridge Protocol examples, method registry tables, and the method-name regex expectations accordingly.
   - Add explicit behavior for unknown versions (e.g., `storage.v9.get` ⇒ `INVALID_REQUEST` with safe message).

2. **Fix the response envelope contract**
   - If you want deprecation warnings, add to CDDL + Rust structs, e.g.:
     - `? warnings: [* warning]` where `warning={code:text, message:text}`.
   - Otherwise remove warning behavior from registry spec.

3. **Fully specify extension package trust and signing**
   - Define:
     - signature input (recommended: hash of a deterministic manifest + hashes of all files, or a signed Merkle root),
     - canonicalization rules,
     - trust root / key distribution and rotation,
     - revocation mechanism (blocklist, CRL-style, transparency log, or marketplace API).
   - Explicitly state whether offline installation is allowed and what trust policy applies.

4. **Define extension execution boundaries (or explicitly defer)**
   - If extensions execute code (WASM), specify:
     - sandbox runtime, allowed host calls, resource limits (CPU/mem/time), I/O policy,
     - capability gating for host functions,
     - update compatibility guarantees for host ABI.
   - If they do *not* execute code in this phase, explicitly say extensions are **metadata-only** and how methods are actually handled.

5. **Complete lifecycle semantics**
   - Specify `update_extension()`, `remove_extension()`, `deactivate_module()`, dependency resolution, and atomic commit/rollback.
   - Define “snapshot” semantics: sessions should authorize/dispatch against a registry version/hash (e.g., pin per-session `registry_hash` or always use latest with safe transition rules).

6. **Make schema validation enforceable**
   - Define:
     - validation order: CBOR profile validation → envelope validation → auth/session validation → schema validation → handler execution → result schema validation.
     - strict-mode semantics precisely (unknown keys policy, extension key behavior, map key types).
     - schema compilation/caching approach and worst-case cost controls.

7. **Harden registry integrity**
   - Add a signed/anchored trust mechanism:
     - store trusted `registry_hash` in OS keystore, or
     - sign registry state with a platform key, or
     - maintain an append-only audit log with hash chaining + anchored head.
   - Define startup verification and failure mode (fail closed vs recovery).

8. **Clarify introspection contract**
   - Decide whether `list_methods` returns:
     - (A) all non-shell methods, or
     - (B) only methods the session is authorized to call (capabilities granted).
   - Add pagination and include capability discovery APIs (at least `bridge.list_capabilities` / `bridge.get_capability_spec`).

9. **Normalize error-hiding rules**
   - Choose and document a consistent policy for shell-only visibility:
     - return `NOT_FOUND` to hide existence, or
     - return `UNAUTHORIZED` to deny.
   - Ensure Bridge + Registry specs match.

If you want, I can propose a concrete set of “authoritative contracts” (single canonical MethodSpec + Bridge envelope extensions) that reconciles docs/specs/04 and docs/specs/05 with minimal churn.
