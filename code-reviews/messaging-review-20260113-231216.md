## Major findings (cross-cutting)

- The **Double Ratchet header placement is currently non-functional** (chicken-and-egg: you need the header to derive the key, but the header is encrypted).
- The specs disagree on whether **Secure Envelope is the universal container** (group messaging defines a second, separate envelope).
- The system claims **“deniability”** while using **Ed25519 signatures on every message**, which provides strong non-repudiation (opposite of deniability).
- Several parts lack **canonical encoding / signature coverage definitions**, which is a common source of real-world crypto/protocol failures.

---

## Issues (with severity, affected file(s), issue, recommended fix)

### 1) BLOCKING — Ratchet header is encrypted, making decryption impossible
- **File(s):** `spec/03-messaging-sync/double-ratchet.md`, `spec/03-messaging-sync/secure-envelope.md`
- **Specific issue:** `double-ratchet.md` says “This header is included in the secure envelope’s plaintext”. But the receiver must read `dh_public`, `chain_index`, and `previous_chain_length` **before** it can derive the message key. If those fields are inside the ciphertext, the receiver can’t derive the key to decrypt them.
- **Recommended fix:**
  - Move the ratchet header to an **unencrypted, authenticated header** area (classic Signal design), e.g.:
    - Add fields to the Secure Envelope header for `{ratchet_dh_public, pn, n}` and treat them as **AEAD AAD** (and/or signed).
  - Alternatively, define a **separate “Ratchet Message” framing** where the ratchet header is outside the encrypted payload, and Secure Envelope becomes a wrapper with clear layering rules.

---

### 2) BLOCKING — “Secure Envelope is foundational for all messages” conflicts with a separate Group envelope
- **File(s):** `spec/03-messaging-sync/overview.md`, `spec/03-messaging-sync/secure-envelope.md`, `spec/03-messaging-sync/group-messaging.md`, `spec/03-messaging-sync/interfaces.md`
- **Specific issue:** Overview claims Secure Envelope is the foundation “for all messages”, but group messaging defines an independent binary envelope (`PUGM`) and the interfaces define `GroupEncryptedMessage` distinct from `EncryptedMessage`.
- **Recommended fix (pick one and make it explicit):**
  1. **Single-envelope approach:** Represent group messages as Secure Envelopes with `recipient type = group`, and put the group sender-key fields in a defined header extension (AAD) + ciphertext.
  2. **Two-envelope approach:** Clearly specify **layering** and transport mapping (e.g. “Message stream carries either PUSE or PUGM frames”), and replicate shared requirements consistently (versioning, signature coverage, nonce rules, limits, etc.).

---

### 3) BLOCKING — “Deniability” claim is incompatible with message signatures
- **File(s):** `spec/03-messaging-sync/overview.md`, `spec/03-messaging-sync/secure-envelope.md`, `spec/03-messaging-sync/group-messaging.md`
- **Specific issue:** Ed25519 signatures over the whole envelope allow any recipient to prove to a third party that the sender authored a message (non-repudiation). That conflicts with the stated goal: “**Deniability**”.
- **Recommended fix:**
  - Either **remove/relax the deniability goal** (document that the system is non-repudiable by design), **or**
  - Switch to a deniable authentication design:
    - Use **session MACs** derived from the ratchet (Signal-style) for message authentication.
    - Keep signatures only for key agreement / identity binding (e.g., signed prekeys / identity assertions), not for every message.

---

### 4) HIGH — Double Ratchet state machine is underspecified / deviates from known-correct designs
- **File(s):** `spec/03-messaging-sync/double-ratchet.md`
- **Specific issue:** The ratchet algorithm omits several standard, correctness-critical elements (Signal terminology: `Ns`, `Nr`, `PN`, skipped-key handling tied to previous chains), and the presented pseudocode leaves ambiguity about:
  - exact indexing semantics (0-based vs 1-based) for `chain_index`
  - how `previous_chain_length` is computed and used (the `...` placeholder)
  - when DH-sending key is rotated relative to receive events (it is deferred to next send; may be OK, but must be precisely specified to avoid interop failures)
- **Recommended fix:**
  - Adopt the established Double Ratchet spec variables and transitions (Root Key RK, CKs/CKr, DHs/DHr, Ns/Nr/PN).
  - Define **exact header fields**, their meaning, and monotonicity rules.
  - Provide **normative** (not illustrative) pseudocode for send/receive including skipped-key storage.

---

### 5) HIGH — KDF usage for chain steps is non-standard and ambiguously defined
- **File(s):** `spec/03-messaging-sync/double-ratchet.md`, `spec/03-messaging-sync/group-messaging.md`
- **Specific issue:** The specs use HKDF in a way that’s not clearly HKDF-Extract+Expand with domain separation and may be mis-implemented:
  - `kdf_chain_step(chain_key)` uses HKDF twice with `salt=b""` and different `info`. This can work if implemented correctly, but it’s easy to implement incorrectly and differs from common, well-audited Signal-style `HMAC(chain_key, 0x01/0x02)` KDF.
  - Group sender-key derivation uses `HKDF(sender_key.chain_key, b"message", length=32)` without specifying parameters (salt/info/ikm mapping).
- **Recommended fix:**
  - Specify a **single, unambiguous KDF construction**, e.g.:
    - `message_key = HMAC-SHA256(chain_key, 0x01)`
    - `new_chain_key = HMAC-SHA256(chain_key, 0x02)`
  - Or specify HKDF precisely (Extract salt, Expand info, and input bytes).
  - Add explicit **domain separation** that includes protocol + version, and (for group sender keys) **bind to `(group_id, sender_iid, key_id)`** in the KDF info.

---

### 6) HIGH — X3DH simplification breaks offline delivery under identity key rotation (and weakens authentication properties)
- **File(s):** `spec/03-messaging-sync/double-ratchet.md`
- **Specific issue:** The spec “skips signed prekey for simplicity” and relies on identity encryption keys that may rotate. If Bob rotates and deletes old private keys, Bob may be unable to decrypt Alice’s initial message (asynchronous failure). Also, omitting prekeys changes the authentication/FS properties compared to X3DH.
- **Recommended fix:**
  - Implement a minimal prekey bundle:
    - Signed prekey (SPK) + optionally one-time prekeys (OPK), or
    - At minimum, require recipients to **retain old encryption private keys** for a defined grace period and publish corresponding previous public keys.
  - Clarify exactly which identity keys are used and how they are authenticated/bound to signing keys.

---

### 7) HIGH — Group message signature does not cover the header fields
- **File(s):** `spec/03-messaging-sync/group-messaging.md`
- **Specific issue:** `sender_key_encrypt()` signs only `ciphertext`. The outer fields (sender IID, group ID, key ID, iteration, nonce, ciphertext length) are not covered. This enables trivial tampering/replay-context manipulation leading to:
  - denial-of-service (force decryption attempts with wrong derived keys)
  - potential cross-context replay if implementations mistakenly reuse keys across groups or mishandle lookups
- **Recommended fix:**
  - Sign **all bytes** of the group envelope except the signature field (same approach as Secure Envelope), and/or
  - Use AEAD with **AAD = header** and sign the full frame for robust binding.
  - State verification order: header parse → signature verify → derive/decrypt.

---

### 8) HIGH — Sync protocol lacks a complete security wrapper (authn/authz, anti-spoofing, canonical signing)
- **File(s):** `spec/03-messaging-sync/sync-protocol.md`, `spec/03-messaging-sync/overview.md`
- **Specific issue:** The sync stream wire protocol defines message types (REQUEST/OFFER/ACCEPT/OPS/ACK/…) but does not clearly specify:
  - how peers authenticate SYNC_REQUEST/OFFER etc. (spoofable control traffic can cause resource exhaustion)
  - whether sync control messages are signed, MAC’d, or wrapped in Secure Envelope
  - canonical encoding rules for signed operations and Merkle hashing
- **Recommended fix:**
  - Choose a security envelope for sync traffic:
    - Wrap sync frames in **Secure Envelope (recipient=peer)**, or
    - Define a sync-specific AEAD+signature framing with **canonical CBOR**.
  - Define **exact bytes** that are signed for `SyncOperation` and hashed for Merkle nodes (use RFC 8949 canonical CBOR).
  - Include limits/rate controls for offers/requests to reduce DoS.

---

### 9) MEDIUM — Secure Envelope replay detection is only possible after decryption (DoS vector)
- **File(s):** `spec/03-messaging-sync/secure-envelope.md`
- **Specific issue:** Replay protection relies on `message_id` and `sequence` inside encrypted JSON. Attackers can replay valid old envelopes to force signature verification and decryption work before detection.
- **Recommended fix:**
  - Include a small **unencrypted replay token** (e.g., `message_id` or `SHA256(message_id)` or an envelope unique ID) in the authenticated header (AAD/signed) so replays can be rejected pre-decrypt.
  - Keep the full message ID encrypted if metadata minimization is desired.

---

### 10) MEDIUM — Secure Envelope nonce format leaks timestamp and is unnecessary given AEAD key rotation
- **File(s):** `spec/03-messaging-sync/secure-envelope.md`
- **Specific issue:** Nonce includes a cleartext timestamp. This leaks timing even if transport timing is masked in the future. Also, many messages use distinct keys (ratchet message keys), making nonce uniqueness easier.
- **Recommended fix:**
  - Prefer **random 96-bit nonce** (simple, standard) or a per-session counter (if stateful).
  - If keeping timestamp for debugging, document it explicitly as metadata leakage.

---

### 11) MEDIUM — Missing device identity and routing model for “multiple devices”
- **File(s):** `spec/03-messaging-sync/double-ratchet.md`, `spec/03-messaging-sync/secure-envelope.md`, `spec/03-messaging-sync/overview.md`, `spec/03-messaging-sync/interfaces.md`
- **Specific issue:** The spec mentions “separate sessions per device” and includes a `Forward` flag, but there is no device identifier in headers, no per-device key material model, and no routing rules for mailbox fanout across devices.
- **Recommended fix:**
  - Add `device_id` and device public keys to the Identity layer integration.
  - Define whether envelopes are addressed to `(iid)` or `(iid, device_id)` and how “forward to other devices” is enforced securely.

---

### 12) MEDIUM — Group sender-key out-of-order / gap handling is not fully specified
- **File(s):** `spec/03-messaging-sync/group-messaging.md`
- **Specific issue:** The receiver must advance a sender’s chain to a given `iteration`. Large gaps can cause CPU DoS; out-of-order messages require skipped-key storage. The spec mentions `ITERATION_GAP` and “request resync” but does not define:
  - max gap policy
  - skipped-key cache structure/TTL
  - resync request/response message formats
- **Recommended fix:**
  - Define a per-(group_id, sender_iid, key_id) state with:
    - `current_iteration`, `chain_key`, `skipped_message_keys` (bounded), and a `max_skip` similar to the 1:1 ratchet.
  - Define explicit `sender_key_resync_request/response` messages.

---

### 13) MEDIUM — Conversation/message identifiers inconsistent between envelope plaintext and API
- **File(s):** `spec/03-messaging-sync/interfaces.md`, `spec/03-messaging-sync/secure-envelope.md`
- **Specific issue:** API `Message` includes `conversationId`, but Secure Envelope plaintext schema does not. Ordering and sequence are described “per conversation”, but conversation identity isn’t carried in the normative plaintext schema.
- **Recommended fix:**
  - Add `conversation_id` (or define derivation rules):
    - 1:1: `conversation_id = H("1:1" || min(iidA,iidB) || max(...) )`
    - group: `conversation_id = group_id`
  - Make the field naming consistent (snake_case vs camelCase) across spec and interfaces.

---

### 14) MEDIUM — Read receipt API mismatch (messageId vs sequence)
- **File(s):** `spec/03-messaging-sync/interfaces.md`
- **Specific issue:** `markAsRead(conversationId, upToMessageId)` but `onReadReceipt` reports `upToSequence`. This creates ambiguity about the canonical acknowledgement unit.
- **Recommended fix:** Pick one:
  - receipts by `messageId`, or
  - receipts by `(senderId, sequence)` / per-sender sequence, or
  - receipts by an HLC timestamp.
  Then align the interface and the on-wire receipt content.

---

### 15) MEDIUM — Group membership operations lack an explicit authentication/signing model
- **File(s):** `spec/03-messaging-sync/group-messaging.md`, `spec/03-messaging-sync/interfaces.md`
- **Specific issue:** Membership conflict resolution talks about versions and admin IIDs, but does not specify how membership updates are authenticated and protected against forgery/tampering (especially when relayed/mailboxed).
- **Recommended fix:**
  - Define group membership as a signed object / signed event log:
    - Admin-signed `group_event` updates with a group-state hash chain, or
    - Store membership as a **sync document (CRDT)** with admin signatures.
  - Bind group messages to membership state via a `membership_version` or `membership_root_hash` in message AAD.

---

### 16) LOW — Wire format details are missing/ambiguous in several places (interop risk)
- **File(s):** `spec/03-messaging-sync/secure-envelope.md`, `spec/03-messaging-sync/group-messaging.md`, `spec/03-messaging-sync/double-ratchet.md`, `spec/03-messaging-sync/sync-protocol.md`
- **Specific issue examples:**
  - `Sender Key ID` is 16 bytes on wire but `keyId` is a string in interfaces—no encoding defined.
  - “IID raw is 20 bytes” but the exact Base32 variant/decoding rules should be normative.
  - Sync messages say “CBOR-encoded” but later examples are JSON.
- **Recommended fix:**
  - Add a “Canonical Encodings” section:
    - Base32 alphabet/casing/padding rules
    - UUID byte order
    - Canonical CBOR requirement (if used for signatures/hashes)
  - Define all fixed-size identifiers (key_id format, generation, collision resistance).

---

### 17) LOW — Sync protocol GC/compaction conditions are not implementable as written
- **File(s):** `spec/03-messaging-sync/sync-protocol.md`
- **Specific issue:** “Tombstones can be GC’d when all replicas have observed deletion” requires a replica set model + acknowledgements/version vectors, which are not defined.
- **Recommended fix:**
  - Define replica tracking (explicit peer set per document) and acknowledgement vectors, **or**
  - Make GC purely policy/time-based unless explicit confirmations exist.

---

## Integration gaps to address explicitly

- **Identity ↔ Messaging:** clarify how key rotation interacts with asynchronous initiation (retain old privkeys? signed prekeys? key IDs in envelopes?).
- **Transport ↔ Wire formats:** define whether QUIC streams carry *only* one framing (PUSE) or multiple (PUSE/PUGM/sync frames), and how multiplexing is done safely.
- **Mailbox:** define how mailbox indexing works (by recipient IID? by device?), and whether mailbox requires authenticated upload (to reduce spam/DoS).

---

If you want, I can propose a concrete “v1 corrected framing” that unifies:
- Secure Envelope outer frame (magic/version/flags/sender/recipient/nonce/len/signature),
- an authenticated **header extension** for ratchet/group/sync metadata (AAD),
- and a single canonical CBOR payload for all message types (instead of mixing JSON/CBOR).
