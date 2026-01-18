## Holistic Review (Iteration 6): Cross-layer consistency + remaining messaging issues

Below are the issues that most directly block or materially complicate implementation. For each: **problem → citation(s) → proposed fix**.

---

# BLOCKING (cannot implement unambiguously without resolving)

## B1) Secure Envelope: signature coverage and field list are internally inconsistent
**Problem**: `secure-envelope.md` says the signature is “over everything above”, but the later “What is Signed” list omits several fields that are present in the wire format (e.g., `message_id`, `header_extension_length`, `header_extension`). It also references `ephemeral_public`, which is **not a fixed top-level field** in the envelope wire format (it lives inside the header extension).

- **Citations**
  - `spec/03-messaging-sync/secure-envelope.md` → **Envelope Structure / Wire Format**
  - `spec/03-messaging-sync/secure-envelope.md` → **Signature Scheme / What is Signed**

**Proposed fix (normative)**:
1. Replace “What is Signed” with:  
   ```
   signed_data = all bytes from Magic through Ciphertext (inclusive),
                i.e., the entire envelope except the final 64-byte signature.
   ```
2. Explicitly state that `header_extension` bytes (including any embedded ephemeral public key) are covered by the signature automatically because they precede the signature field.
3. Add a precise parse order for streaming receivers:
   - read fixed prefix through `header_extension_length`
   - read `header_extension`
   - read `nonce`
   - read `ciphertext_length`
   - read `ciphertext`
   - read `signature`

This also makes message framing on QUIC streams unambiguous.

---

## B2) Messaging interfaces’ encrypted message structs do not match the Secure Envelope wire format
**Problem**: `Messaging & Sync Interfaces` defines `EncryptedMessage`/`GroupEncryptedMessage` as ad-hoc structs (with fields like `ephemeralPublicKey`) that do not exist as top-level fields in the PUSE wire format and omit fields that do (magic/version/flags/message_id/header_extension_length/etc.). This creates an API↔wire mismatch.

- **Citations**
  - `spec/03-messaging-sync/interfaces.md` → `EncryptedMessage`, `GroupEncryptedMessage`
  - `spec/03-messaging-sync/secure-envelope.md` → **Wire Format**

**Proposed fix** (choose one approach; A is cleaner):
- **A (recommended)**: Define `type SecureEnvelopeBytes = Uint8Array;` and make encryption APIs return raw envelope bytes plus minimal parsed metadata:
  ```ts
  interface SealedEnvelope {
    bytes: Uint8Array;          // full PUSE bytes
    messageId: Uint8Array;      // 16 bytes
    senderIid: Uint8Array;      // 20 bytes
    recipient: Uint8Array;      // 20 bytes (IID or GroupID raw)
    flags: number;              // 1 byte
  }
  ```
  Then remove/replace `EncryptedMessage` and `GroupEncryptedMessage`.
- **B**: Expand the structs to exactly mirror the PUSE wire format (including extension bytes) and state a canonical serialization method. This is more error-prone.

---

## B3) Secure Envelope “Initial Key Exchange” is inconsistent with the Double Ratchet/X3DH section
**Problem**: `secure-envelope.md` describes a “first message” key derivation using only `X25519(ephemeral_private, recipient_public)` with HKDF salt `sender_iid || recipient_iid`. Meanwhile `double-ratchet.md` specifies an X3DH-like initialization using **two DH outputs** (IK×IK and EK×IK) with a different domain separation strategy. These are not the same protocol; implementers won’t know which to ship.

- **Citations**
  - `spec/03-messaging-sync/secure-envelope.md` → **Key Exchange (1:1 Messages) / Initial Key Exchange**
  - `spec/03-messaging-sync/double-ratchet.md` → **Session Initialization / X3DH**

**Proposed fix**:
- Make `secure-envelope.md` purely a *container* spec (wire format + authenticated encryption interface), and move all 1:1 key establishment details to `double-ratchet.md` (or a new `session-init.md`).
- Normatively state in `secure-envelope.md`:
  > “The AEAD key used for PUSE is provided by the Messaging layer session protocol (X3DH+Double Ratchet). PUSE does not define session establishment itself.”
- If you want PUSE to support “initial messages” without an established ratchet, then **explicitly bind it to the X3DH spec you already wrote**:
  - Initial header extension (0x00) includes `EK_A_public`
  - Message key derivation references `kdf_initial()` (from `double-ratchet.md`) exactly

---

## B4) Sync security wrapper is undefined and contradicts “Secure Envelope is foundational for all messages”
**Problem**: `03/overview.md` claims Secure Envelope wraps “all messages”, but `sync-protocol.md` defines independent CBOR-framed sync messages on the QUIC sync stream without stating whether they are:
- E2E protected at the messaging layer (PUSE), or
- only transport-protected by QUIC TLS, relying on per-operation signatures, or
- optionally encrypted per-document.

This is a cross-layer contract gap.

- **Citations**
  - `spec/03-messaging-sync/overview.md` → “Secure Envelope … foundation for all messages”
  - `spec/03-messaging-sync/sync-protocol.md` → **Sync Wire Protocol** (CBOR framing)

**Proposed fix** (minimal, implementable MVP):
- Define a clear rule:
  - **Sync stream (0x04)** is carried over an **identity-authenticated QUIC connection** and uses CBOR framing as specified.
  - Integrity/auth comes from `SyncOperation.signature` + authenticated transport peer IID binding (from `peer-handshake.md`).
  - Confidentiality:
    - If document is private: operations/metadata MUST be encrypted with a document key (your “Encrypted Sync” section), and the encryption format MUST be specified (AEAD + nonce + AAD).
- Update `03/overview.md` to say: Secure Envelope is foundational for **messaging (0x03) and mailbox storage**, not necessarily for sync (0x04), unless you explicitly choose to wrap sync in PUSE.

(Alternative: wrap every sync frame in PUSE and put CBOR inside ciphertext; that’s coherent but heavier.)

---

## B5) Device / multi-node model is missing but implied by goals and breaks several protocols
**Problem**: The spec implies users will run multiple nodes/devices for the same identity (“sync to a second node”), but there is no **Device ID / Node ID** concept. This breaks or ambiguates:
- transport connection dedup/glare (“one connection per peer IID”)
- session tickets/resumption binding
- ratchet sessions (“separate sessions per device” is mentioned as a test case but no IDs exist)
- group sender keys distribution (who exactly receives which key share?)

- **Citations**
  - `spec/00-overview/success-criteria.md` → “Sync app data to a second node”
  - `spec/01-transport-connectivity/interfaces.md` → `Connection.peerId` is a single IID; dedup rule uses IID only
  - `spec/03-messaging-sync/double-ratchet.md` → test scenario: “Multiple devices … separate sessions per device”

**Proposed fix (MVP-grade, but must be explicit)**:
1. Introduce **Device Identifier (DID)**:
   - `DID = Base32Lower(SHA256(device_signing_pubkey)[0:20])` (same encoding rules as IID)
2. Define a **Device Document** signed by the identity signing key:
   ```json
   {
     "did": "...",
     "iid": "...",
     "device_signing_key": "...",     // Ed25519
     "device_transport_key": "...",   // optional, if different
     "created_at": "...",
     "expires_at": "...",
     "signature_by_identity": "..."
   }
   ```
3. Update `peer-handshake.md`:
   - handshake claims `(iid, did)`
   - challenge signatures prove identity ownership **and** device binding
4. Update transport dedup rules to be per `(peer_iid, peer_did)` not just IID.
5. Update messaging sessions to be keyed by `(peer_iid, peer_did)`.

If you *don’t* want multi-device in MVP, you must say so explicitly and remove/defang the references/tests that assume it.

---

## B6) Identity Document schema contains incompatible endpoint and encryption history representations
**Problem**: `identity-document-schema.md` simultaneously presents:
- an older endpoint shape using `"address": "<host:port>"` (top “Document Structure” section)
- the newer normative endpoint object with `host`, `port`, `priority`, etc.
Also `keys.encryption.previous` is described as an array of history objects, but examples show `null` or a single string.

- **Citations**
  - `spec/02-identity-trust/identity-document-schema.md` → **Document Structure** (endpoints with `address`)
  - `spec/02-identity-trust/identity-document-schema.md` → **Endpoint Object (Normative)**
  - `spec/02-identity-trust/identity-document-schema.md` → **Example Documents** (`keys.encryption.previous: null` and later a string)

**Proposed fix**:
- Make the “Endpoint Object (Normative)” the *only* endpoint representation; remove/replace the `address` form everywhere.
- Standardize encryption key history:
  - `keys.encryption.previous` MUST be an array (possibly empty `[]`), never `null`, never a bare string.
- Update example documents accordingly.
- Ensure `spec/02-identity-trust/interfaces.md` matches (it currently expects an array, which is good).

---

## B7) Sequence number type inconsistencies across layers (string vs number) will cause correctness bugs
**Problem**: Global convention says sequence numbers are decimal strings to avoid JSON precision loss, and identity uses string. Transport layer uses `number` in multiple places (`expectedSequence`, `PeerEndpoints.sequence`, resumption examples), which will break beyond 2^53 and creates cross-layer mismatch.

- **Citations**
  - `spec/00-shared/layer-integration.md` → “Sequence numbers: Decimal string”
  - `spec/02-identity-trust/identity-document-schema.md` → `sequence` is string
  - `spec/01-transport-connectivity/interfaces.md` → `ConnectOptions.expectedSequence?: number`, `PeerEndpoints.sequence: number`
  - `spec/01-transport-connectivity/peer-handshake.md` → resumption example uses numbers

**Proposed fix**:
- Introduce `type SequenceNumber = string` in transport interfaces (imported from identity), and use it consistently:
  - `expectedSequence?: SequenceNumber`
  - `PeerEndpoints.sequence: SequenceNumber`
  - handshake resumption fields use strings
- Add a normative rule: implementations MUST treat sequence as uint64 string on wire; internal may use bigint.

---

# HIGH (significant ambiguity/inconsistency; implementable but risky)

## H1) TLS certificate policy contradicts itself (“ephemeral and ignored” vs “identity-bound certificate”)
- **Citations**
  - `spec/00-shared/layer-integration.md` → **QUIC TLS Certificate Policy** (“accept any valid TLS certificate; not used for identity”)
  - `spec/01-transport-connectivity/quic-integration.md` → TLS config table says “Certificate type: Self-signed, identity-bound”

**Fix**: Align on one statement:
- TLS cert is **self-signed and not identity-meaningful**; identity proof happens in post-TLS handshake.
- Optionally: MAY embed IID as a non-verified hint in cert SAN, but MUST NOT be relied on.

---

## H2) Endpoint `port` semantics say “UDP port” but mailbox is HTTPS (TCP)
- **Citations**
  - `spec/01-transport-connectivity/interfaces.md` → `Endpoint.port` comment: “UDP port”
  - `spec/02-identity-trust/identity-document-schema.md` → mailbox endpoints with `transport:"https"`

**Fix**: Redefine:
- `port` is the service port; protocol depends on `transport`:
  - `quic` ⇒ UDP
  - `https` ⇒ TCP (or HTTP/3 explicitly if desired)

---

## H3) DHT “single value” vs “multiple values” retrieval mismatch
**Problem**: Identity caching policy anticipates multiple DHT responses (accept highest valid sequence), but layer integration `fetchIdentity()` assumes a single returned value.

- **Citations**
  - `spec/02-identity-trust/caching-policy.md` → DHT may return multiple; accept highest valid sequence
  - `spec/00-shared/layer-integration.md` → `dht.get(key)` returns one result in pseudocode

**Fix**: Update layer-integration pseudocode to:
- accept multiple records
- decode+verify each
- select highest sequence (with TOFU/genesis constraint)

---

## H4) Mailbox auth token format and replay/abuse protections are underspecified
- **Citations**
  - `spec/00-shared/layer-integration.md` → **Mailbox Protocol** uses `Authorization: Bearer <sender-signed-token>` but no token format

**Fix** (minimal):
- Define token as a signed request object:
  ```json
  {
    "v": 1,
    "sender_iid": "...",
    "recipient_iid": "...",
    "issued_at": "...",
    "expires_at": "...",
    "nonce": "...",
    "signature": "..."   // Ed25519 over canonical form
  }
  ```
- Mailbox MUST enforce:
  - expiry
  - nonce replay cache
  - per-sender rate limits

---

## H5) Group membership operations lack a signing/authorization model and a convergent state model
**Problem**: `group-messaging.md` defines invites/leave/remove events as JSON snippets, but does not normatively define:
- what exactly is signed (envelope signature only? any group-state signature?)
- how members converge on the same membership list/version
- what “group version” increments, by whom, and how conflicts are resolved beyond a brief rule

- **Citations**
  - `spec/03-messaging-sync/group-messaging.md` → **Membership Operations** and **Membership Conflicts**

**Fix**:
- Define a **Group State Update** object:
  - includes `group_id`, `version` (uint64 string), `actor_iid`, `action`, `timestamp`, and is sent as a *group message*.
  - authorization rules by role are checked against locally known membership state.
- Make the update’s authenticity rely on:
  - sender identity signature on the PUSE envelope (already required)
  - plus a deterministic conflict resolution rule that is applied to *state updates* (not just described informally)

(Or: model membership as a CRDT OR-Set with signed operations; that’s even cleaner and matches your Sync/CRDT direction.)

---

## H6) Group sender key KDF is inconsistent within messaging docs
**Problem**: `double-ratchet.md` defines a specific `kdf_sender_key()` using HMAC-SHA256 with domain separation; `group-messaging.md` shows a sender-key chain using HKDF calls (`HKDF(chain_key, "message")`, etc.). This is ambiguity in a security-critical primitive.

- **Citations**
  - `spec/03-messaging-sync/double-ratchet.md` → **Group Sender Key KDF**
  - `spec/03-messaging-sync/group-messaging.md` → **Sender Key Chain** example

**Fix**: Pick one and make it normative everywhere (recommend: reuse the HMAC-SHA256 chain-step style for consistency and simpler implementation).

---

# MEDIUM (paper cuts, but should be cleaned for consistency)

## M1) Message ID appears in both envelope header (16-byte UUID) and plaintext JSON (`id`)
- **Citations**
  - `spec/03-messaging-sync/secure-envelope.md` → header has Message ID (16 bytes)
  - `spec/03-messaging-sync/secure-envelope.md` → plaintext JSON includes `"id": "<message-id-uuid>"`

**Fix**: State one of:
- Plaintext `id` MUST equal the header `message_id` (with exact UUID byte/string conversion rules), or
- Remove plaintext `id` and rely on header `message_id` only.

---

## M2) “Secure Envelope plaintext is JSON” vs sync CBOR and mixed on-wire conventions
- **Citations**
  - `spec/03-messaging-sync/secure-envelope.md` → plaintext JSON
  - `spec/03-messaging-sync/sync-protocol.md` → CBOR payloads

**Fix**: Clarify per-stream encoding rules:
- Message stream (0x03): PUSE ciphertext contains UTF-8 JSON (snake_case)
- Sync stream (0x04): CBOR frames (optionally encrypted) + operation signatures

---

## M3) Anonymous connections vs transport connection type assumptions
- **Citations**
  - `spec/01-transport-connectivity/peer-handshake.md` → Anonymous Connections allow `client_iid: null`
  - `spec/01-transport-connectivity/interfaces.md` → `Connection.peerId: IdentityIdentifier` (non-null)

**Fix**:
- Either disallow anonymous connections except explicitly defined services (relay/discovery), or
- make `peerId?: IdentityIdentifier` nullable and define states clearly for anonymous connections.

---

# LOW (quality/clarity improvements)

## L1) Add a single normative glossary for core identifiers and encodings
You currently have the rules scattered across `identity-document-schema.md` and `00-shared/layer-integration.md`. A small glossary would eliminate repeated micro-inconsistencies:
- IID string vs IID raw (20 bytes)
- GroupId encoding (also Base32Lower of 20 bytes)
- DID (if adopted)

---

# Suggested “Fix Plan” (minimal sequencing)

1. **Resolve B1+B2** (PUSE signature coverage + API↔wire alignment).
2. **Resolve B3** (one coherent session-init story: X3DH+ratchet vs ad-hoc initial DH).
3. **Resolve B4** (sync security wrapper and whether PUSE applies to sync).
4. **Resolve B5** (either explicitly single-device MVP, or define DID/device docs and update handshake+sessions).
5. Clean up schema and type consistency (**B6+B7**).

If you want, I can draft concrete patch text for the most central files (`secure-envelope.md`, `interfaces.md` in messaging/transport, and the identity schema cleanup) in a “diff-like” format.
