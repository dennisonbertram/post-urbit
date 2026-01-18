## Iteration 58: HOLISTIC REVIEW

### Cross-Document Consistency Check

- spec/03-messaging-sync/sync-protocol.md (§ “CBOR Schemas for Sync Messages”) vs spec/03-messaging-sync/sync-protocol.md (§ “Subscription and Push” examples): `document_id` is specified as **32-byte bstr** in the normative CBOR schemas, but the later “Wire format” examples incorrectly show **20-byte bstr** for `document_id`.
- Severity: **BLOCKING**
- Status: **CONFIRMED ISSUE**

- spec/00-shared/layer-integration.md (“Domain Separator Registry (Normative)”) vs spec/05-ux-packaging/app-distribution.md + spec/04-app-runtime/manifest-schema.md: layer-integration claims to enumerate **all cryptographic domain separators used across the Post-Urbit protocol**, but packaging introduces additional separators (`postapp-signature-v1`, `postnode-repo-v1`, `postnode-update-v1`) that are **not listed** in the registry.
- Severity: **MINOR**
- Status: **CONFIRMED ISSUE**

- spec/04-app-runtime/manifest-schema.md (UI icon guidance: “PNG, 256x256 recommended”) vs spec/05-ux-packaging/app-distribution.md (package layout: “icon.png 512x512 PNG”): conflicting “recommended/expected” icon size guidance.
- Severity: **MINOR**
- Status: **CONFIRMED ISSUE**

- spec/00-shared/layer-integration.md (“Device document … `device_transport_key` removed in v1”) vs spec/05-ux-packaging/node-daemon.md (“Device transport key (X25519)” present in key hierarchy): v1 text implies the field/key is unused/removed, while daemon architecture still lists it as part of the key hierarchy (even though later it notes it’s reserved).
- Severity: **MINOR**
- Status: **CONFIRMED ISSUE**

### Blocking Issues (B1, B2, etc.)

- **B1:** `document_id` byte length mismatch in Sync protocol examples  
  - `sync-protocol.md` must consistently specify `document_id` as **32 bytes** everywhere (including SYNC_SUBSCRIBE / push examples), or explicitly define when/why a 20-byte identifier would be used (currently no such alternate ID type exists).

### Minor Issues (M1, M2, etc.)

- **M1:** Domain separator registry in `00-shared/layer-integration.md` is labeled “all domain separators” but omits app distribution separators; either add them or narrow the stated scope (e.g., “protocol layers 01–03 + sync primitives”).
- **M2:** App icon size guidance inconsistency (256 vs 512). Align by making one canonical requirement (e.g., “512 preferred, 256 minimum”) across both docs.
- **M3:** Device transport key messaging: clarify that daemon may store/generate a device X25519 key for future use but it is **not part of v1 wire formats/handshake**.

### Verdict

[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — N blocking issues require fixes
