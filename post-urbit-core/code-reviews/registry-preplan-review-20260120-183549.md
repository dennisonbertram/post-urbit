Below is what the **Protocol Registry domain** should specify so the bridge “MethodRegistry” becomes a production-grade, secure, evolvable contract. I’m framing this as **normative spec content** (what the document must contain) plus concrete design choices that fit your existing Bridge v1 envelope and threat model.

---

## 0) Scope, goals, and invariants (must be explicit)

**Purpose:** Define how bridge methods are **declared, loaded, versioned, authorized, discovered, validated, and deprecated**—without expanding the attack surface beyond `postbridge_invoke`.

**Non-negotiable security invariants (repeat from 04 + extend):**
- Apps **MUST NOT** dynamically register or override bridge methods.
- Method dispatch **MUST** be determined solely by backend-controlled registry state.
- Registry changes **MUST** be auditable and integrity-protected (hash + signatures for extensions).
- Introspection **MUST NOT** expose shell-only/private methods to app webviews.

---

## 1) Method registration lifecycle (built-in vs plugin-provided)

### 1.1 Registry composition model
Define the registry as a merge of **modules**:

1) **Core (built-in) module(s)**  
   - Compiled into the backend.
   - Always present, versioned with the platform release.
   - Own reserved namespaces (`bridge.*`, `storage.*`, …).

2) **Extension modules (plugin-provided)**
   - Installed separately (e.g., marketplace/native plugin packs).
   - Loaded by backend at startup (or controlled reload), never by apps.
   - Namespaced under an extension prefix (see §5).

**Spec must define:**
- Module identity (`module_id`, vendor/publisher, version, build hash).
- Load order and merge rules.
- Conflict policy (fail closed).

### 1.2 Registration phases (lifecycle)
A production-ready lifecycle usually needs these phases:

**A. Install-time validation (offline / before activation)**
- Verify signature / provenance.
- Validate manifest schema.
- Validate method namespace ownership.
- Validate schemas (params/results) and size limits.
- Validate capability declarations (no wildcards, no collisions).

**B. Activation (runtime registration)**
- Registry loads module and registers methods atomically.
- Any error ⇒ module not activated; registry remains in last-known-good state.

**C. Deactivation / uninstall**
- Methods removed from active registry.
- Calls become `INVALID_REQUEST` (unknown method) or `NOT_FOUND` depending on your existing taxonomy; pick one and standardize.

**D. Update**
- Must be atomic: either old module stays active or new one fully replaces it.
- Must define what happens to existing sessions (see §6: usually “sessions remain valid but new methods may appear/disappear”; or “registry change invalidates sessions”).

### 1.3 Conflict and override rules (important)
Specify strong “fail closed” rules:

- A module **MUST NOT** register a method name already registered by another module.
- A module **MUST NOT** register within reserved namespaces it doesn’t own.
- No “override”, “shadowing”, or “monkey patching” of built-ins in production mode.

This prevents malicious extensions from intercepting calls to `storage.get` etc.

---

## 2) Protocol versioning strategy (transport vs API vs schema)

You need **three version axes**, each with a different upgrade policy:

### 2.1 Transport/envelope version (Bridge Protocol)
This is your existing `bridge-request.v` and `bridge-response.v`.

**Spec should state:**
- `v` is **major-only** (breaking changes only).
- Backend advertises supported envelope versions (e.g., `[1]`, later `[1,2]`).
- Sessions negotiate and bind to a single envelope version.

If you don’t already have negotiation in 04, the registry spec should either:
- define a `bridge.negotiate` method (AlwaysGranted) used before session creation, or
- extend the shell-only session creation handshake to include supported versions and a chosen `v`.

### 2.2 Method/API versioning (per method)
Avoid breaking changes inside a stable method name. In production, you need an explicit strategy that’s enforceable.

**Recommended (fits your method-name regex):**
- Encode **method major** in the namespace as a segment:  
  `storage.v1.get`, `storage.v2.get`  
  This works with: `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$`

**Rules:**
- Within the same major (`v1`), changes must be backward compatible (additive/optional only).
- Breaking changes require new major (`v2`) and coexistence for a deprecation window.

If you want to preserve existing names (`storage.get`), specify that:
- methods without an explicit `.vN.` segment are **implicitly v1**, but new work should prefer explicit majors going forward.

### 2.3 Schema versioning (params/results)
Every method must bind to explicit schemas:

- `params_schema_id`
- `result_schema_id`

Schema IDs should be immutable identifiers, with a separate `schema_version` or semantic version.

**Practical rule:**
- If you make a non-breaking additive change → bump schema minor, keep same method major.
- If you make a breaking schema change → new method major (and usually new schema major).

---

## 3) Capability requirements per method (authorization contract)

### 3.1 MethodSpec must be normative and complete
The Protocol Registry spec should define the canonical `MethodSpec` fields (not just examples). At minimum:

- `method`: string (fully-qualified)
- `module_id`: string
- `stability`: `experimental | beta | stable | deprecated`
- `required_capabilities`: list of strings
- `permission_tier`: (`AlwaysGranted | GrantOnce | PromptAlways | ShellOnly`)
- `idempotent`: bool (ties into replay cache semantics)
- `timeout_ms`: u64 (ties into timeout config)
- `rate_limit_class` or explicit limits (ties into Domain 3)
- `max_request_bytes`, `max_response_bytes` (optional overrides under global caps)
- `params_schema_id`, `result_schema_id`
- `audit`: what to log (method name, timing, but never sensitive payload unless explicitly allowed)

### 3.2 Capability naming + ownership
Production-ready capability governance needs rules for:
- Reserved capability prefixes owned by platform (`storage:*`, `clipboard:*`, etc.)
- Extension capability namespace (see §5): e.g. `x:com.example.mod:read`

**Hard rules (align with your “non-wildcard” rule):**
- Apps MUST request specific capabilities (no `storage:*`).
- Modules MUST NOT mint capabilities in namespaces they don’t own.
- Prompted capabilities must include human-readable prompts (title/body) defined by the method/module so shell prompts are consistent and reviewable.

### 3.3 Conditional capabilities (if you need them)
Some methods’ required privileges depend on parameters (e.g., a path prefix). If you support this, specify a *capability evaluation function*:

- `required_capabilities`: unconditional
- `capability_selectors`: parameter-driven checks (defined declaratively if possible, or explicitly as “implemented in handler; must be documented and tested”)

Be careful: conditional capability logic must be audited because it’s a common confused-deputy source.

---

## 4) Method discovery / introspection APIs (without leaking sensitive surface)

Introspection is essential for third-party apps to adapt to versions, but it’s also fingerprinting surface.

### 4.1 Define a minimal, safe introspection suite
Add platform methods (likely under `bridge.*`) such as:

- `bridge.get_server_info` (AlwaysGranted)  
  Returns: supported envelope versions, registry hash, platform version, build info (non-sensitive).

- `bridge.list_methods` (AlwaysGranted or gated)  
  Returns **only methods callable by this session**, filtered by namespace/module/stability.

- `bridge.get_method_spec` (AlwaysGranted or gated)  
  Returns spec for one method **only if it is callable**.

- `bridge.list_capabilities` (optional)  
  Returns capabilities granted to the session + human-readable descriptions.

### 4.2 Critical privacy/security requirements
The spec must state:

- Introspection results MUST be **session-filtered**:
  - Exclude `ShellOnly`
  - Exclude methods requiring capabilities the session does not have (or include them but mark “not authorized”; choose one and standardize—filtering is safer).
- Introspection MUST be rate-limited.
- Introspection MUST support caching:
  - Include `registry_hash` (stable digest of active registry)
  - Optionally allow client to send `if_registry_hash` to get a cheap “not modified” response (saves CPU).

### 4.3 Schema access policy
Decide and specify whether apps can retrieve full schemas.

Common approach:
- By default, return only schema IDs + hashes.
- Provide `bridge.get_schema(schema_id)` **only in developer mode** or behind a capability like `bridge:introspect:schema`.

This reduces attacker ergonomics while still supporting tooling.

---

## 5) Extension points for third-party functionality (safe, governable)

You need to clearly separate:
- **Untrusted apps** (webviews) calling methods, vs
- **Trusted code** (backend + optionally signed native extensions) defining methods.

### 5.1 Namespace conventions (avoid collisions)
Define reserved namespaces and a vendor namespace:

- Platform reserved: `bridge.*`, `storage.*`, `system.*`, etc.
- Extension methods:  
  `x.<reverse_dns>.<module>.v{N}.*`  
  Example: `x.com_acme.payments.v1.invoice_create`

(Use `_` where your regex doesn’t permit `-`.)

### 5.2 Extension module manifest (production requirement)
A plugin must ship a manifest that is machine-validated and signed:

- `module_id` (reverse-DNS)
- `module_version` (SemVer)
- `min_platform_version`, `max_platform_version` (or compatibility range)
- list of methods + MethodSpec
- list of schemas (CDDL/JSON-schema) with hashes
- declared capabilities (owned by module) + prompt text where relevant
- signature block (publisher signing key, timestamp, transparency log pointer if you have one)

### 5.3 Execution sandboxing for extensions
If extensions are native code, your spec must at least declare the operational security stance:
- Are extensions fully trusted (same privilege as backend)?
- Are they sandboxed (WASM, seccomp, limited FS, etc.)?

Even if you don’t implement sandboxing initially, the spec should call out:
- threat implications,
- required audit logs,
- revocation mechanism (disable module, rotate keys, etc.).

---

## 6) Backward compatibility (rules, guarantees, and deprecation)

This is where production specs usually fail unless they’re explicit.

### 6.1 Compatibility contract
Define what “compatible” means at each level:

- **Envelope v1** compatible across backend releases that still support v1.
- **Method major** compatible: server supports `v1` methods until deprecated sunset date.
- **Within a method major**:
  - additive optional fields allowed,
  - new enum variants allowed only if clients tolerate unknowns (otherwise treat as breaking),
  - semantics must not change in a way that breaks security expectations.

### 6.2 Deprecation policy
Specify:
- How a method becomes deprecated (`stability=deprecated`, `deprecated_since`, `sunset_after`).
- Minimum support window (e.g., 2 platform releases or 180 days).
- How callers learn about deprecation (introspection field; *not* via new envelope keys unless you plan for it).

### 6.3 Coexistence policy
For breaking changes:
- Old and new majors coexist (`foo.v1.*` and `foo.v2.*`).
- Registry must allow both simultaneously, with distinct schemas and capabilities if needed.

---

## 7) Schema evolution and validation (runtime enforcement)

Your registry domain should make schemas **first-class**, not just documentation.

### 7.1 Canonical schema format
Pick one canonical representation and specify it:
- CDDL is a strong choice since you already use it.
- You may optionally generate JSON Schema / TypeScript types for tooling.

### 7.2 Runtime validation requirements
For each invoked method, backend must:
1) authenticate session/token (Domain 04)
2) authorize capabilities (registry)
3) validate params against the registered schema **before** executing handler logic
4) validate result against result schema **before** encoding response (prevents leaking unexpected structures)

### 7.3 Forward-compatibility vs strictness (important decision)
Because you have a strong “fail closed” posture, be explicit about unknown fields:

A practical production stance:
- Envelope keys: strict (unknown keys rejected) *or* provide a single extension key like `x` reserved for future use.
- Method params/results: strict **except** for a reserved extension map (e.g., allow optional field `x: { * text => any }`), so additive evolution can happen without opening “arbitrary unknown top-level keys”.

This gives you:
- predictable parsing,
- controlled extensibility,
- less accidental breakage.

### 7.4 Schema constraints beyond structure
Registry should support validation metadata that isn’t naturally expressed in CDDL alone, such as:
- max bytes for specific fields (e.g., blob chunk)
- string regexes (you already use for method name)
- numeric ranges
- collection length limits per method (in addition to global CBOR limits)

### 7.5 Registry integrity
Specify:
- `registry_hash = hash(canonical_sorted_registry_representation)`
- canonicalization rules (sort keys, stable ordering, stable encoding)
- include `registry_hash` in `bridge.get_server_info` and optionally bind it to the session handshake so clients can detect changes.

---

## What the Protocol Registry specification should contain (production-ready checklist)

1) **Terminology + threat model** specific to registry and extensions  
2) **Formal data model**: ModuleSpec, MethodSpec, SchemaSpec, CapabilitySpec  
3) **Namespace ownership rules** and conflict resolution (default deny)  
4) **Lifecycle**: install/validate/activate/deactivate/update + atomicity guarantees  
5) **Versioning**: envelope vs method major vs schema version + negotiation  
6) **Authorization contract**: capabilities, tiers, prompts, conditional rules  
7) **Introspection APIs**: method listing/spec retrieval + filtering + caching + rate limits  
8) **Extension system**: module manifests, signing, compatibility gates, revocation  
9) **Backward compatibility**: deprecation timelines, coexistence, sunset behavior  
10) **Schema validation**: canonical schemas + runtime validation order + extension fields policy  
11) **Operational requirements**: audit logs, metrics, deterministic registry hash, test vectors  
12) **Conformance tests**: registry merge, conflict rejection, introspection filtering, schema validation failures, version negotiation cases

If you want, I can draft the actual markdown spec skeleton (with CDDL for `module-manifest`, `method-spec`, and the `bridge.list_methods`/`bridge.get_method_spec` APIs) in the same style as `04-SECURE_BRIDGE_PROTOCOL.md`.
