## Iteration 62: HOLISTIC REVIEW

### Cross-Document Consistency Check

1. **Sender-key KDF domain-separator byte length mismatch (25 vs 26)**
   - **Where:** `spec/03-messaging-sync/double-ratchet.md` (“Group Sender Key KDF”) states:
     - `Total info length: 26 (prefix) + ... = 84 bytes (fixed)`
   - **But:** The actual prefix `post-urbit-sender-key-v1:` is **25 bytes** (and this is what the registry and RFC-0003 imply):
     - `spec/00-shared/layer-integration.md` Domain Separator Registry: `post-urbit-sender-key-v1:` = **25+**
     - `spec/06-rfcs/RFC-0003-messaging.md` Appendix A: `post-urbit-sender-key-v1:` = **25 bytes + binding**
   - **Impact:** `double-ratchet.md` marks the size as *fixed*; implementers may enforce the wrong fixed-length (84) and reject correct inputs, or build incorrect “length-checked” KDF inputs.
   - **Correct values:** Prefix length **25**, total info length **83** (25 + 20 + 1 + 20 + 1 + 16).

2. **Bulk stream payload format inconsistency**
   - **Where:** `spec/00-shared/layer-integration.md` stream table says:
     - `0x05 Bulk | Binary | Raw data transfer`
   - **But:** `spec/06-rfcs/RFC-0002-transport.md` §6.3 defines Bulk payload as:
     - “First 2 bytes = opcode (0x0001=DATA_CHUNK, 0x0002=COMPLETE, 0x0003=ABORT)”
   - **Impact:** Conflicting wire-format expectations for stream type `0x05` across “glue” vs RFC; risks interoperability failures for bulk transfers.

3. **2DH vs X3DH naming drift (terminology/reference inconsistency)**
   - **Where:** `spec/03-messaging-sync/secure-envelope.md` refers to **X3DH** for initial messages; `spec/03-messaging-sync/double-ratchet.md` also labels the section “Initial Key Derivation (X3DH)”.
   - **But:** `spec/06-rfcs/RFC-0003-messaging.md` explicitly defines the initial exchange as **2DH** (simplified).
   - **Impact:** Not a byte-level mismatch, but it is a cross-document reference mismatch that can mislead implementers about whether signed prekeys exist/are required.

4. **Handshake error-code enum drift between protocol messages vs API error enum**
   - **Where:** `spec/01-transport-connectivity/peer-handshake.md` documents JSON error codes like `DOCUMENT_INVALID`, `TLS_BINDING_MISMATCH`, `VERSION_UNSUPPORTED`, `NONCE_REUSE`.
   - **But:** `spec/01-transport-connectivity/interfaces.md` `TransportErrorCode` union does not include several of these.
   - **Impact:** Mostly API-level consistency/ergonomics; protocol remains clear in RFC-0002. Still an enum mismatch across documents.

### Blocking Issues (B1, B2, etc.)

**B1. Sender-key KDF prefix length/total-length mismatch (protocol-critical “fixed length” claim)**
- **Files:**  
  - `spec/03-messaging-sync/double-ratchet.md` vs `spec/00-shared/layer-integration.md` vs `spec/06-rfcs/RFC-0003-messaging.md`
- **Fix:** In `double-ratchet.md`, correct:
  - prefix length to **25 bytes**
  - total info length to **83 bytes**
  - (and any derived arithmetic/comments asserting “fixed 84 bytes”)

**B2. Bulk stream (0x05) payload format mismatch between glue doc and RFC**
- **Files:**  
  - `spec/00-shared/layer-integration.md` vs `spec/06-rfcs/RFC-0002-transport.md` (§6.3)
- **Fix:** Update `layer-integration.md` stream type table to match RFC-0002 (or explicitly defer bulk payload format to RFC-0002 and remove “raw data transfer” claim).

### Minor Issues (M1, M2, etc.)

**M1. 2DH vs X3DH terminology drift**
- **Files:** `spec/03-messaging-sync/secure-envelope.md`, `spec/03-messaging-sync/double-ratchet.md` vs `spec/06-rfcs/RFC-0003-messaging.md`
- **Suggested fix:** Rename the sections/references to “2DH” (keeping the HKDF info string `post-urbit-x3dh-v1` if desired as a stable label), and add a brief note that the info label is historical.

**M2. Handshake error-code enum mismatch (protocol vs API surface)**
- **Files:** `spec/01-transport-connectivity/peer-handshake.md` vs `spec/01-transport-connectivity/interfaces.md`
- **Suggested fix:** Either (a) expand `TransportErrorCode` to include all handshake failure reasons documented, or (b) explicitly state that `TransportErrorCode` is an implementation/API abstraction and not required to mirror on-wire `handshake_complete.error.code`.

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — N blocking issues require fixes
