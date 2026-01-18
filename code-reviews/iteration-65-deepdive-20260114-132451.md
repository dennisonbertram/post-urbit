## Iteration 65: DEEP DIVE

### Document Analysis
1. **UUID byte encoding is not specified normatively (PUSE `message_id`, receipts, reply references, and Sync `document_id` UUID form).**  
   Multiple places say “UUID v4” / “UUID bytes” / UUID strings in plaintext fields (e.g., receipts’ `message_ids`, `reply_to`) but **nowhere defines the canonical mapping between:**
   - 16-byte on-wire UUID fields (PUSE header `message_id`; sync “UUID padded to 32 bytes”),
   - and their string form (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`).  
   Different language runtimes treat UUID endianness differently (notably some Windows/.NET APIs use mixed-endian layouts). Without a canonical mapping, two correct implementations can disagree about which string corresponds to which 16 bytes, breaking cross-implementation threading/reply/receipt correlation and UUID-based sync document IDs.

2. **Device document verification key selection is inconsistent across docs (current-only vs current-or-historical).**  
   - `spec/00-shared/layer-integration.md` suggests verifying `signature_by_identity` with identity’s current **or historical** signing key.  
   - `RFC-0001` DHT verification rules for device docs imply verification using the **current** signing key.  
   This is mostly a security/clarity issue (accepting historical keys expands acceptance), but the mismatch can mislead implementers.

3. **Recovery attestation verification pseudocode omits the domain-separated signature construction in some non-RFC docs.**  
   `recovery-mechanisms.md` includes signing rules, but some pseudo-verifiers read as “verify signature over the object” rather than “domain separator + JCS(without signature field)”. This is likely not harmful if implementers follow RFC-0001, but it’s a recurring footgun.

4. **Base32 normalization language varies (“reject uppercase” vs “normalize then compare”).**  
   Most normative text says lowercase-only canonical; some snippets mention normalization. This is non-blocking if producers always emit lowercase, but it’s worth tightening for implementer clarity.

### Blocking Issues (B1, B2, etc.)
**B1. Missing canonical UUID serialization rules for 16-byte UUID fields and UUID strings.**  
This is genuinely interoperability-blocking because the spec uses UUID strings *as protocol-visible identifiers* (e.g., `receipt.message_ids[]`, `reply_to`, etc.) that must refer to the 16-byte `message_id` in the PUSE header. If two implementations stringify/parse UUID bytes differently, the recipient cannot reliably correlate receipts/replies to messages produced by a different implementation. The same problem appears in Sync where a “UUID-based” `document_id` is defined as `uuid_bytes || 16 zero bytes` without defining which UUID byte order to use.

**Minimum fix (normative):**
- Define that UUID strings are **RFC 4122 canonical textual representation** (lowercase hex, 8-4-4-4-12 with hyphens).
- Define that “UUID bytes” on wire are the **RFC 4122 network byte order** corresponding to that string (the 16 bytes as laid out in RFC 4122, not mixed-endian platform encodings).
- Provide one concrete test vector: UUID string ↔ 16-byte hex.

### Minor Issues (M1, M2, etc.)
**M1. Align device doc signature verification guidance** (current-only vs allow current/previous/history with non-revoked constraint). Pick one rule and reference it consistently (RFC + layer docs).

**M2. Make recovery attestation verification pseudocode consistently reflect the domain-separated JCS signature input** (even if RFC-0001 remains authoritative). This reduces implementer error.

**M3. Unify Base32 acceptance language**: either “MUST reject non-lowercase” everywhere, or “MUST normalize to lowercase for parsing but MUST emit lowercase” (either is fine; just be consistent).

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — 1 blocking issues require fixes

The spec is very close, but **B1 (UUID byte/string canonicalization)** needs a normative addition (and ideally a test vector) to ensure independent implementations can interoperate on receipts/replies and UUID-based sync document identifiers. Once that is addressed, remaining items are clarity hardening rather than interop blockers.
