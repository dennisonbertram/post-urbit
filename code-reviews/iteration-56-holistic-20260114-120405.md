## Iteration 56: HOLISTIC REVIEW

### Cross-Document Consistency Check

- spec/03-messaging-sync/double-ratchet.md vs spec/06-rfcs/RFC-0003-messaging.md (and spec/03-messaging-sync/secure-envelope.md): Ratchet header counter semantics (N/PN) and pseudocode indexing disagree (double-ratchet appears 1-indexed in send pseudocode; RFC-0003 is explicitly 0-indexed).
- Severity: **BLOCKING**
- Status: **CONFIRMED ISSUE**

- spec/01-transport-connectivity/relay-protocol.md vs spec/06-rfcs/RFC-0002-transport.md: PURL `ERROR` packet payload format differs (relay-protocol.md omits the 2-byte “Message Length” field that RFC-0002 defines).
- Severity: **BLOCKING**
- Status: **CONFIRMED ISSUE**

- spec/05-ux-packaging/node-daemon.md vs spec/00-shared/layer-integration.md: Node daemon “Key Hierarchy” diagram includes a “Device transport key (X25519)” even though v1 explicitly removed/unused `device_transport_key` and handshake uses device signing key.
- Severity: **MINOR**
- Status: **CONFIRMED ISSUE**

- spec/03-messaging-sync/sync-protocol.md vs spec/03-messaging-sync/interfaces.md: Sync “TypeScript” structs mix snake_case (`created_at`, `document_id`) while the Messaging & Sync TS interfaces use camelCase (`createdAt`, `documentId`). No explicit mapping section is provided for Sync (CBOR) vs TS API objects.
- Severity: **MINOR**
- Status: **CONFIRMED ISSUE**

- spec/05-ux-packaging/app-distribution.md (internal) vs spec/05-ux-packaging/app-distribution.md (later section): repository manifest `signature` shown once as a string and later as an object `{ operator_iid, timestamp, sig }`.
- Severity: **MINOR**
- Status: **CONFIRMED ISSUE**

### Blocking Issues (B1, B2, etc.)

B1: spec/03-messaging-sync/double-ratchet.md — **Ratchet header N/PN semantics + indexing inconsistent with RFC-0003**
- What’s wrong: The non-RFC Double Ratchet doc describes and uses a “Chain Index” in a way that conflicts with RFC-0003’s normative definition where **N is 0-indexed** and **PN is previous chain length**. The example send pseudocode increments the index before placing it into the header, producing 1-indexed behavior for the first message.
- Evidence:
  - From **spec/03-messaging-sync/double-ratchet.md**:
    - Header fields:  
      > “Previous Chain Length (big-endian) 4 bytes … Chain Index (big-endian) 4 bytes”
    - Send pseudocode:  
      > “state.sending_chain_key, message_key = kdf_chain_step(state.sending_chain_key)  
      > state.sending_chain_index += 1  
      > header = encode_header( … chain_index=state.sending_chain_index … )”
  - From **spec/06-rfcs/RFC-0003-messaging.md**:
    - Counter semantics (normative):  
      > “N is 0-indexed: first message in a chain has N=0”  
      > “PN is the count of messages sent in the PREVIOUS sending chain”
- Exact fix:
  1. In `spec/03-messaging-sync/double-ratchet.md`, rename “Chain Index” to **“Message Number (N)”** and define **N as 0-indexed** and **PN as previous chain length**, matching RFC-0003 §3.4.3/§4.4.
  2. Update the send pseudocode so the header uses the **pre-increment** value:
     - Replace:
       - `state.sending_chain_index += 1` before header creation
       - `chain_index=state.sending_chain_index`
     - With:
       - `n = state.sending_chain_index`
       - derive key
       - `state.sending_chain_index += 1`
       - header uses `n` (and PN defined as previous chain length at ratchet boundary)
  3. Add an explicit note: “This document’s ratchet header semantics are identical to RFC-0003; RFC-0003 is authoritative.”

B2: spec/01-transport-connectivity/relay-protocol.md — **PURL ERROR packet payload format disagrees with RFC-0002**
- What’s wrong: `relay-protocol.md` defines an ERROR payload without a message-length field, but RFC-0002 defines ERROR payload as including a **2-byte Message Length**. This is a wire-format incompatibility.
- Evidence:
  - From **spec/01-transport-connectivity/relay-protocol.md**:
    > “ERROR Packet: … Error Code: 1 byte … Retry After (seconds) 4 bytes … Message (UTF-8) variable”
  - From **spec/06-rfcs/RFC-0002-transport.md** §7.12:
    > “ERROR Packet Payload: … Error Code (1 byte) … Retry After (seconds, big-endian) (4 bytes) … Message Length (2 bytes) … Message (UTF-8) (<length> bytes)”
- Exact fix:
  - Edit `spec/01-transport-connectivity/relay-protocol.md` “Rate Limit Response / ERROR Packet” diagram and description to **insert**:
    - `Message Length` — **2 bytes** (big-endian)
  - Update any textual description to say the message is `<length>` bytes, and that receivers MUST parse using the explicit length (not datagram remainder).

### Minor Issues (M1, M2, etc.)

M1: spec/05-ux-packaging/node-daemon.md — **Device transport key shown as present in v1 key hierarchy**
- Issue: The key hierarchy diagram includes “Device transport key (X25519)” while the rest of the spec says v1 does not use `device_transport_key` and handshake uses device signing key.
- Suggested fix: Mark “Device transport key (X25519)” as “reserved/future” in the diagram (or remove it), aligning with the note already present later:
  > “NOTE: getDeviceTransportKey is reserved for future use. v1 uses device signing key for handshake.”

M2: spec/03-messaging-sync/sync-protocol.md — **Snake_case vs camelCase mismatch vs Messaging & Sync TS interfaces**
- Issue: Sync Protocol uses TypeScript-looking structs with `created_at`, `document_id` etc, while `spec/03-messaging-sync/interfaces.md` defines TS APIs with `createdAt`, `documentId`.
- Suggested fix: Add a short “Wire (CBOR) vs TS API mapping” subsection in `sync-protocol.md`, or rewrite the non-wire “TypeScript” examples to camelCase and reserve snake_case for the CBOR schema blocks (which already define `document_id`, etc).

M3: spec/05-ux-packaging/app-distribution.md — **Repository manifest signature field inconsistent within the same document**
- Issue: Example shows `"signature": "<repository-operator-signature>"` but later specifies `"signature": { "operator_iid": ..., "timestamp": ..., "sig": ... }`.
- Suggested fix: Update the earlier repository.json example to match the later structured signature object, and explicitly state the signature field type once.

### Verdict

[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — **2** blocking issues require fixes
