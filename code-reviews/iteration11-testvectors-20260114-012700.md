## Issues Found

### BLOCKING (incorrect/missing critical vectors)

1. **Vectors are not actually reproducible / internally consistent**
   - Multiple “expected” values are clearly placeholders or synthetic patterns (e.g., sequential-looking hex like `2a4b6c7d...`, truncated hashes with `...`, ciphertext/tag shown as `<encrypted bytes>`, many `<32 bytes>` placeholders).
   - As written, implementers cannot regenerate or verify most vectors end-to-end.

2. **Ed25519 key material is almost certainly not valid for the claimed derivation**
   - Test Vector 1 claims an Ed25519 public key corresponding to the seed `7f83b1...`, but the public key shown (`2a4b6c7d...`) appears non-derived/patterned.
   - This breaks **IID derivation**, **identity doc signature**, and any later signatures that depend on the same identity.

3. **Base64 encodings have length/decoding problems**
   - For 32-byte raw keys, Base64 without padding should be **43 chars** (since 32 bytes → 44 chars with one `=`, stripped → 43).
   - Example in Test Vector 2: `"KktsffifChssPU5fant4nQ4fKjtMXW5/ipswHSovX5Vb"` appears to be **44 characters with no padding**, which would decode to the wrong length (implementation-dependent failure).
   - Ed25519 signatures are 64 bytes → Base64 without padding should be **86 chars**. The provided signature Base64 in Test Vector 2 is not 86 chars and is not credible as a computed value.

4. **Critical vectors (needed for interoperable crypto) are incomplete**
   - Test Vector 3 (PUSE) lacks actual ciphertext, tag, ciphertext length, signature, and fully specified sender/recipient raw IID bytes.
   - Test Vector 4 (X3DH) omits DH outputs and the final `(root_key, initial_chain_key)`.
   - Test Vector 5/6/7 (ratchet + sender key KDFs) include truncated outputs (`...`) or placeholders.
   - Test Vector 8/9 also provide placeholders instead of computed hashes/signatures/derivations.

5. **Nonce timestamp example appears wrong**
   - In Test Vector 3, `Timestamp (4 bytes): 0x678e8a80 (2025-01-20 00:00:00 UTC)` is very likely not the correct UNIX seconds encoding for that timestamp.
   - Because nonce format is normative (and replays/nonces are safety-critical), the vector must be exact.

6. **Spec inconsistency: header extension framing differs between documents**
   - `spec/03-messaging-sync/double-ratchet.md` says ratchet header extension wire format includes `type (1) || length (2) || ratchet_header (40)`.
   - `spec/03-messaging-sync/secure-envelope.md` defines the header extension as `type (1) || …` *without* a per-extension length field (it already has a global `Header Extension Length` in the envelope).
   - Test Vector 3 matches the “no per-extension length” approach (`0x00 + pubkey`), while double-ratchet.md currently describes an embedded length. This will cause interop failures if not resolved.

7. **HKDF empty-salt behavior is underspecified and library-dependent**
   - The prose says `HKDF-Extract(salt=b"", ...)`. Some libraries treat empty salt as an empty HMAC key, others follow RFC5869’s “salt absent → HashLen zeros”.
   - Your generator code uses the RFC5869-compatible behavior (`salt or b'\x00'*32`). The spec should normatively state this to avoid cross-language mismatch.

---

### HIGH (important gaps)

1. **No “known-good” full-message end-to-end vector**
   - Implementers need at least one vector that goes from:
     - derived keys → IID/DID → identity document JCS → signature → envelope build → signature verify → AEAD decrypt
   - Right now, each step is either placeholder or uses non-derived values.

2. **No negative/invalid vectors (these catch real-world bugs)**
   Missing vectors for:
   - non-canonical Base32 IID inputs (uppercase, padding, invalid alphabet chars)
   - Base64 padding acceptance/rejection rules
   - wrong-length keys/signatures
   - signature verification failure cases (wrong signing key, modified AAD, modified ciphertext, modified header fields)
   - nonce reuse detection expectations (at least “what state to maintain”)

3. **Identity key rotation and “previous signature” path not vectorized**
   - The identity spec relies heavily on `signatures.previous` to authorize rotations.
   - There is no test vector showing:
     - sequence increment
     - new signing key
     - `keys.signing.previous` populated
     - `signatures.previous` computed and verified against the *old* key
   - This is one of the highest-risk implementation areas.

4. **Handshake vectors don’t bind to actual TLS exporter bytes**
   - Test Vector 8 gives a `tls_binding` hex value, but not how it was obtained (it’s fine to fix it as a constant for the vector), and does not provide computed `challenge_data_hash` or signature outputs.
   - Also missing: server-vs-client role swap test (the two challenge constructions differ).

5. **X25519 test vectors should include clamping expectations**
   - Different libraries expose “raw scalar” vs “clamped scalar” vs “seed”.
   - Without an explicit rule (“use RFC7748 X25519 function which clamps internally; store the 32-byte scalar as produced by HKDF”), implementers may generate different public keys.

---

### MEDIUM (nice to have)

1. **JCS edge cases**
   - You use JCS, which is great, but implementers routinely get tripped up on:
     - Unicode normalization (JCS does not normalize strings)
     - escaping rules
     - ordering of keys in nested objects (already partially shown)
   - Add vectors with non-ASCII, embedded quotes, and edge JSON cases.

2. **Boundary conditions**
   - IID/DID derivation for keys that hash to Base32 strings containing lots of `2-7` characters (catches alphabet bugs).
   - Sequence numbers near uint64 boundaries (you already use string encoding—good—but test it).
   - Envelope max/min sizes (min size already computed; provide a max-size test).

3. **Multiple header extensions**
   - The current format is “one header extension blob”; you’ll likely want TLV composition later (ratchet + ack + padding, etc.). If so, vectorize concatenated extensions early.

---

## Recommendations

1. **Replace all placeholder/pattern values with computed outputs and make vectors self-verifying**
   - For each vector, provide *complete* hex/Base64 outputs (no ellipses) for:
     - derived private seed(s)
     - derived public key(s)
     - SHA256 outputs
     - signatures
     - HKDF/HMAC outputs (full 32 bytes)
     - ciphertext + Poly1305 tag (full bytes)
     - final envelope bytes (full hex)
   - Add a single “golden” script (and CI check) that regenerates and diffs these vectors.

2. **Add a “Vector 0: primitive sanity checks” section**
   - Ed25519: sign/verify on a short fixed message with the derived key.
   - X25519: scalar mult vs known test (or at least show computed pubkey + shared secret for a pair).
   - HKDF-SHA256: include RFC5869 known test case to validate your HKDF implementation.

3. **Make HKDF salt handling normative**
   - Specify: *“If salt is empty/absent, use 32 bytes of 0x00 as the HMAC key (RFC5869).”*
   - In pseudocode, write it explicitly (avoid `salt=b""` ambiguity across libraries).

4. **Resolve header-extension framing inconsistency**
   - Choose one:
     - **Option A (simpler, consistent with secure-envelope.md):** envelope has one length; header extension is just bytes beginning with `type`, no inner length.
     - **Option B (future-proof TLV):** header extension is a sequence of `type(1) || len(2) || value(len)` items.
   - Then update *both* `double-ratchet.md` and `secure-envelope.md`, and update Test Vector 3 accordingly.

5. **Add the most valuable missing vectors (priority order)**
   1. **Full PUSE envelope (initial message, ext type 0x00)**  
      - Include: derived X3DH keys, derived message key, nonce, AAD, ciphertext+tag, full envelope bytes, Ed25519 signature, and verification steps.
   2. **Full PUSE envelope (ratchet message, ext type 0x01)**  
      - Include: ratchet header fields, chain index behavior, and decrypt path.
   3. **Identity rotation vector (sequence 0 → 1)**  
      - Demonstrate `signatures.current` (new key) and `signatures.previous` (old key) both verifying over the same canonical “document_without_signatures”.
   4. **Handshake challenge vectors (server->client and client->server)**  
      - Provide exact `challenge_data` bytes (hex), `SHA256(challenge_data)` (hex), and the Ed25519 signature (hex + Base64).
      - Include a DID-present variant to validate `device_signature`.
   5. **Double ratchet skipped/out-of-order handling**  
      - A small scenario: send 3 messages, deliver #3 before #2, show skipped key derivation and successful decrypt.
   6. **Negative vectors**  
      - Modified AAD bit → AEAD failure  
      - Modified header field → signature failure  
      - Uppercase IID → reject  
      - Wrong Base64 length → reject

If you want, I can propose an exact revised set of test vectors with concrete computed values (including correcting the Base64 lengths and generating real Ed25519/X25519 material) and a reference generator layout that outputs a machine-readable `test-vectors.json` alongside the markdown.
