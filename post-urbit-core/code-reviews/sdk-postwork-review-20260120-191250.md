## Rating: 6/10

## Strengths
- Clear package split (`sdk` / `protocol` / `devtools`) with a sensible dependency graph; good foundation for long-term maintainability.
- Bootstrap validation + “token in memory only” + “reload on UNAUTHORIZED” aligns with the Bridge threat model (no silent refresh, no persistence).
- The “single Tauri command” assumption is correctly mirrored by the SDK transport (`postbridge_invoke` only).
- Namespace-based API surface (storage/system/events/permission/blob, plus React hooks) is ergonomic and approachable for typical app developers.
- Chunked blob helpers are practical DX and match the platform’s 256KB envelope constraint.
- Good coverage intent: test matrices are present and organized by concern (bootstrap / codec / integration / DX).

## Critical Gaps
1. **Method naming/versioning is inconsistent across specs (breaking correctness)**
   - Bridge spec registers `storage.get`, `storage.set`, etc. (`04-SECURE_BRIDGE_PROTOCOL.md`), but SDK uses `storage.v1.get`, `storage.v1.set` (`07-SDK...`).
   - Same mismatch risk exists for any namespace not shown fully (clipboard/external/resource). This will break generated `MethodName` unions, runtime routing, and docs.
   - Action: specs must share a single canonical method registry (including versioning policy).

2. **Envelope fields are implemented incorrectly in the SDK (`idempotency_key`, `trace_id`, `deadline_ms`)**
   - Bridge envelope defines top-level fields: `idempotency_key`, `trace_id`, `deadline_ms`.
   - SDK `ProtocolClient` instead injects `__idempotency_key` / `__trace_id` into `params`, which is not in the Bridge envelope schema and can collide with real params.
   - `InvokeOptions.timeout` is unused; should map to `deadline_ms` (client hint) and/or to a client-side abort.
   - This is a direct correctness/spec divergence.

3. **TOCTOU helper is brittle and underspecified (and may be unsafe-by-DX)**
   - `withPermission()` polls `permission.execute_action` and detects “not confirmed” via `error.message.includes('not confirmed')`—string matching is not a stable protocol contract.
   - There is no specified error code for “WAITING_FOR_USER / NOT_CONFIRMED / EXPIRED / DENIED” in the Bridge error taxonomy; everything collapses to a few generic codes.
   - The helper encourages repeated `execute_action` calls (polling) which can create unnecessary load and might interact badly with rate limits.
   - Missing a formal “pending action status” mechanism (either a `permission.get_action_status` method or an `events.*` topic emitted when prompts resolve).

4. **SDK “enforces TOCTOU for PromptAlways” is not actually enforced by the API surface**
   - The spec claims: “SDK does not allow direct calls” for PromptAlways methods, but:
     - `Transport.invoke(method: string, ...)` accepts any string.
     - Public `client.invoke` can be bypassed via `as any`.
     - No runtime guard exists to prevent `clipboard.write` / `external.open_url` from being called directly.
   - Server-side enforcement still protects security, but the SDK’s stated invariant is not met and DX will be confusing (some calls fail unless you know to use TOCTOU).

5. **Codec validation is incomplete and in places incorrect vs the platform limits**
   - String limit: SDK checks `value.length` (characters), but Bridge spec limits string length in **bytes** (UTF-8). This can undercount and exceed backend limits.
   - Typed arrays: `Uint8Array` is an object; `Object.keys(new Uint8Array(...))` yields indices and can trigger incorrect “collection too large” errors or deep traversal costs. You need explicit handling for `ArrayBuffer` / `ArrayBufferView`.
   - Decode-side limits: SDK enforces payload size but does not enforce max depth / max collection length on decode, and does not specify strict decode options (tags, floats NaN/Inf, etc.). `cbor-x` defaults may not match the strict CBOR profile.

6. **Error handling code samples are inconsistent and incomplete**
   - `handleResponse` shows `throw new BridgeError(error)` where `BridgeError` expects `(code, message, ...)` unless using `BridgeError.fromResponse(...)`. This won’t typecheck as written.
   - No canonical mapping rules are specified for converting `BridgeError` into `RateLimitedError` / `PermissionDeniedError` (the test table expects it, but the transport doesn’t implement it).

7. **Bootstrap validation does not cover several important invariants**
   - Does not validate `app_id` format/length per CDDL constraints.
   - Does not validate `issued_at_ms <= expires_at_ms`, skew bounds, or “expires not absurdly far in future”.
   - Does not validate `capabilities` type/shape/size (untrusted input from the app’s perspective—even if shell injects it, treating it defensively improves robustness).
   - Dev mode fields: `dev_nonce` is captured but no normative spec describes how it is used for inspector injection safely.

8. **Type generation pipeline is too high-level to be implementable without rework**
   - CDDL→JSON Schema→TS is plausible, but the hard parts are not specified:
     - How CBOR `bstr` maps to TS (`Uint8Array` vs `number[]`) consistently across all methods (currently mixed).
     - How to generate `MethodParams/MethodResult` as a single merged map across many schema files.
     - How to ensure Rust (`ciborium::Value`) ↔ TS types align (e.g., integer bounds, maps vs objects, bytes).
     - “Schema hash verification” is mentioned but not defined (where stored, how checked, failure behavior).

9. **CLI toolchain has major unspecified integration points**
   - No spec for `.postapp` package format, manifest schema, signing, or how `posturbit install` communicates with the running shell/backend.
   - `dev` command vs CSP constraints: you describe reload-based dev, but don’t specify the required directory layout, watcher protocol, or how source maps/debugging work.
   - `doctor` checks are unspecified (Tauri version? Rust toolchain? running shell instance?).

10. **Testing coverage is good as a checklist, but missing several high-risk tests**
   - No fuzz/property tests for CBOR decoding/encoding vs limits.
   - No conformance tests that compare SDK-generated envelopes against the Rust CDDL/decoder expectations.
   - No tests for TOCTOU denial/expiry states (beyond “not confirmed”), prompt rate limiting interactions, or event-driven confirmation.
   - No tests ensuring dev-only inspector cannot be enabled in production builds.

## Recommendations
1. **Unify the canonical method registry across specs (and define versioning policy)**
   - Decide: either `storage.get` everywhere or `storage.v1.get` everywhere.
   - Publish a single “Protocol Registry” source of truth that drives:
     - Rust method registry
     - CDDL schemas
     - Generated `MethodName/Params/Result`
     - SDK namespaces and docs

2. **Fix the envelope implementation to match the Bridge schema**
   - Put `idempotency_key`, `trace_id`, `deadline_ms` on the request envelope (not inside `params`).
   - Implement `InvokeOptions.timeout` as:
     - `deadline_ms` hint to server (clamped)
     - plus a client-side abort (AbortController) so UI doesn’t hang if the app no longer cares.

3. **Make TOCTOU a first-class protocol flow (stop polling by error message)**
   - Add one of:
     - `permission.get_action_status(action_token)` returning `waiting/approved/denied/expired/consumed`, or
     - an events topic: `events.subscribe(topic="permission.prompt_resolved", filter={prompt_id})`.
   - Add explicit error codes (or structured `details.kind`) for “not confirmed”, “expired”, “denied”, “consumed” so SDK logic is stable.

4. **Clarify and enforce “PromptAlways requires TOCTOU” at the SDK layer**
   - Option A (preferred DX): clipboard/external/resource “dangerous” namespace methods automatically run `prepare_action → await resolution → execute_action`.
   - Option B: keep low-level methods, but mark them `unsafe*` and require explicit use of `permission.withPermission`.
   - Either way, document the rule and make it hard to misuse.

5. **Harden the CBOR codec implementation**
   - Validate string lengths in UTF-8 bytes (not JS code units).
   - Treat `Uint8Array`/`ArrayBuffer` as byte strings and avoid iterating indices as object keys.
   - Add decode-side depth/collection limits (either via library options or by validating decoded structure).
   - Specify strict decode behavior to mirror backend rules (or explicitly state “backend is the source of truth; client only enforces size/depth for performance”).

6. **Normalize error mapping and make samples type-correct**
   - Define a single `fromBridgeResponseError()` mapping that returns specialized errors (`RateLimitedError`, `PermissionDeniedError`, generic `BridgeError`).
   - Ensure the `UNAUTHORIZED` reload path is well-defined (avoid reload loops; expose a hook/callback for apps to save state before reload if allowed).

7. **Make typegen a real, reproducible contract**
   - Specify exact mappings for CBOR types (e.g., `bstr => Uint8Array` everywhere).
   - Specify how schema files are merged and how `MethodName` is generated.
   - Define “schema hash verification”: what is hashed, where stored (in bootstrap? in protocol package?), and runtime failure behavior.

8. **Fill in CLI/tooling integration specs**
   - Define `.postapp` archive layout, manifest JSON schema, signing requirements, and install transport (file drop? local RPC? bridge method?).
   - Specify dev workflow mechanics: output directory conventions, watcher signals, dev/prod CSP deltas, source map support.

9. **Expand tests with conformance + security regression coverage**
   - Add SDK↔Rust protocol conformance tests (golden vectors of CBOR).
   - Add fuzz tests for CBOR decode and envelope parsing.
   - Add TOCTOU lifecycle tests for all states (approved/denied/expired/consumed).
   - Add tests ensuring dev-only inspector is gated by bootstrap and cannot be enabled by app code.

If you want, I can propose a concrete revised SDK request/response flow (including an event-driven TOCTOU confirmation API and updated envelope typing) that would bring this to ~8–9/10 with minimal surface churn.
