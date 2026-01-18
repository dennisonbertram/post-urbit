## Iteration 60: HOLISTIC REVIEW

### Cross-Document Consistency Check

1. **`recovery_proof.status` enum values disagree across canonical docs**
   - `spec/02-identity-trust/identity-document-schema.md` and `spec/02-identity-trust/interfaces.md` define `recovery_proof.status` as `pending|active|contested`.
   - `spec/02-identity-trust/recovery-mechanisms.md` also uses `pending|active|contested` (and states it is informational only).
   - **But** `spec/06-rfcs/RFC-0001-identity-document.md` §9.4 shows `status: "pending|active"` (missing `contested`), despite also discussing contestation elsewhere.
   - This is an enum/value-level mismatch across the Identity schema and its RFC.

2. **Stream framing presentation is uneven (potential reader confusion)**
   - Authoritative framing: `spec/00-shared/layer-integration.md` + `spec/06-rfcs/RFC-0002-transport.md` §6.3 require `StreamType (1 byte once) + repeated [u32be length + payload]`.
   - `spec/01-transport-connectivity/quic-integration.md` describes stream type byte then “Payload...” without reiterating the 4-byte frame length convention. Not directly contradictory, but inconsistent in stated wire convention.

3. **Terminology mismatch for initial session establishment**
   - `spec/06-rfcs/RFC-0003-messaging.md` renames the initial exchange to **2DH** while retaining domain separator `post-urbit-x3dh-v1`.
   - `spec/03-messaging-sync/double-ratchet.md` and `spec/03-messaging-sync/secure-envelope.md` still refer to **X3DH** in text.
   - Not a wire-format break (domain separator is consistent), but cross-doc naming is inconsistent.

4. **“Key valid at timestamp” logic is underspecified vs identity schema fields**
   - `spec/05-ux-packaging/app-distribution.md` and `spec/04-app-runtime/manifest-schema.md` verification flows say “verify author key was valid at signature timestamp”.
   - Identity schema provides `keys.signing.history[].valid_from/valid_until` as **sequence numbers**, plus `expires_at` timestamps, but does not directly map “which signing key was current at an arbitrary wall-clock timestamp” without also having historical IDOC versions available.
   - This is a spec-level cross-document contract ambiguity (algorithm reference vs available data).

### Blocking Issues (B1, B2, etc.)

**B1 — RFC-0001 `recovery_proof.status` enum mismatch**
- **Confirmed**: RFC-0001’s schema/example constraints for `recovery_proof.status` omit `contested`, while the layer schema/interfaces include it.
- **Fix**: Update RFC-0001 to match the canonical schema: `pending|active|contested` (or explicitly declare `status` as free-form/informational string and allow unknown values).

### Minor Issues (M1, M2, etc.)

**M1 — QUIC framing clarity drift**
- Add an explicit reference in `spec/01-transport-connectivity/quic-integration.md` that all streams use the RFC-0002 §6.3 length-prefixed frame format, to avoid implementers assuming raw/unframed payload after stream type.

**M2 — 2DH vs X3DH terminology**
- Normalize wording across `double-ratchet.md`, `secure-envelope.md`, and RFC-0003 (either “2DH (no prekeys)” everywhere or “(simplified) X3DH/2DH” everywhere). Keep domain separator unchanged.

**M3 — RFC-0001 “history key selection by validity window” phrasing**
- RFC-0001 §7.5 suggests matching historical keys by “validity window”, but validity fields are sequence-based and don’t align to message timestamps. Clarify alignment with the messaging-layer guidance (primarily `expires_at` and/or explicit historical IDOC lookup).

**M4 — App signing verification wording vs identity data**
- Adjust packaging docs to specify a mechanically implementable rule for selecting the verifying key (e.g., “try current/previous/history keys; require `timestamp <= expires_at` when using history entries; reject if key is revoked effective before timestamp”), or define required access to historical IDOC snapshots.

### Verdict

[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — N blocking issues require fixes
