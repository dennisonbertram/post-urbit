## Iteration 66: HOLISTIC REVIEW

### Cross-Document Check
- **App Runtime method registry mismatch:** `system.get_deterministic_random` is specified as a v1 Host API method (and referenced by sandbox/determinism text + capability mapping) but is **missing from the authoritative method registry** in `spec/04-app-runtime/abi.md`.
- **App manifest schema vs distribution extensions:** `spec/05-ux-packaging/app-distribution.md` defines a `distribution` object added to `manifest.json`, but `spec/04-app-runtime/manifest-schema.md` does not include it nor state whether unknown fields MUST be ignored (forward-compat). This can lead to divergent validator behavior.
- **Device document signature verification key selection:** `spec/00-shared/layer-integration.md` allows verifying `signature_by_identity` with identity “current (or historical)” signing keys, while RFC-0001/RFC-0002 + `identity-document-schema.md` imply “current signing key”. This is cross-doc inconsistent (security/availability implications).
- **IDOC envelope canonicalization wording:** `spec/00-shared/layer-integration.md` and RFC-0001 require JCS-canonical JSON inside the IDOC envelope; `spec/02-identity-trust/identity-document-schema.md` wire-format section says “Canonical JSON” but doesn’t explicitly norm JCS for the envelope bytes.

### Blocking Issues (B1, B2, etc.)
**B1. App Runtime Host API: `system.get_deterministic_random` inconsistent across authoritative sources**
- **Where:**  
  - Present/defined: `spec/04-app-runtime/api-surface.md` (System API), `spec/04-app-runtime/wasm-sandbox.md` (determinism section), `spec/04-app-runtime/capability-system.md` (CAPABILITY_MAP)  
  - Missing: `spec/04-app-runtime/abi.md` (“Method String Format” registry; `abi.md` is declared authoritative for method registry)
- **Why blocking:** Two independent runtimes can implement different method sets, causing apps built to the spec to run on one node and fail with `METHOD_NOT_FOUND`/`NOT_IMPLEMENTED` on another.
- **Fix:** Add `system.get_deterministic_random` to `abi.md` method registry and clearly mark it **v1** (or, alternatively, mark it **reserved** everywhere and remove/soften the sandbox + API-surface claims).

**B2. App packaging: `distribution` manifest fields can be rejected by strict schema implementations**
- **Where:** `spec/05-ux-packaging/app-distribution.md` (“Manifest Extensions for Distribution”) vs `spec/04-app-runtime/manifest-schema.md` (schema) and `spec/04-app-runtime/interfaces.md` (`AppManifest` type).
- **Why blocking:** A `.postapp` whose `manifest.json` includes `distribution` (as described) may install on an implementation that ignores unknown fields but fail on an implementation that enforces a closed schema—creating non-portable packages.
- **Fix:** Either:
  1) Extend `manifest-schema.md` + `interfaces.md` to include `distribution`, **or**  
  2) Add a normative rule in `manifest-schema.md` that **unknown top-level fields MUST be ignored** (recommended for forward compatibility), and state whether unknown nested fields are ignored/rejected.

### Minor Issues (M1, M2, etc.)
**M1. Device document signature verification key ambiguity (current vs historical)**
- Recommend choosing one rule and aligning `layer-integration.md`, RFC-0001 §13, RFC-0002 §5.8, and `identity-document-schema.md`. If “current-only” is intended (stronger post-rotation security), add an explicit requirement to **republish device docs immediately on signing-key rotation** (and clarify expected caching/TTL behavior).

**M2. IDOC envelope JSON canonicalization should be explicit in `identity-document-schema.md`**
- Align wire-format section with RFC-0001 §6.2 / `layer-integration.md`: the JSON bytes inside the IDOC envelope should be **JCS-canonicalized (including `signatures`)**.

**M3. Sync operation ID string encoding not pinned in TS interfaces**
- `spec/03-messaging-sync/sync-protocol.md` defines wire IDs as 32-byte bstr and a hex-string display form; `spec/03-messaging-sync/interfaces.md` uses `string` without specifying encoding. Add a one-line normative mapping (e.g., lowercase hex, 64 chars) for interface-level IDs.

**M4. Repository manifest example signature type is inconsistent internally**
- In `app-distribution.md`, `repository.json` example shows `"signature": "<repository-operator-signature>"` (string) but later defines a structured signature object. Make the example match the defined structure.

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — 2 blocking issues require fixes

The spec is very close, but **B1** (authoritative Host API method registry mismatch) and **B2** (manifest schema vs distribution fields ambiguity) can cause real divergence between independent implementations, preventing portable apps/packages from functioning consistently across nodes. Fixing these two items should be straightforward and would likely clear the path to SPEC-COMPLETE.
