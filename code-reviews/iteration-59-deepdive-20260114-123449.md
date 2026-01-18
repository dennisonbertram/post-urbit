## Iteration 59: DEEP DIVE

### Document-by-Document Analysis

- Document: spec/00-overview/success-criteria.md  
- Issues found: None

- Document: spec/00-overview/vision.md  
- Issues found: None

- Document: spec/00-shared/layer-integration.md  
- Issues found: None (internally consistent; byte counts and framing rules line up)

- Document: spec/00-shared/test-vectors.md  
- Issues found: Minor: some “truncated for display” hashes/inputs could be mistaken as complete values by implementers skimming; otherwise consistent.

- Document: spec/01-transport-connectivity/interfaces.md  
- Issues found: None

- Document: spec/01-transport-connectivity/nat-traversal.md  
- Issues found: Minor: `detect_nat_type()` pseudocode does not match typical NAT taxonomy behavior (it appears to conflate port-preservation with “full cone” vs “port restricted”); could mislead implementers even if intended as heuristic.

- Document: spec/01-transport-connectivity/overview.md  
- Issues found: None

- Document: spec/01-transport-connectivity/peer-handshake.md  
- Issues found: Minor: some fields are shown as `"<...>|null"` while tables mark them “No/optional”; could clarify whether field omission vs explicit `null` is required/allowed.

- Document: spec/01-transport-connectivity/quic-integration.md  
- Issues found: None

- Document: spec/01-transport-connectivity/relay-protocol.md  
- Issues found: None (wire sizes and token semantics consistent internally)

- Document: spec/02-identity-trust/caching-policy.md  
- Issues found: None

- Document: spec/02-identity-trust/identity-document-schema.md  
- Issues found: Minor: “Example Documents” include Base64 strings (not in `<…>` placeholder form) that likely do not satisfy stated decoded-size constraints (notably signatures). Examples should either be valid-length Base64 or clearly marked as placeholders.

- Document: spec/02-identity-trust/interfaces.md  
- Issues found: None

- Document: spec/02-identity-trust/key-rotation.md  
- Issues found: Minor: some JSON-like snippets use arithmetic (`sequence: N + 1`) rather than the on-wire required decimal string; can confuse readers about string vs number requirement.

- Document: spec/02-identity-trust/name-resolution.md  
- Issues found: None

- Document: spec/02-identity-trust/overview.md  
- Issues found: None

- Document: spec/02-identity-trust/recovery-mechanisms.md  
- Issues found: Minor: social recovery attestation verification pseudocode verifies signatures over the whole attestation object without explicitly applying the domain separator + JCS “remove signature field” rule (while contest docs do specify it). This is internally inconsistent in how signature inputs are treated within the same document.

- Document: spec/02-identity-trust/revocation.md  
- Issues found: Minor: `verifyKeyRevocation()` pseudocode references `verify(pubkey, revocation, sig)` without specifying the canonicalization/domain-sep removal rules for the revocation doc in this file (it defers to RFC-0001, but the local pseudocode reads like it signs raw JSON objects).

- Document: spec/03-messaging-sync/double-ratchet.md  
- Issues found: Minor: pseudocode uses variables like `nonce` without defining where it comes from (it’s implied to be the PUSE nonce); could add an explicit parameter/source to avoid ambiguity.

- Document: spec/03-messaging-sync/group-messaging.md  
- Issues found: Minor: `sender_key_encrypt()` example encrypts “directly” and returns an encoded blob, but later the spec states group messages are carried via PUSE with a group header extension; the example could be clearer that this step is “inside PUSE”.

- Document: spec/03-messaging-sync/interfaces.md  
- Issues found: None

- Document: spec/03-messaging-sync/overview.md  
- Issues found: None

- Document: spec/03-messaging-sync/secure-envelope.md  
- Issues found: Minor: the signature verification pseudocode calls `ed25519_verify(signing["current"], …)` without explicitly decoding Base64 → raw 32-byte pubkey; likely implied, but inconsistent with other places that explicitly say “decode then verify”.

- Document: spec/03-messaging-sync/sync-protocol.md  
- Issues found: Minor: `merge_lww()` compares `a.timestamp > b.timestamp` even though timestamps are HLC objects and a dedicated `hlc_compare()` is defined later; pseudocode should use `hlc_compare()` to avoid ambiguity/incorrect implementations.

- Document: spec/04-app-runtime/abi.md  
- Issues found: None

- Document: spec/04-app-runtime/api-surface.md  
- Issues found: Minor: mixed naming conventions inside “TypeScript” shapes (e.g., `has_more`) could be confused with the “TS camelCase vs wire snake_case” rule; consider consistently labeling these as wire schemas or TS schemas.

- Document: spec/04-app-runtime/capability-system.md  
- Issues found: None

- Document: spec/04-app-runtime/interfaces.md  
- Issues found: None

- Document: spec/04-app-runtime/manifest-schema.md  
- Issues found: None

- Document: spec/04-app-runtime/overview.md  
- Issues found: None

- Document: spec/04-app-runtime/wasm-sandbox.md  
- Issues found: None

- Document: spec/05-ux-packaging/admin-ui.md  
- Issues found: Minor: CSP note claims Tailwind needs `unsafe-inline`; Tailwind typically doesn’t require runtime inline styles if built statically—could be clarified (security posture impact).

- Document: spec/05-ux-packaging/app-distribution.md  
- Issues found: Minor: “distribution” manifest extensions are introduced “beyond manifest schema”; ensure this is explicitly permitted by manifest-schema.md validation rules (currently it doesn’t mention allowing/denying unknown top-level fields).

- Document: spec/05-ux-packaging/deployment.md  
- Issues found: None

- Document: spec/05-ux-packaging/interfaces.md  
- Issues found: None

- Document: spec/05-ux-packaging/node-daemon.md  
- Issues found: None

- Document: spec/05-ux-packaging/observability.md  
- Issues found: None

- Document: spec/05-ux-packaging/overview.md  
- Issues found: None

- Document: spec/06-rfcs/RFC-0001-identity-document.md  
- Issues found: Minor: §7.3 bootstrap pseudocode calls `dht_fetch_sequence(iid, sequence=0)` which is not otherwise defined in this RFC and can be misread as “DHT supports sequence queries”; later sections instead define a separate genesis keyspace (`post-urbit:genesis:`). Clarify the fetch primitive.

- Document: spec/06-rfcs/RFC-0002-transport.md  
- Issues found: None

- Document: spec/06-rfcs/RFC-0003-messaging.md  
- Issues found: **BLOCKING (confirmed):** (1) group decryption algorithm is off-by-one vs the specified iteration semantics; (2) mailbox URL canonicalization code contradicts the canonicalization rules (and would break signature reproducibility).

- Document: spec/progress.md  
- Issues found: Minor: claims “no contradictions between specs” despite remaining internal inconsistencies noted above (examples/pseudocode issues).

### Blocking Issues (B1, B2, etc.)
- **B1:** RFC-0003 §6.7 `group_decrypt()` advances the sender-key chain incorrectly (off-by-one).  
  - Encryption sets `header.iteration` to the count after a single `kdf_sender_key()` step (message key corresponds to that iteration).  
  - Decryption does: `while local_iteration < header.iteration: kdf_sender_key(discard)` then performs an additional `kdf_sender_key()` for the message key, producing the *next* iteration’s key.  
  - Result: correct decryption fails for in-order messages; implementations diverge.  
  - Document: `spec/06-rfcs/RFC-0003-messaging.md`

- **B2:** RFC-0003 §7.3 mailbox URL canonicalization is internally inconsistent and non-reproducible.  
  - Rules specify lowercasing scheme/host, but the reference `canonicalize_mailbox_url()` implementation lowercases the entire URL (`url.lower()`), incorrectly lowercasing the path (path is case-sensitive in general) and potentially changing the signed bytes across implementations.  
  - This breaks mailbox token signature verification determinism.  
  - Document: `spec/06-rfcs/RFC-0003-messaging.md`

### Minor Issues (M1, M2, etc.)
- **M1:** `spec/01-transport-connectivity/nat-traversal.md` NAT-type detection pseudocode appears incorrect for standard NAT classifications; clarify as heuristic or fix logic.
- **M2:** `spec/02-identity-trust/identity-document-schema.md` “Example Documents” likely violate stated Base64 decoded-size constraints (notably signatures); mark as placeholders or replace with valid-length Base64.
- **M3:** `spec/02-identity-trust/recovery-mechanisms.md` social recovery attestation signature verification pseudocode omits explicit domain-sep + JCS-without-signature treatment while contest docs include it; align.
- **M4:** `spec/03-messaging-sync/sync-protocol.md` `merge_lww()` compares HLC objects directly instead of using the defined comparator `hlc_compare()`.
- **M5:** `spec/04-app-runtime/api-surface.md` mixes wire-style snake_case fields inside “TypeScript interfaces”; could confuse implementers about canonical casing.
- **M6:** `spec/06-rfcs/RFC-0001-identity-document.md` bootstrap pseudocode references an undefined DHT “fetch by sequence” primitive; align with the defined `post-urbit:genesis:` record.

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — N blocking issues require fixes
