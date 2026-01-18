# RFC-0003: Post-Urbit Messaging Protocol

**Status:** Draft
**Version:** 1.0
**Authors:** Post-Urbit Protocol Team
**Created:** 2026-01-14

## Abstract

This document specifies the Post-Urbit Secure Envelope (PUSE) format for end-to-end encrypted messaging, the Double Ratchet session protocol for forward-secret key derivation, and the Mailbox protocol for offline message delivery. Together these provide confidential, authenticated, forward-secret messaging between identities.

## Table of Contents

1. [Introduction](#1-introduction)
2. [Conventions](#2-conventions)
3. [PUSE Envelope Format](#3-puse-envelope-format)
4. [Session Protocol (Double Ratchet)](#4-session-protocol-double-ratchet)
5. [Initial Key Exchange (2DH)](#5-initial-key-exchange-2dh)
6. [Group Messaging](#6-group-messaging)
7. [Mailbox Protocol](#7-mailbox-protocol)
8. [Message Types](#8-message-types)
9. [Error Handling](#9-error-handling)
10. [Security Considerations](#10-security-considerations)
11. [Test Vectors](#11-test-vectors)
12. [Implementation Notes](#12-implementation-notes)
13. [References](#13-references)

---

## 1. Introduction

### 1.1 Purpose

The Post-Urbit Messaging Protocol provides:

- **Confidentiality**: Message content readable only by sender and recipient(s)
- **Authenticity**: Recipients verify sender identity via Ed25519 signatures
- **Integrity**: Tamper detection via AEAD authentication tags
- **Forward secrecy**: Compromised keys don't reveal past messages
- **Break-in recovery**: Future messages protected after compromise (via ratcheting)
- **Offline delivery**: Messages stored in mailboxes for offline recipients

### 1.2 Scope

This RFC covers:
- PUSE (Post-Urbit Secure Envelope) binary format
- Double Ratchet session management
- 2DH initial key exchange
- Group messaging with sender keys
- Mailbox store-and-forward protocol

Out of scope:
- Transport layer (see RFC-0002)
- Identity documents (see RFC-0001)
- Sync protocol (separate RFC)

### 1.3 Terminology

| Term | Definition |
|------|------------|
| **PUSE** | Post-Urbit Secure Envelope - the encrypted message container |
| **IID** | Identity Identifier (32-char Crockford Base32) |
| **Ratchet** | Key derivation chain that advances with each message |
| **Sender Key** | Per-sender symmetric key used for group encryption |
| **Mailbox** | Server that stores messages for offline recipients |

---

## 2. Conventions

### 2.1 Notation

This document uses the same encoding conventions as RFC-0001 and RFC-0002:

| Format | Encoding | Notes |
|--------|----------|-------|
| IID/DID | Crockford Base32 lowercase | 32 characters |
| Keys/signatures | Base64 standard (no padding) | `A-Za-z0-9+/` |
| Raw bytes in wire format | Binary | Big-endian for multi-byte integers |

### 2.2 Cryptographic Primitives

| Purpose | Algorithm | Notes |
|---------|-----------|-------|
| Key agreement | X25519 | ECDH on Curve25519 |
| Key derivation | HKDF-SHA256 | RFC 5869 |
| Symmetric encryption | ChaCha20-Poly1305 | AEAD, RFC 8439 |
| Signing | Ed25519 | RFC 8032 |
| Hashing | SHA-256 | General purpose |

### 2.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119. [REQ-MSG-029]

---

## 3. PUSE Envelope Format

### 3.1 Overview

The PUSE envelope is a binary format containing:
1. **Unauthenticated header**: Routing information (readable without keys)
2. **Header extension (AAD)**: Cryptographic parameters (authenticated but unencrypted)
3. **Ciphertext**: Encrypted payload
4. **Signature**: Ed25519 signature over all preceding bytes

### 3.2 Wire Format

```
PUSE Envelope:
┌────────────────────────────────────────┐
│ Magic: 0x50 0x55 0x53 0x45 ("PUSE")   │ 4 bytes
├────────────────────────────────────────┤
│ Version: 0x01                          │ 1 byte
├────────────────────────────────────────┤
│ Flags                                  │ 1 byte
├────────────────────────────────────────┤
│ Sender IID (raw bytes)                 │ 20 bytes
├────────────────────────────────────────┤
│ Recipient IID/GroupID (raw bytes)      │ 20 bytes
├────────────────────────────────────────┤
│ Message ID (UUID v4)                   │ 16 bytes
├────────────────────────────────────────┤
│ Header Extension Length                │ 2 bytes (big-endian)
├────────────────────────────────────────┤
│ Header Extension                       │ <ext_len> bytes
├────────────────────────────────────────┤
│ Nonce                                  │ 12 bytes
├────────────────────────────────────────┤
│ Ciphertext Length                      │ 4 bytes (big-endian)
├────────────────────────────────────────┤
│ Ciphertext                             │ <ct_len> bytes
├────────────────────────────────────────┤
│ Signature                              │ 64 bytes
└────────────────────────────────────────┘
```

**Field offsets (fixed header = 64 bytes before extension):**

| Field | Offset | Size |
|-------|--------|------|
| Magic | 0 | 4 |
| Version | 4 | 1 |
| Flags | 5 | 1 |
| Sender IID | 6 | 20 |
| Recipient IID | 26 | 20 |
| Message ID | 46 | 16 |
| Header Extension Length | 62 | 2 |

**UUID Serialization (Normative):** The 16-byte `Message ID` field MUST use RFC 4122 network byte order (big-endian). See `spec/00-shared/layer-integration.md` "UUID Serialization (Normative)" for the authoritative byte-order specification and test vectors. The canonical string format (used in mailbox APIs) is lowercase hexadecimal with hyphens: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`. [REQ-MSG-030]

**Size constraints:**
- Minimum envelope size: `160 + ext_len` bytes (16-byte ciphertext minimum for empty plaintext with ChaCha20-Poly1305 tag)
  - Group extension (21 bytes): min = 181 bytes
  - Initial/Ephemeral extension (33 bytes): min = 193 bytes
  - Ratchet extension (41 bytes): min = 201 bytes
- Maximum envelope size: 1,048,576 bytes (1 MB)
- Maximum header extension: 1024 bytes
- Maximum ciphertext: `MAX_ENVELOPE - (64 + ext_len + 12 + 4 + 64)` bytes
  - With 21-byte extension: max ciphertext = 1,048,411 bytes
  - With 1024-byte extension: max ciphertext = 1,047,408 bytes

**Maximum Envelope Size (Normative):**

The 1,048,576 byte (1 MB) limit applies to the **total PUSE envelope size** as transmitted on the wire. This is the sum of all bytes from the magic field through the signature field, inclusive.

Implementations MUST reject envelopes exceeding this limit at the earliest possible point (ideally during length-prefix parsing in stream framing, before allocating buffers for the full envelope). [REQ-MSG-031]

**Derived Maximum Plaintext Size:**

Given the envelope structure, the maximum plaintext size is:

```
max_plaintext = MAX_ENVELOPE - fixed_overhead - ext_len - poly1305_tag
              = 1,048,576 - 144 - ext_len - 16
              = 1,048,416 - ext_len bytes
```

Where `fixed_overhead` = 64 (header) + 12 (nonce) + 4 (ciphertext_length) + 64 (signature) = 144 bytes.

| Extension Type | ext_len | Max Plaintext |
|----------------|---------|---------------|
| Group (0x02) | 21 | 1,048,395 bytes |
| Initial (0x00) | 33 | 1,048,383 bytes |
| Ratchet (0x01) | 41 | 1,048,375 bytes |
| Max extension | 1024 | 1,047,392 bytes |

**Note:** The ciphertext field contains `plaintext + 16-byte Poly1305 tag`, hence the tag is subtracted from the maximum plaintext calculation.

**Header extension sizes by type:**
- Group (0x02): 21 bytes (1 type + 16 sender_key_id + 4 iteration)
- Initial/Ephemeral (0x00): 33 bytes (1 type + 32 ephemeral_key)
- Ratchet (0x01): 41 bytes (1 type + 32 dh_public + 4 pn + 4 n)

**Note:** Exactly one header extension is REQUIRED (see §3.4). Reject envelopes with `ext_len == 0`. [REQ-MSG-032]

### 3.3 Flags Byte

```
Flags (1 byte):
┌─┬─┬─┬─┬─┬─┬─┬─┐
│7│6│5│4│3│2│1│0│
└─┴─┴─┴─┴─┴─┴─┴─┘
 │ │ │ │ │ │ └─┴── Recipient type: 00=1:1, 01=group, 10=broadcast, 11=reserved
 │ │ │ │ │ └───── Requires ACK: 1=yes, 0=no
 │ │ │ │ └─────── Priority: 1=high, 0=normal
 │ │ │ └───────── Forward: 1=can forward to other devices, 0=single device
 │ │ └─────────── Reserved (must be 0)
 │ └───────────── Reserved (must be 0)
 └─────────────── Reserved (must be 0)
```

**Recipient type values:**
| Value | Meaning |
|-------|---------|
| 0x00 | 1:1 message (recipient is IID) |
| 0x01 | Group message (recipient is group ID) |
| 0x02 | Broadcast (reserved for future use) |
| 0x03 | Reserved |

**Recipient Type ↔ Extension Type Linkage (Normative):**

The `recipient_type` (flags bits 0-1) and header extension type MUST be consistent: [REQ-MSG-033]

| Recipient Type | Allowed Extension Types |
|----------------|------------------------|
| 0x00 (1:1) | 0x00 (Initial) or 0x01 (Ratchet) |
| 0x01 (Group) | 0x02 (Group) |
| 0x02 (Broadcast) | Reserved (reject) |
| 0x03 (Reserved) | Reserved (reject) |

Implementations MUST reject envelopes where the recipient type and extension type are inconsistent. This ensures routing logic and encryption mode are always aligned. [REQ-MSG-034]

### 3.4 Header Extensions

Header extensions carry cryptographic parameters needed before decryption.

**Framing Rule:** Exactly ONE header extension MUST be present per envelope. The `Header Extension Length` field MUST equal the fixed size for the given extension type. Implementations MUST reject envelopes where `ext_len == 0` or where `ext_len` does not match the expected size for the extension type. [REQ-MSG-035]

#### 3.4.1 Extension Type Registry

| Type | Name | Size | Usage |
|------|------|------|-------|
| 0x00 | Initial | 33 bytes | First message (2DH) |
| 0x01 | Ratchet | 41 bytes | Double Ratchet messages |
| 0x02 | Group | 21 bytes | Group sender key messages |

#### 3.4.2 Initial Extension (Type 0x00)

For the first message to a new recipient:

```
Initial Header Extension:
┌────────────────────────────────────────┐
│ Extension Type: 0x00                   │ 1 byte
├────────────────────────────────────────┤
│ Ephemeral Public Key (X25519)          │ 32 bytes
└────────────────────────────────────────┘
Total: 33 bytes
```

**Initial Message Key Derivation (Normative):**

The sender MUST derive the message key for the initial (0x00) message using `kdf_chain_step`: [REQ-MSG-036]

```python
new_chain_key, message_key = kdf_chain_step(initial_chain_key)
```

After encrypting the initial message:
- Sender stores `new_chain_key` as `sendingChainKey` with `sendingChainIndex = 1`
- Only ONE message per session MAY use extension type 0x00 [REQ-MSG-037]
- All subsequent messages MUST use extension type 0x01 (Ratchet) [REQ-MSG-038]

**AAD Definition (Normative):** The ChaCha20-Poly1305 AAD for initial messages MUST be the exact 33-byte PUSE header extension bytes: [REQ-MSG-039]
```
initial_extension_bytes = 0x00 || ephemeral_public_key(32)
```

**Receiver Processing for Initial (0x00) Message (Normative):**

When receiving an initial (0x00) message, the recipient MUST: [REQ-MSG-040]

1. Derive `initial_chain_key` from the 2DH exchange using the ephemeral key from the header extension
2. Compute `new_chain_key, message_key = kdf_chain_step(initial_chain_key)`
3. Decrypt the ciphertext using `message_key`
4. Store `new_chain_key` as the receiving chain key for the sender's DH public key
5. Set the receiving chain index to `1` (because message N=0 was just consumed)

This ensures the receiver's state is synchronized for subsequent 0x01 messages from the same sender, which will have `n >= 1`.

**Transition to Ratchet Messages (0x00 → 0x01) (Normative):**

After the initial (0x00) message, subsequent messages from the **same sender** use extension type 0x01 with the following constraints:

1. **DH public key**: Reuse the same ephemeral public key from the 0x00 message until a DH ratchet occurs
2. **Message number N**: Start at 1 (because the 0x00 message is effectively N=0 of that sending chain)
3. **Previous chain length PN**: 0 (no previous sending chain exists at session start)

The sender's second message (first 0x01 message) will have:
- `dh_public`: Same as the ephemeral key sent in the 0x00 header
- `pn`: 0
- `n`: 1

A DH ratchet step occurs when the sender receives a message from the recipient (which includes the recipient's new DH public key).

#### 3.4.3 Ratchet Extension (Type 0x01)

For ongoing Double Ratchet messages:

```
Ratchet Header Extension:
┌────────────────────────────────────────┐
│ Extension Type: 0x01                   │ 1 byte
├────────────────────────────────────────┤
│ DH Public Key (X25519)                 │ 32 bytes
├────────────────────────────────────────┤
│ Previous Chain Length (PN)             │ 4 bytes (big-endian)
├────────────────────────────────────────┤
│ Message Number (N)                     │ 4 bytes (big-endian)
└────────────────────────────────────────┘
Total: 41 bytes
```

**Counter Definitions (Normative):**
- **N (Message Number):** 0-indexed message number in the current sending chain. First message sent with a new DH key has N=0.
- **PN (Previous Chain Length):** Total number of messages sent in the previous sending chain before the DH ratchet step. When starting a new session or after a DH ratchet, the sender records how many messages were sent with the old DH key.

#### 3.4.4 Group Extension (Type 0x02)

For group sender-key messages:

```
Group Header Extension:
┌────────────────────────────────────────┐
│ Extension Type: 0x02                   │ 1 byte
├────────────────────────────────────────┤
│ Sender Key ID                          │ 16 bytes
├────────────────────────────────────────┤
│ Sender Key Iteration                   │ 4 bytes (big-endian)
└────────────────────────────────────────┘
Total: 21 bytes
```

**Iteration Counter (Normative):**
- Sender Key Iteration is **1-indexed**: the first message encrypted with a new sender key has `iteration = 1`
- Value 0 is invalid; implementations MUST reject group envelopes with `iteration = 0` [REQ-MSG-041]
- Each message increments the iteration counter; receivers use this to derive the correct message key
- Maximum value: 2^32 - 1; after this, rotate the sender key

### 3.5 Nonce Generation

The 12-byte nonce MUST be constructed as: [REQ-MSG-042]

```
Nonce:
┌────────────────────────────────────────┐
│ Timestamp (seconds since epoch, BE)    │ 4 bytes
├────────────────────────────────────────┤
│ Random                                 │ 8 bytes
└────────────────────────────────────────┘
```

**Requirements:**
- Timestamp SHOULD be current time (for replay correlation) [REQ-MSG-043]
- Receivers MUST NOT reject messages based on timestamp age (messages may be delivered via mailbox days later) [REQ-MSG-044]
- Receivers MAY reject messages with timestamps more than 24 hours in the future [REQ-MSG-045]
- Random bytes MUST come from a CSPRNG [REQ-MSG-046]
- (key, nonce) pairs MUST never be reused [REQ-MSG-047]

**Nonce Uniqueness:** The combination of a per-message key (from the ratchet) and the timestamp+random nonce provides strong uniqueness guarantees. Since each message key is used exactly once (derived from a single `kdf_chain_step` call), nonce collision would only matter if the same message key were reused, which the protocol forbids.

### 3.6 AEAD Construction

Encryption uses ChaCha20-Poly1305 with:
- **Key**: Message key (derived from session protocol, see §4)
- **Nonce**: 12-byte nonce from envelope
- **AAD**: Header extension bytes (authenticated but not encrypted)
- **Plaintext**: Message payload (see §8)

```
ciphertext || tag = ChaCha20-Poly1305(
  key = message_key,
  nonce = envelope.nonce,
  aad = envelope.header_extension,
  plaintext = payload
)
```

The 16-byte Poly1305 tag is included in the ciphertext. Thus, `ciphertext_length` (the 4-byte field in §3.2) MUST equal `len(plaintext) + 16` (the Poly1305 tag size). [REQ-MSG-048]

### 3.7 Signature

The signature covers all bytes of the envelope BEFORE the signature field:

```
signed_data = envelope[0 : total_length - 64]

signature = Ed25519_Sign(sender_signing_key, signed_data)
```

**Note:** The signature is NOT over a hash - it signs the raw envelope bytes directly. Ed25519 internally hashes the message.

### 3.8 Parsing Order

Receivers MUST parse in this order for streaming support: [REQ-MSG-049]

1. Read fixed header (64 bytes): magic through header_extension_length
2. Read header_extension (length from step 1)
3. Read nonce (12 bytes)
4. Read ciphertext_length (4 bytes)
5. Read ciphertext (length from step 4)
6. Read signature (64 bytes)
7. Fetch sender identity document (from cache or DHT, using sender IID from header)
8. Verify signature over bytes from steps 1-5 using sender's signing keys
9. Derive message key based on extension type
10. Decrypt ciphertext

**Trailing Bytes Rule (Normative):** Parsers MUST reject PUSE envelopes where the total byte count does not exactly match the computed envelope size: `64 + ext_len + 12 + 4 + ct_len + 64`. Trailing bytes MUST NOT be silently ignored. [REQ-MSG-050]

**Signature Verification Key Selection (Normative):**

Verifiers MUST attempt signature verification using keys in order: [REQ-MSG-051]
1. `keys.signing.current`
2. `keys.signing.previous` (if present)
3. ALL `keys.signing.history[]` entries (regardless of `expires_at`)

Accept the envelope if ANY key successfully verifies the signature. This handles mailbox-delayed messages where the sender may have rotated keys after sending.

The `expires_at` field is metadata for UI warnings and audit trails; it MUST NOT be used to reject signatures during verification. [REQ-MSG-052]

**`expires_at` Usage (Informative):**

| Context | Usage | Notes |
|---------|-------|-------|
| UI display | MAY show warning for expired keys | "Verified with expired key" indicator  [REQ-MSG-053]|
| Audit logs | SHOULD record which key verified | Include `expires_at` status in logs  [REQ-MSG-054]|
| Signature verification | MUST NOT reject based on `expires_at` | Always try all keys  [REQ-MSG-055]|

For PUSE message verification, the recommended behavior is:
- Accept if ANY key verifies (current → previous → all history)
- Log a warning if the verifying key's `expires_at` has passed
- Application MAY present a UI indicator for "verified with expired key" [REQ-MSG-056]

**Note:** Steps 7-8 require the sender's identity document before signature verification can proceed. Implementations MAY cache identity documents to avoid repeated fetches. [REQ-MSG-057]

---

## 4. Session Protocol (Double Ratchet)

### 4.1 Overview

The Double Ratchet provides forward secrecy and break-in recovery for 1:1 conversations. Each message uses a unique message key derived from an evolving chain.

### 4.2 Key Derivation Functions

#### 4.2.1 Chain Step KDF

Advances a symmetric chain, returning new chain key and message key:

```python
def kdf_chain_step(chain_key: bytes) -> tuple[bytes, bytes]:
    """
    Returns (new_chain_key, message_key).
    Uses HMAC-SHA256 with single-byte domain constants.
    """
    message_key = HMAC_SHA256(key=chain_key, data=b"\x01")
    new_chain_key = HMAC_SHA256(key=chain_key, data=b"\x02")
    return new_chain_key, message_key
```

**Domain separator:** The constants `0x01` and `0x02` provide domain separation between message key and chain key derivation.

#### 4.2.2 Root Chain KDF

Advances root chain with DH output, returning new root key and chain key:

```python
def kdf_root(root_key: bytes, dh_output: bytes) -> tuple[bytes, bytes]:
    """
    Returns (new_root_key, new_chain_key).
    Uses HKDF-SHA256 with proper domain separation.
    """
    prk = HKDF_Extract(salt=root_key, ikm=dh_output)
    derived = HKDF_Expand(prk=prk, info=b"post-urbit-ratchet-v1", length=64)
    return derived[0:32], derived[32:64]
```

**Domain separator:** `post-urbit-ratchet-v1` (21 ASCII bytes)

#### 4.2.3 Initial Key Derivation

Derives initial root key from 2DH outputs:

```python
def kdf_initial(dh1: bytes, dh2: bytes, iid_a: bytes, iid_b: bytes) -> tuple[bytes, bytes]:
    """
    Derives (root_key, initial_chain_key) from 2DH outputs.
    IIDs are raw 20-byte identifiers (not Base32 strings).
    """
    ikm = dh1 + dh2  # 64 bytes

    # Salt includes sorted IIDs for consistent derivation
    if iid_a < iid_b:
        salt = iid_a + iid_b
    else:
        salt = iid_b + iid_a

    prk = HKDF_Extract(salt=salt, ikm=ikm)
    derived = HKDF_Expand(prk=prk, info=b"post-urbit-x3dh-v1", length=64)
    return derived[0:32], derived[32:64]
```

**Domain separator:** `post-urbit-x3dh-v1` (18 ASCII bytes)

**IID Sorting (Normative):** The comparison `iid_a < iid_b` MUST be performed as **bytewise lexicographic comparison of the raw 20-byte IID values**: [REQ-MSG-058]
- Compare byte-by-byte from index 0 to 19
- The first differing byte determines the ordering (lower byte value = smaller IID)
- If all 20 bytes are equal, the IIDs are equal

Implementations MUST NOT sort by Base32 string representation, as Base32 ordering differs from raw byte ordering for some IID pairs. This rule applies to all places in this specification where IIDs are sorted or compared (including `kdf_initial`, sender key derivation, and any future extensions). [REQ-MSG-059]

### 4.3 Ratchet State

Each peer maintains ratchet state per conversation:

```typescript
interface RatchetState {
  // Peer identity
  peerIid: Uint8Array;              // 20 bytes, raw

  // DH ratchet keys
  dhSendingKey: {
    private: Uint8Array;            // 32 bytes
    public: Uint8Array;             // 32 bytes
  } | null;
  dhReceivingKey: Uint8Array | null; // 32 bytes, peer's current DH public

  // Root chain
  rootKey: Uint8Array;              // 32 bytes

  // Sending chain
  sendingChainKey: Uint8Array | null; // 32 bytes
  previousSendingChainLength: number; // PN for current sending chain (persists until next DH ratchet)
  sendingChainIndex: number;        // uint32

  // Receiving chains (may have multiple for out-of-order)
  receivingChains: Map<string, {    // Key: hex of peer DH public
    chainKey: Uint8Array;
    chainIndex: number;
  }>;

  // Skipped message keys (for out-of-order delivery)
  skippedKeys: Map<string, {        // Key: "dhPubHex:index"
    messageKey: Uint8Array;
    storedAt: Timestamp;
  }>;

  // Limits
  maxSkip: number;                  // Default: 100
}
```

### 4.4 Sending a Message

```python
def ratchet_encrypt(state: RatchetState, plaintext: bytes) -> tuple[Header, bytes]:
    """
    Encrypt a message using the Double Ratchet.
    Returns (ratchet_header, nonce, ciphertext).

    Counter semantics:
    - N is 0-indexed: first message in a chain has N=0
    - PN is the count of messages sent in the PREVIOUS sending chain
    - PN is stored in state and used for ALL messages in the current chain
    """
    # If no sending chain, perform DH ratchet step
    if state.sending_chain_key is None:
        # Record previous chain length (PN) BEFORE ratcheting
        # This value persists for all messages sent with the new DH key
        state.previous_sending_chain_length = state.sending_chain_index

        state.dh_sending_key = generate_x25519_keypair()
        dh_output = x25519(state.dh_sending_key.private, state.dh_receiving_key)
        state.root_key, state.sending_chain_key = kdf_root(state.root_key, dh_output)
        state.sending_chain_index = 0

    # Current message number (N) before derivation
    n = state.sending_chain_index

    # Derive message key and advance chain
    state.sending_chain_key, message_key = kdf_chain_step(state.sending_chain_key)
    state.sending_chain_index += 1

    # Build ratchet header
    # PN = messages sent in previous chain (stored in state); N = current message number (0-indexed)
    header = RatchetHeader(
        dh_public=state.dh_sending_key.public,
        pn=state.previous_sending_chain_length,
        n=n
    )

    # Encrypt (nonce generated per §3.5)
    # AAD is the 41-byte ratchet extension: 0x01 || dh_public(32) || PN(u32-be) || N(u32-be)
    nonce = generate_nonce()
    ratchet_extension_bytes = header.encode()  # MUST return exact PUSE header extension bytes
    ciphertext = chacha20_poly1305_encrypt(
        key=message_key,
        nonce=nonce,
        aad=ratchet_extension_bytes,
        plaintext=plaintext
    )

    # Securely delete message key
    secure_zero(message_key)

    return header, nonce, ciphertext
```

**Message Key Derivation:** The message key for message number N is derived by applying `kdf_chain_step` once to the chain key. After derivation, the chain key advances to enable the next message. This means message N=0 uses the first `kdf_chain_step` output from the initial chain key.

**AAD Definition (Normative):** The ChaCha20-Poly1305 AAD for ratchet messages MUST be the exact 41-byte PUSE header extension bytes: [REQ-MSG-060]
```
ratchet_extension_bytes = 0x01 || dh_public(32) || PN(u32-be) || N(u32-be)
```
Both sender and receiver MUST use this exact byte sequence. The `header.encode()` method MUST return these 41 bytes verbatim. [REQ-MSG-061]

### 4.5 Receiving a Message

```python
def ratchet_decrypt(state: RatchetState, header: RatchetHeader,
                    nonce: bytes, ciphertext: bytes) -> bytes:
    """
    Decrypt a message using the Double Ratchet.

    Counter semantics (matching §4.4):
    - header.n is 0-indexed: the first message in a chain has n=0
    - chainIndex tracks the next expected message number
    - Skipped keys are stored for indices [chainIndex, header.n)
    """
    # AAD is the parsed 41-byte header extension (see §4.4 AAD Definition)
    ratchet_extension_bytes = header.encode()

    # Check for skipped message key
    skip_key = f"{header.dh_public.hex()}:{header.n}"
    if skip_key in state.skipped_keys:
        entry = state.skipped_keys.pop(skip_key)
        plaintext = chacha20_poly1305_decrypt(
            key=entry.message_key,
            nonce=nonce,
            aad=ratchet_extension_bytes,
            ciphertext=ciphertext
        )
        secure_zero(entry.message_key)
        return plaintext

    # If new DH key from peer, perform DH ratchet
    if header.dh_public != state.dh_receiving_key:
        # Store skipped keys from previous receiving chain
        skip_message_keys(state, state.dh_receiving_key, header.pn)

        # DH ratchet step
        state.dh_receiving_key = header.dh_public
        dh_output = x25519(state.dh_sending_key.private, state.dh_receiving_key)
        state.root_key, receiving_chain_key = kdf_root(state.root_key, dh_output)

        # Clear sending chain (will ratchet on next send)
        state.sending_chain_key = None

        # Store new receiving chain
        state.receiving_chains[header.dh_public.hex()] = {
            "chainKey": receiving_chain_key,
            "chainIndex": 0
        }

    # Get receiving chain
    chain = state.receiving_chains[header.dh_public.hex()]

    # Skip ahead if needed (out-of-order messages)
    while chain["chainIndex"] < header.n:
        if len(state.skipped_keys) >= state.max_skip:
            raise TooManySkippedMessagesError()
        chain["chainKey"], skipped_key = kdf_chain_step(chain["chainKey"])
        state.skipped_keys[f"{header.dh_public.hex()}:{chain['chainIndex']}"] = {
            "messageKey": skipped_key,
            "storedAt": now()
        }
        chain["chainIndex"] += 1

    # Derive message key
    chain["chainKey"], message_key = kdf_chain_step(chain["chainKey"])
    chain["chainIndex"] += 1

    # Decrypt (AAD was extracted at function start)
    plaintext = chacha20_poly1305_decrypt(
        key=message_key,
        nonce=nonce,
        aad=ratchet_extension_bytes,
        ciphertext=ciphertext
    )

    secure_zero(message_key)
    return plaintext
```

### 4.6 Skipped Key Management

Skipped keys allow out-of-order message delivery:

- **Maximum skipped keys:** 100 per conversation (configurable)
- **TTL:** 7 days (delete after)
- **Storage:** Encrypted at rest with device key

---

## 5. Initial Key Exchange (2DH)

### 5.1 Overview

The initial key exchange establishes the shared secret when starting a new conversation. It uses two Diffie-Hellman operations (2DH) combining identity and ephemeral keys.

**Note on Naming:** This protocol is intentionally simpler than Signal's X3DH, which uses signed prekeys for stronger asynchronous properties. Post-Urbit v1 uses 2DH because:
- Identity keys are always available via DHT
- Prekey management adds significant complexity
- The Double Ratchet provides forward secrecy after the first message exchange

Future versions may add signed prekeys for X3DH-equivalent security if needed.

### 5.2 Key Material

Each identity publishes in their identity document:
- **Identity Key (IK):** Long-term X25519 key (`keys.encryption.current`)
- **Previous Keys:** Historical encryption keys (`keys.encryption.previous[]`) for recipients who haven't updated

The initiator generates:
- **Ephemeral Key (EK):** One-time X25519 key pair

**Key Selection Rule:** Always use `keys.encryption.current`. If the recipient rotated keys and the sender has an outdated identity document, the recipient can still decrypt using keys from `previous[]` (matching by key bytes or identity document sequence).

### 5.3 Protocol

**Alice wants to message Bob (first time):**

1. Alice retrieves Bob's identity document (prefer fresh fetch from DHT)
2. Alice extracts Bob's identity key: `IK_B = bob.keys.encryption.current`
3. Alice generates ephemeral key pair: `EK_A = generate_x25519_keypair()`
4. Alice computes DH outputs:
   ```
   DH1 = X25519(IK_A_private, IK_B)    // Alice identity × Bob identity
   DH2 = X25519(EK_A_private, IK_B)    // Alice ephemeral × Bob identity
   ```
5. Alice derives initial keys:
   ```
   root_key, initial_chain_key = kdf_initial(DH1, DH2, alice_iid, bob_iid)
   ```
6. Alice initializes ratchet state with Bob's IK as `dh_receiving_key`
7. Alice sends initial message with:
   - Header extension type 0x00
   - Ephemeral public key `EK_A_public` in extension
   - Message encrypted with key derived from `initial_chain_key`

**Bob receives initial message:**

1. Bob retrieves Alice's identity document
2. Bob extracts Alice's ephemeral key from header extension: `EK_A_public`
3. Bob tries Alice's encryption keys in order until decryption succeeds:
   - `keys.encryption.current` (most common case)
   - `keys.encryption.previous[]` entries whose `expires_at` has not passed
4. For each candidate `IK_A`:
   ```
   DH1 = X25519(IK_B_private, IK_A)    // Bob identity × Alice identity
   DH2 = X25519(IK_B_private, EK_A)    // Bob identity × Alice ephemeral
   root_key, initial_chain_key = kdf_initial(DH1, DH2, alice_iid, bob_iid)
   message_key = kdf_chain_step(initial_chain_key)[1]
   ```
5. Attempt AEAD decryption with `message_key`
6. If decryption fails, try next candidate key; if all fail, reject envelope
7. Bob initializes ratchet state with successful keys

**Encryption Key Trial Order (Normative):**

For 2DH initial message decryption, receivers MUST try sender encryption keys in this order: [REQ-MSG-062]
1. `keys.encryption.current`
2. `keys.encryption.previous[]` entries (newest first) whose `expires_at` has not passed

This handles mailbox-delayed messages where the sender may have rotated encryption keys after sending. The trial decryption approach is necessary because the initial message header carries no sender key identifier.

### 5.4 Security Properties

| Property | Guarantee |
|----------|-----------|
| Forward secrecy | Yes (via ephemeral key) |
| Key confirmation | No (Alice doesn't know Bob received) |
| Replay protection | Via message ID and nonce |
| Identity binding | DH1 binds to both identity keys |

### 5.5 Initial Ratchet State Setup (Normative)

After the 2DH exchange completes, both parties MUST initialize their ratchet state as follows: [REQ-MSG-063]

**Alice (initiator):**
```python
def initialize_ratchet_initiator(
    root_key: bytes,           # 32 bytes from kdf_initial
    initial_chain_key: bytes,  # 32 bytes from kdf_initial
    bob_identity_key: bytes,   # 32 bytes, Bob's IK public
    bob_iid: bytes,            # 20 bytes, Bob's IID
    alice_ephemeral_keypair: tuple  # (private, public) from 2DH setup
) -> RatchetState:
    state = RatchetState()
    state.peerIid = bob_iid

    # Alice sets Bob's identity key as receiving key
    state.dhReceivingKey = bob_identity_key

    # Alice keeps her ephemeral key as sending key
    # (needed to DH-ratchet when Bob replies with his new key)
    state.dhSendingKey = {
        "private": alice_ephemeral_keypair[0],
        "public": alice_ephemeral_keypair[1]
    }

    # Root key from 2DH
    state.rootKey = root_key

    # Alice uses initial_chain_key as her SENDING chain
    # (she sends first, Bob receives)
    state.sendingChainKey = initial_chain_key
    state.sendingChainIndex = 0
    state.previousSendingChainLength = 0  # No previous chain at session start

    # Alice has no receiving chains yet
    state.receivingChains = {}

    # Empty skipped keys
    state.skippedKeys = {}
    state.maxSkip = 100

    return state
```

**Bob (responder):**
```python
def initialize_ratchet_responder(
    root_key: bytes,           # 32 bytes from kdf_initial
    initial_chain_key: bytes,  # 32 bytes from kdf_initial
    alice_ephemeral_key: bytes, # 32 bytes, Alice's EK public
    alice_iid: bytes           # 20 bytes, Alice's IID
) -> RatchetState:
    state = RatchetState()
    state.peerIid = alice_iid

    # Bob sets Alice's ephemeral key as receiving key
    state.dhReceivingKey = alice_ephemeral_key

    # Bob generates his DH key for his first reply
    state.dhSendingKey = generate_x25519_keypair()

    # Root key from 2DH
    state.rootKey = root_key

    # Bob uses initial_chain_key as his RECEIVING chain
    # (Alice sends first, Bob receives)
    state.sendingChainKey = None
    state.sendingChainIndex = 0
    state.previousSendingChainLength = 0  # No previous chain at session start

    state.receivingChains = {
        alice_ephemeral_key.hex(): {
            "chainKey": initial_chain_key,
            "chainIndex": 0
        }
    }

    # Empty skipped keys
    state.skippedKeys = {}
    state.maxSkip = 100

    return state
```

**Key Asymmetry:** The initiator (Alice) and responder (Bob) have opposite initial states:
- Alice uses `initial_chain_key` as her sending chain
- Bob uses `initial_chain_key` as his receiving chain (keyed by Alice's ephemeral key)

This ensures the first message Alice sends can be decrypted by Bob using the shared `initial_chain_key`.

---

## 6. Group Messaging

### 6.1 Overview

Group messaging uses **sender keys** for efficiency: each sender encrypts once for the entire group, rather than once per recipient.

### 6.2 Sender Key Structure

```typescript
interface SenderKey {
  keyId: Uint8Array;        // 16 bytes, random identifier
  senderIid: Uint8Array;    // 20 bytes, sender's IID (raw)
  groupId: Uint8Array;      // 20 bytes, group ID (raw)
  chainKey: Uint8Array;     // 32 bytes, current chain key
  iteration: number;        // uint32, messages encrypted with this key
  createdAt: Timestamp;
}
```

### 6.3 Sender Key KDF

```python
def kdf_sender_key(chain_key: bytes, group_id: bytes, sender_iid: bytes,
                   key_id: bytes) -> tuple[bytes, bytes]:
    """
    Advance sender key chain for group messaging.
    Returns (new_chain_key, message_key).
    Domain separation binds to group, sender, and key ID.
    """
    # Domain-separated info string
    info = b"post-urbit-sender-key-v1:" + group_id + b":" + sender_iid + b":" + key_id

    message_key = HMAC_SHA256(key=chain_key, data=b"\x01" + info)
    new_chain_key = HMAC_SHA256(key=chain_key, data=b"\x02" + info)

    return new_chain_key, message_key
```

**Domain separator:** `post-urbit-sender-key-v1:` (25 ASCII bytes) + binding data

### 6.4 Key Distribution

Sender keys are distributed via 1:1 ratchet messages:

```json
{
  "type": "sender_key_share",
  "content": {
    "group_id": "<32-char-base32-group-id>",
    "sender_iid": "<32-char-base32-iid>",
    "key_id": "<16-bytes-base64>",
    "chain_key": "<32-bytes-base64>",
    "iteration": 0
  }
}
```

**Encoding note:** Identifiers (group_id, sender_iid) use Crockford Base32 consistent with all other identifier fields. Cryptographic material (key_id, chain_key) uses Base64 standard encoding.

**Distribution triggers:**
- Group creation: Creator shares with all initial members
- New member: All existing members share their keys
- Key rotation: Member shares new key with all others

### 6.5 Key Rotation

Sender keys SHOULD rotate: [REQ-MSG-064]
- Every 100 messages
- Every 7 days
- After any member leaves (security measure)

### 6.6 Group Encryption Flow

```python
def group_encrypt(sender_key: SenderKey, plaintext: bytes) -> tuple[GroupHeader, bytes]:
    """
    Encrypt a group message using sender key.
    """
    # Derive message key
    sender_key.chain_key, message_key = kdf_sender_key(
        sender_key.chain_key,
        sender_key.group_id,
        sender_key.sender_iid,
        sender_key.key_id
    )
    sender_key.iteration += 1

    # Build group header
    header = GroupHeader(
        sender_key_id=sender_key.key_id,
        iteration=sender_key.iteration
    )

    # Encrypt
    # AAD is the 21-byte group extension: 0x02 || sender_key_id(16) || iteration(u32-be)
    nonce = generate_nonce()
    group_extension_bytes = header.encode()  # MUST return exact PUSE header extension bytes
    ciphertext = chacha20_poly1305_encrypt(
        key=message_key,
        nonce=nonce,
        aad=group_extension_bytes,
        plaintext=plaintext
    )

    secure_zero(message_key)
    return header, nonce, ciphertext
```

### 6.7 Group Decryption Flow

**Group AAD Definition (Normative):** The ChaCha20-Poly1305 AAD for group messages MUST be the exact 21-byte PUSE header extension bytes: [REQ-MSG-065]
```
group_extension_bytes = 0x02 || sender_key_id(16) || iteration(u32-be)
```
Both sender and receiver MUST use this exact byte sequence. The `header.encode()` method MUST return these 21 bytes verbatim. [REQ-MSG-066]

```python
def group_decrypt(sender_iid: bytes, group_id: bytes, header: GroupHeader,
                  nonce: bytes, ciphertext: bytes,
                  sender_keys: dict) -> bytes:
    """
    Decrypt a group message.
    """
    # AAD is the parsed 21-byte header extension (see Group AAD Definition)
    group_extension_bytes = header.encode()

    # Look up sender key
    key_lookup = f"{sender_iid.hex()}:{header.sender_key_id.hex()}"
    if key_lookup not in sender_keys:
        raise UnknownSenderKeyError()

    sender_key = sender_keys[key_lookup]

    # Verify iteration (prevent replay)
    if header.iteration <= sender_key.last_seen_iteration:
        raise ReplayDetectedError()

    # Skip ahead to iteration - 1 (discard skipped keys)
    # Note: header.iteration is 1-indexed (first message has iteration=1)
    # local_iteration tracks how many keys we've derived (0 = none yet)
    while sender_key.local_iteration < header.iteration - 1:
        sender_key.chain_key, _ = kdf_sender_key(
            sender_key.chain_key,
            group_id,
            sender_iid,
            sender_key.key_id
        )
        sender_key.local_iteration += 1

    # Derive the actual message key for this iteration
    sender_key.chain_key, message_key = kdf_sender_key(
        sender_key.chain_key,
        group_id,
        sender_iid,
        sender_key.key_id
    )
    sender_key.local_iteration += 1
    sender_key.last_seen_iteration = header.iteration

    # Decrypt
    plaintext = chacha20_poly1305_decrypt(
        key=message_key,
        nonce=nonce,
        aad=group_extension_bytes,
        ciphertext=ciphertext
    )

    secure_zero(message_key)
    return plaintext
```

---

## 7. Mailbox Protocol

### 7.1 Overview

Mailboxes provide store-and-forward delivery for offline recipients. They are untrusted intermediaries that store encrypted PUSE envelopes.

### 7.2 Mailbox Discovery

Each identity MAY publish mailbox endpoints in their identity document using the standard endpoint schema (see RFC-0001 §4): [REQ-MSG-067]

```json
{
  "endpoints": [
    {
      "type": "mailbox",
      "host": "mailbox.example.com",
      "port": 443,
      "transport": "https",
      "priority": 30
    }
  ]
}
```

### 7.3 Authentication

Clients authenticate to mailboxes using bearer tokens signed by their identity:

```json
{
  "iid": "<sender-iid-base32>",
  "mailbox_url": "https://mailbox.example.com",
  "expires_at": "<RFC3339>",
  "nonce": "<16-bytes-base64url>",
  "signature": "<64-bytes-base64>"
}
```

**URL Canonicalization (Normative):**

Before signing, `mailbox_url` MUST be canonicalized: [REQ-MSG-068]

1. **Scheme:** Lowercase (`https` not `HTTPS`)
2. **Host:** Lowercase ASCII only; MUST be ASCII-compatible encoding (punycode A-label); non-ASCII Unicode hosts MUST be rejected. No trailing dot. IPv6 literals MUST be bracketed and lowercase (e.g., `[2001:db8::1]`) [REQ-MSG-069]
3. **Port:** Omit default port (`:443` for https, `:80` for http)
4. **Path:** Normalize empty path to `/`. Then, if the path is not exactly `/`, remove **all** trailing `/` bytes (not just one). Path is case-sensitive. Internal `//` sequences are preserved (no path segment normalization). Dot-segments (`.` and `..`) are NOT normalized (they are preserved as-is).
5. **Percent-encoding:** Normalize to uppercase hex (e.g., `%2f` → `%2F`). Unreserved characters (A-Z, a-z, 0-9, `-._~`) MUST NOT be percent-encoded. Already-decoded unreserved characters MUST be kept decoded. **Invalid percent-escapes** (a `%` not followed by exactly two hex digits, e.g., `%`, `%G0`, `%0`) MUST cause rejection—do not attempt to preserve or normalize them. [REQ-MSG-070]
6. **No query or fragment:** Mailbox URLs MUST NOT contain query strings or fragments; implementations MUST reject (not silently strip) URLs with `?` or `#` [REQ-MSG-071]
7. **Scheme required:** Mailbox URLs MUST use `https` scheme; implementations MUST reject `http` or other schemes [REQ-MSG-072]
8. **No userinfo:** Mailbox URLs MUST NOT contain userinfo (`user@` or `user:pass@`); implementations MUST reject URLs with userinfo [REQ-MSG-073]
9. **Host required:** Mailbox URLs MUST contain a valid host; implementations MUST reject URLs without a host [REQ-MSG-074]
10. **Verifiers MUST:** Reject tokens where `mailbox_url` is not already in canonical form. Canonicalize before computing signature input for verification. [REQ-MSG-075]

Examples:
- `HTTPS://Mailbox.Example.COM:443/` → `https://mailbox.example.com/`
- `https://relay.net:8443/api/` → `https://relay.net:8443/api`
- `https://box.org` → `https://box.org/`
- `https://[2001:DB8::1]:443/` → `https://[2001:db8::1]/`
- `https://example.com/Path%2fWith%2fSlashes` → `https://example.com/Path%2FWith%2FSlashes` (uppercase hex)
- `https://müller.example/` → **REJECT** (non-ASCII host; use punycode `xn--mller-kva.example` instead)
- `http://example.com/` → **REJECT** (wrong scheme)
- `https://user@example.com/` → **REJECT** (has userinfo)

**v1 Path Restriction (Normative):**

For v1, the mailbox endpoint schema (RFC-0001 §4.6) does not include a `path` field. Implementations MUST derive mailbox URLs with root path `/` only. Non-root paths (e.g., `/api`) are shown in the examples above to illustrate canonicalization rules, but v1 deployments MUST use root-only mailbox URLs. Specifically: [REQ-MSG-076]
- v1 implementations MUST reject tokens where `mailbox_url` has a non-root path [REQ-MSG-077]
- v1 implementations MUST NOT generate tokens with non-root `mailbox_url` [REQ-MSG-078]

See `spec/00-shared/layer-integration.md` "Mailbox Base URL Derivation" for the normative URL construction from endpoint schema.

**Reference Implementation (Illustrative):**

The following pseudocode is illustrative. Implementations MUST follow all normative rules above, including percent-encoding normalization. Test vectors at the end of this section are authoritative. [REQ-MSG-079]

```python
import re
from urllib.parse import urlparse, urlunparse

UNRESERVED = set('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~')

def normalize_percent_encoding(s: str) -> str:
    """Normalize percent-encoding: uppercase hex digits, decode unreserved characters."""
    result = []
    i = 0
    while i < len(s):
        if s[i] == '%':
            # MUST have exactly 2 hex digits following
            if i + 2 >= len(s):
                raise ValueError("Truncated percent-escape at end of string")
            hex_chars = s[i+1:i+3]
            if not all(c in '0123456789ABCDEFabcdef' for c in hex_chars):
                raise ValueError(f"Invalid percent-escape: %{hex_chars}")
            byte_val = int(hex_chars, 16)
            char = chr(byte_val)
            if char in UNRESERVED:
                # Decode unreserved characters to literal form
                result.append(char)
            else:
                # Keep as percent-encoded, uppercase hex
                result.append('%' + hex_chars.upper())
            i += 3
        else:
            result.append(s[i])
            i += 1
    return ''.join(result)

def canonicalize_mailbox_url(url: str) -> str:
    p = urlparse(url)
    # Reject invalid URLs
    if p.query or p.fragment:
        raise ValueError("Mailbox URLs MUST NOT contain query or fragment")
    if p.scheme.lower() != 'https':
        raise ValueError("Mailbox URLs MUST use https scheme")
    if p.username or p.password:
        raise ValueError("Mailbox URLs MUST NOT contain userinfo")
    if not p.hostname:
        raise ValueError("Mailbox URLs MUST contain a host")
    # Check for non-ASCII in host (reject, require punycode)
    if any(ord(c) > 127 for c in p.hostname):
        raise ValueError("Non-ASCII hosts MUST be punycode-encoded")
    # Lowercase scheme
    scheme = p.scheme.lower()
    # Handle host (lowercase, preserve IPv6 brackets)
    host = p.hostname.lower().rstrip('.')
    if ':' in host:  # IPv6 literal
        host = f'[{host}]'
    port = p.port
    # Omit default port
    if port == 443:
        port = None
    # Normalize path with percent-encoding normalization
    path = normalize_percent_encoding(p.path) or '/'
    if len(path) > 1 and path.endswith('/'):
        path = path.rstrip('/')
    netloc = host + (f':{port}' if port else '')
    return urlunparse((scheme, netloc, path, '', '', ''))
```

**Canonicalization Test Vectors (Normative):**

| Input | Output |
|-------|--------|
| `HTTPS://Mailbox.Example.COM:443/` | `https://mailbox.example.com/` |
| `https://relay.net:8443/api/` | `https://relay.net:8443/api` |
| `https://box.org` | `https://box.org/` |
| `https://[2001:DB8::1]:443/` | `https://[2001:db8::1]/` |
| `https://example.com/path%2fslash` | `https://example.com/path%2Fslash` |
| `https://example.com/%41%42%43` | `https://example.com/ABC` |
| `https://example.com/a%20b` | `https://example.com/a%20b` |
| `https://example.com/api///` | `https://example.com/api` |
| `https://example.com//double//slash` | `https://example.com//double//slash` |
| `https://example.com/./dotpath/../keep` | `https://example.com/./dotpath/../keep` |
| `https://müller.example/` | **REJECT** |
| `http://example.com/` | **REJECT** |
| `https://user@example.com/` | **REJECT** |
| `https://example.com/?query` | **REJECT** |

**Signature construction:**

**Ed25519 Signing API (Normative):** Mailbox token signatures use **standard Ed25519** (RFC 8032 PureEdDSA), NOT Ed25519ph or Ed25519ctx. When the code shows `ed25519_sign(key, sha256(data))`, the 32-byte SHA256 digest is the message input to the standard Ed25519 signing function.

**Domain separator:** `post-urbit-mailbox-token-v1` (27 ASCII bytes)

```python
def create_mailbox_token(iid: str, mailbox_url: str, signing_key: bytes) -> str:
    mailbox_url = canonicalize_mailbox_url(mailbox_url)  # MUST canonicalize
    expires_at = now() + timedelta(hours=24)
    nonce = generate_random(16)

    signature_input = concat(
        b"post-urbit-mailbox-token-v1",  # 27 bytes
        decode_base32(iid),               # 20 bytes
        mailbox_url.encode('utf-8'),      # variable
        expires_at.encode('utf-8'),       # 20 bytes canonical
        nonce                             # 16 bytes
    )

    signature = ed25519_sign(signing_key, sha256(signature_input))

    token = {
        "iid": iid,
        "mailbox_url": mailbox_url,
        "expires_at": expires_at,
        "nonce": base64url_encode(nonce),
        "signature": base64_encode(signature)
    }

    return base64url_encode(jcs_canonicalize(token))
```

**Bearer Token Wire Format (Normative):**

The `Authorization: Bearer <token>` header value MUST be constructed as: [REQ-MSG-080]

1. **JSON object:** Construct the token JSON with fields: `iid`, `mailbox_url`, `expires_at`, `nonce`, `signature`
2. **JCS canonicalization:** Apply JSON Canonicalization Scheme (RFC 8785) to produce deterministic UTF-8 bytes
3. **Base64url encoding:** Encode the JCS output using Base64url alphabet (RFC 4648 §5), **no padding**

The resulting string is the `<token>` value in `Authorization: Bearer <token>`.

**Field encodings within the JSON:**
- `iid`: Crockford Base32, lowercase, 32 characters
- `mailbox_url`: Canonicalized URL string (per rules above)
- `expires_at`: RFC 3339 timestamp, canonical form `YYYY-MM-DDTHH:MM:SSZ`
- `nonce`: Base64url-encoded, no padding, 22 characters (16 bytes raw)
- `signature`: Base64 standard alphabet, no padding, 86 characters (64 bytes raw)

**Verification:** Servers MUST reject tokens where any field encoding differs from these rules. [REQ-MSG-081]

**Token Expiration Validation (Normative):**

Mailbox servers MUST validate the `expires_at` field: [REQ-MSG-082]

1. **Past expiration:** Mailbox servers MUST reject tokens where `expires_at` is in the past (allowing ±5 minutes clock skew) [REQ-MSG-083]
2. **Excessive lifetime:** Mailbox servers MUST reject tokens where `expires_at` exceeds now + 24 hours [REQ-MSG-084]
3. **Client guidance:** Clients SHOULD use token lifetimes of 1-24 hours [REQ-MSG-085]

**Rationale:** Short-lived tokens limit the damage from token theft. The 5-minute clock skew tolerance accommodates minor time synchronization differences. The 24-hour maximum prevents long-lived tokens that could be abused if stolen.

### 7.4 Mailbox API

**Message ID Semantics (Normative):**
- Mailbox `message_id` MUST equal the PUSE header `message_id` (UUID v4) [REQ-MSG-086]
- The mailbox extracts this from the stored envelope's header (bytes 46-61, 0-indexed)
- All `message_id` values in store/retrieve/delete APIs use RFC 4122 canonical lowercase string format
- Mailbox MUST treat duplicate stores of the same `message_id` for the same recipient as idempotent (return success, do not store twice) [REQ-MSG-087]

#### 7.4.1 Store Message

```
POST /messages/{inbox_owner_iid}
Authorization: Bearer <token>
Content-Type: application/octet-stream

<PUSE envelope bytes>
```

**Path parameters:**
- `inbox_owner_iid`: The IID of the inbox owner (32-char Crockford Base32), i.e., the identity whose mailbox should receive this message

Response:
```json
{
  "message_id": "<uuid>",
  "stored_at": "<RFC3339>",
  "expires_at": "<RFC3339>"
}
```

Note: `message_id` in response equals the PUSE header `message_id` from the stored envelope.

**HTTP Response Codes (Normative):**
- `201 Created`: Message stored successfully (body as shown)
- `400 Bad Request`: Malformed envelope, invalid JSON, missing fields
- `401 Unauthorized`: Invalid or expired token
- `403 Forbidden`: Sender IID mismatch (token.iid != envelope.sender_iid)
- `413 Payload Too Large`: Envelope exceeds 1 MB
- `429 Too Many Requests`: Rate limit exceeded
- `507 Insufficient Storage`: Recipient quota exceeded

**Storage Keying and Routing (Normative):**

Mailbox servers MUST route and store messages by the **explicit inbox owner IID** from the URL path (not by parsing the PUSE envelope recipient field): [REQ-MSG-088]

1. Extract `inbox_owner_iid` from the URL path parameter
2. Decode it from Crockford Base32 to raw 20 bytes
3. Store the PUSE envelope as-is in the inbox keyed by this `inbox_owner_iid`
4. Treat `(inbox_owner_iid, message_id)` as the idempotency key for duplicate detection

**Why explicit inbox owner (not envelope recipient)?**

The PUSE envelope's `recipient` field (bytes 26-45) contains:
- For 1:1 messages (recipient_type=0x00): The recipient's IID
- For group messages (recipient_type=0x01): The **group_id** (not a member's IID)

For group messages, the sender fans out to each member's mailbox individually. Each store request targets a specific member's inbox via `inbox_owner_iid`, while the PUSE envelope (containing the group_id as recipient) is stored unchanged. This allows:
- Group messages to be delivered to offline members via their individual mailboxes
- Recipients to retrieve messages using their own IID
- The PUSE envelope to remain unmodified (preserving the group_id for decryption context)

**Sender Fanout for Group Messages (Normative):**

When sending a group message to offline members, the sender MUST: [REQ-MSG-089]
1. Encrypt the message once using the sender key (PUSE recipient = group_id)
2. For each offline member:
   a. Look up the member's mailbox endpoint from their identity document
   b. POST the same PUSE envelope to `POST /messages/{member_iid}` on that mailbox
3. Each mailbox stores the envelope under the respective member's IID

**Sender IID Binding (Normative):**
Mailbox servers MUST validate that the sender IID in the PUSE envelope header matches the IID in the authenticated bearer token: [REQ-MSG-090]
1. Parse the PUSE envelope header to extract `sender_iid` (bytes 6-25, raw 20 bytes)
2. Decode the `token.iid` from Base32 to raw 20 bytes
3. Compare the two 20-byte values using **constant-time byte comparison**
4. If the 20-byte values differ, reject with HTTP 403 Forbidden

**Note:** The Base32 decode of the token IID MUST use the same Crockford alphabet as RFC-0002 §2.1. Comparison MUST be on raw bytes, not on Base32 strings (to avoid case normalization issues). [REQ-MSG-091]

This prevents authenticated senders from spoofing messages as other identities. Recipients MAY still verify the full PUSE signature, but mailbox-level validation provides defense-in-depth. [REQ-MSG-092]

**Mailbox URL Binding (Normative):**

Mailbox servers MUST validate that `token.mailbox_url` matches the server's own canonical URL: [REQ-MSG-093]

1. Server computes its canonical mailbox URL using:
   - Scheme: MUST be `https` [REQ-MSG-094]
   - Host: Server's configured public hostname (NOT from request headers)
   - Port: Server's configured public port (omit if 443)
   - Path: Server's configured base path (default: empty)

2. Server MUST reject tokens where the canonicalized `token.mailbox_url` does not exactly match the server's canonical URL [REQ-MSG-095]

3. For deployments behind reverse proxies, the public host/port MUST be determined from server configuration, NOT from `Host` or `X-Forwarded-*` headers (which can be spoofed) [REQ-MSG-096]

This prevents token reuse attacks where a token generated for one mailbox could be used at another.

#### 7.4.2 Retrieve Messages

```
GET /messages?cursor=<opaque-cursor>&limit=100
Authorization: Bearer <token>
```

**Query parameters:**
- `cursor` (optional): Opaque cursor string from previous response; omit for initial request
- `limit` (optional): Maximum messages to return (1-1000, default 100)

Response:
```json
{
  "messages": [
    {
      "message_id": "<uuid>",
      "stored_at": "<RFC3339>",
      "sender_iid": "<base32>",
      "size": 1234,
      "envelope": "<base64-puse-envelope>"
    }
  ],
  "next_cursor": "<opaque-cursor-or-null>",
  "has_more": false
}
```

**Retrieval Filtering (Normative):**

Mailbox servers MUST return only messages stored in the authenticated identity's inbox: [REQ-MSG-097]

1. Decode `token.iid` from Base32 to raw 20 bytes - this is the **inbox owner**
2. Return all envelopes that were stored under this `inbox_owner_iid` (via the store endpoint path parameter)
3. The PUSE envelope's `recipient` field is NOT used for retrieval filtering

**Note on PUSE recipient field:** The returned envelopes may have different values in their PUSE `recipient` field (bytes 26-45):
- 1:1 messages: `recipient` equals the inbox owner's IID
- Group messages: `recipient` contains the group_id

The client determines message type by examining the PUSE flags (recipient_type bits 0-1) and processes accordingly. The mailbox treats all envelopes as opaque blobs indexed by inbox owner.

**Pagination Semantics (Normative):**
- Messages MUST be ordered by `stored_at` ascending, then `message_id` ascending (for tie-breaking) [REQ-MSG-098]
- `next_cursor` is an opaque string that, when used in subsequent requests, returns the next page
- `has_more` is true if more messages exist after this page
- `next_cursor` MUST be `null` when `has_more` is false [REQ-MSG-099]
- Cursor format is implementation-defined; clients MUST treat it as opaque [REQ-MSG-100]
- Cursors MAY expire after a reasonable time (recommended: 1 hour); expired cursors return HTTP 400 [REQ-MSG-101]

**Envelope Encoding (Normative):**
- The `envelope` field MUST be encoded using Base64 standard alphabet (RFC 4648 §4), **no padding** [REQ-MSG-102]
- Line breaks MUST NOT be included [REQ-MSG-103]
- Implementations MUST reject envelopes containing padding characters (`=`) or invalid Base64 characters [REQ-MSG-104]

**HTTP Response Codes (Normative):**
- `200 OK`: Success (body as shown)
- `400 Bad Request`: Invalid cursor format
- `401 Unauthorized`: Invalid or expired token

#### 7.4.3 Delete Messages

```
DELETE /messages
Authorization: Bearer <token>
Content-Type: application/json

{
  "message_ids": ["<uuid>", "<uuid>"]
}
```

**HTTP Response Codes (Normative):**
- `200 OK`: Deletion processed (JSON body: `{"deleted": <count>}`)
- `400 Bad Request`: Malformed request body
- `401 Unauthorized`: Invalid or expired token

**Error Response Schema (Normative):**
```json
{"error": "<code>", "message": "<human-readable>"}
```

**Delete Authorization (Normative):**

Mailbox servers MUST only delete messages stored in the authenticated identity's inbox: [REQ-MSG-105]

1. Decode `token.iid` from Base32 to raw 20 bytes - this is the **inbox owner**
2. For each `message_id`, verify the message was stored under this `inbox_owner_iid`
3. If any `message_id` refers to a message not in `token.iid`'s inbox, skip that message (do not delete, do not error)
4. Return success if all owned messages were deleted; silently ignore non-owned or non-existent IDs

**Note:** The PUSE envelope's `recipient` field is NOT used for delete authorization. Authorization is based on which inbox the message was stored in (the `inbox_owner_iid` from the original store request).

### 7.5 Mailbox Retention

- Default retention: 30 days
- Maximum envelope size: 1 MB
- Rate limits: 1000 messages/hour per sender, 10000 messages/day per recipient

### 7.6 Trust Model

| Property | Guarantee |
|----------|-----------|
| Confidentiality | Mailbox sees only encrypted envelopes |
| Integrity | PUSE signature protects content |
| Metadata | Mailbox sees: sender IID, recipient IID, size, timestamp |
| Availability | Mailbox may drop messages or go offline |

---

## 8. Message Types

### 8.1 Plaintext Format

The encrypted payload is UTF-8 JSON:

```json
{
  "type": "<message-type>",
  "timestamp": "<RFC3339>",
  "sequence": "<uint64-string>",
  "thread_id": "<optional-uuid>",
  "reply_to": "<optional-message-id>",
  "expires_at": "<optional-RFC3339>",
  "content": { ... }
}
```

**Sequence Number Constraints (Normative):**
- `sequence` MUST be a decimal string in range [0, 2^64-1] [REQ-MSG-106]
- MUST be canonical: no leading zeros except for the value `"0"` itself [REQ-MSG-107]
- MUST NOT be encoded as a JSON number (to preserve precision for values > 2^53) [REQ-MSG-108]
- Valid examples: `"0"`, `"1"`, `"42"`, `"18446744073709551615"`
- Invalid examples: `"01"`, `"+1"`, `" 1"`, `1`, `0x10`
- Comparison MUST be numeric (not lexicographic) [REQ-MSG-109]
- Receivers MUST reject messages with non-canonical or out-of-range sequence values [REQ-MSG-110]

This matches RFC-0001 §6.4 sequence number constraints for consistency across the protocol.

**Note:** `message_id` is in the envelope header, not the plaintext.

### 8.2 Type Registry

| Type | Description | Content Schema (wire: snake_case) |
|------|-------------|-----------------------------------|
| `text` | Plain text message | `{ "text": string, "mentions": Mention[] }` |
| `rich` | Rich text | `{ "format": "markdown"\|"html", "text": string, "mentions": Mention[] }` |
| `media` | Media reference | `{ "media_type": string, "url": string, "key": string, ... }` |
| `reaction` | Emoji reaction | `{ "target_message_id": uuid, "emoji": string, "action": "add"\|"remove" }` |
| `receipt` | Delivery/read receipt | `{ "receipt_type": "delivered"\|"read", "message_ids": uuid[] }` |
| `typing` | Typing indicator | `{ "is_typing": boolean }` |
| `key_update` | Ratchet key update | `{ "new_ephemeral_public": string }` |
| `sender_key_share` | Group key distribution | See §6.4 |
| `group_state_update` | Group membership change | See §8.6 below |
| `sync_op` | Sync operation (mailbox fallback) | See note below |
| `app` | Application-specific | `{ "app_id": string, "data": any }` |

**Wire format note:** On-wire JSON uses snake_case field names; TypeScript interfaces use camelCase. See `spec/03-messaging-sync/interfaces.md` for the wire/TS mapping convention.

**Note on `sync_op`:** Sync operations normally flow over the dedicated sync stream (stream type 0x04), which is NOT PUSE-wrapped. However, when sync data must be delivered via mailbox (e.g., recipient offline for extended periods), it MAY be encapsulated in PUSE with type `sync_op`. Recipients MUST validate both the PUSE envelope signature AND the sync operation's internal `signature` field. [REQ-MSG-111]

**`sync_op` Plaintext Schema (Normative):**

When a sync operation is encapsulated in PUSE for mailbox delivery, the plaintext MUST use this schema: [REQ-MSG-112]

```json
{
  "type": "sync_op",
  "timestamp": "<RFC3339>",
  "sequence": "<uint64-string>",
  "content": {
    "sync_type": <uint8>,
    "cbor": "<base64-std-no-pad>"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `sync_type` | uint8 | Sync message type code: 0x01=SYNC_REQUEST, 0x02=SYNC_OFFER, 0x03=SYNC_ACCEPT, 0x04=SYNC_OPERATIONS, 0x05=SYNC_ACK, 0x06=SYNC_SUBSCRIBE, 0x07=SYNC_UNSUBSCRIBE; see `sync-protocol.md` §3 Message Types |
| `cbor` | string | Base64 standard encoding (no padding) of the complete CBOR-encoded sync operation bytes. `content.cbor` MUST contain **only** the CBOR Data portion (the bytes that follow the 1-byte sync Message Type on stream 0x04). It MUST NOT include the 1-byte `sync_type` prefix.  [REQ-MSG-113]|

**Processing rules:**
1. Decode `content.cbor` from Base64 to raw bytes
2. Parse the CBOR structure per `sync-protocol.md` schema for the given `sync_type`
3. Verify the internal `signature` field within the CBOR structure
4. Apply the sync operation to local state

**Example:**
```json
{
  "type": "sync_op",
  "timestamp": "2026-01-14T12:00:00Z",
  "sequence": "100",
  "content": {
    "sync_type": 1,
    "cbor": "pWR0eXBlAWd2ZXJzaW9ueDIwMjYtMDEtMTRUMTI6MDA6MDBaLjEuYjFhbmFzcjVoZGVwc4BnY2hhbmdlc4GjYm9wY3NldGRwYXRoai9wcm9maWxlL3hldmFsdWVkVGVzdGlzaWduYXR1cmV4WC4uLg"
  }
}
```

**Host API to PUSE Plaintext Mapping (Normative):**

The Host API `messaging.send` (see `spec/04-app-runtime/api-surface.md`) uses:
- `message_type: string` - Application-defined type identifier
- `content: Uint8Array` - CBOR-encoded content

The host runtime MUST map these to PUSE plaintext as follows: [REQ-MSG-114]

1. **System types** (`text`, `rich`, `media`, `reaction`, `receipt`, `typing`):
   - The host runtime generates the PUSE plaintext directly
   - Apps should use dedicated Host API methods (if available) or the generic API
   - `message_type` in Host API corresponds to PUSE `type` field

2. **App-defined types** (any `message_type` not in the system registry):
   - PUSE `type` field MUST be `"app"` [REQ-MSG-115]
   - PUSE `content` MUST be: `{ "app_id": "<message_type>", "data": <CBOR-decoded-content> }` [REQ-MSG-116]
   - The Host API `content: Uint8Array` (CBOR) MUST be decoded to JSON for the `data` field [REQ-MSG-117]

**Example mapping:**

```typescript
// Host API call
messaging.send({
  recipient: "abc123...",
  message_type: "com.example.chess.move",
  content: cbor_encode({ from: "e2", to: "e4" })
})

// Results in PUSE plaintext:
{
  "type": "app",
  "timestamp": "2026-01-14T12:00:00Z",
  "sequence": "42",
  "content": {
    "app_id": "com.example.chess.move",
    "data": { "from": "e2", "to": "e4" }
  }
}
```

**Message subscription dispatch:**
- Recipients subscribe via `messaging.subscribe` with `message_types` filter
- For app messages, the filter matches against the `app_id` within `content`, NOT the literal `"app"` type
- Pattern matching applies (e.g., `"com.example.*"` matches `"com.example.chess.move"`)

**CBOR↔JSON Conversion (Normative):**

The Host runtime MUST decode CBOR to JSON-compatible values. This mapping is deterministic and reversible for supported types. [REQ-MSG-118]

| CBOR Type | JSON Representation | Notes |
|-----------|---------------------|-------|
| unsigned int (0..2^53-1) | number | Direct mapping |
| unsigned int (≥2^53) | `"~i<decimal>"` | String prefix preserves precision |
| negative int (-2^53..−1) | number | Direct mapping |
| negative int (≤−2^53) | `"~i<decimal>"` | String prefix preserves precision |
| byte string | `"~b<base64-std-no-pad>"` | e.g., `"~bYWJj"` for `[0x61, 0x62, 0x63]` |
| text string | string | Direct mapping (UTF-8) |
| array | array | Recursive conversion |
| map (string keys) | object | Recursive conversion |
| map (non-string keys) | `"~m<base64-cbor>"` | Entire map re-encoded as CBOR, then Base64 |
| tag | `{ "~t": <uint>, "v": <value> }` | Recursive conversion of tagged value |
| false/true | boolean | Direct mapping |
| null | null | Direct mapping |
| undefined | `"~u"` | Literal string |
| float (finite) | number | Direct mapping |
| float (NaN) | `"~fNaN"` | Literal string |
| float (+Infinity) | `"~f+Inf"` | Literal string |
| float (-Infinity) | `"~f-Inf"` | Literal string |
| simple value (other) | `"~s<uint>"` | e.g., `"~s255"` for simple(255) |

**Processing rules:**
1. **Prefixes are reserved:** Any JSON string starting with `~` followed by a lowercase letter is reserved for CBOR encoding. Apps MUST NOT use such strings as literal values. [REQ-MSG-119]
2. **Reversibility:** Recipients can reverse the mapping to reconstruct original CBOR types using the prefix as a discriminator.
3. **Precision:** Integer values ≥2^53 or ≤−2^53 MUST use the `~i` prefix to avoid JavaScript/JSON precision loss. [REQ-MSG-120]

**Example conversions:**
```
CBOR: h'deadbeef' → JSON: "~b3q2+7w"
CBOR: 9007199254740993 → JSON: "~i9007199254740993"
CBOR: {1: "one", 2: "two"} → JSON: "~m..." (CBOR-encoded map as Base64)
CBOR: tag(1, 1234567890) → JSON: {"~t": 1, "v": 1234567890}
```

### 8.3 Text Message

```json
{
  "type": "text",
  "timestamp": "2026-01-14T12:00:00Z",
  "sequence": "42",
  "content": {
    "text": "Hello @alice!",
    "mentions": [
      {
        "iid": "b1anasr5h0bj3832xqexwy0f0987e1xb",
        "offset": 6,
        "length": 6
      }
    ]
  }
}
```

### 8.4 Media Message

```json
{
  "type": "media",
  "timestamp": "2026-01-14T12:00:00Z",
  "sequence": "43",
  "content": {
    "media_type": "image/jpeg",
    "size": 1234567,
    "width": 1920,
    "height": 1080,
    "hash": "sha256:abcd1234...",
    "key": "<32-bytes-base64>",
    "nonce": "<12-bytes-base64>",
    "url": "https://cdn.example.com/encrypted/abc123"
  }
}
```

Media files are encrypted separately using the provided key/nonce, then uploaded to a content host. The URL points to the encrypted blob.

### 8.5 Receipt

```json
{
  "type": "receipt",
  "timestamp": "2026-01-14T12:01:00Z",
  "sequence": "44",
  "content": {
    "receipt_type": "read",
    "message_ids": [
      "550e8400-e29b-41d4-a716-446655440000",
      "550e8400-e29b-41d4-a716-446655440001"
    ]
  }
}
```

Receipt types: `delivered`, `read`

---

### 8.6 Group State Update

Group membership changes are signaled via `group_state_update` messages:

```json
{
  "type": "group_state_update",
  "timestamp": "2025-01-15T12:00:00Z",
  "sequence": "100",
  "content": {
    "action": "add_member",
    "group_id": "<32-char-base32>",
    "target_iid": "<32-char-base32>",
    "version": "5.b1anasr5"
  }
}
```

**Content Schema (wire format):**

| Field | Type | Description |
|-------|------|-------------|
| `action` | string | One of: `add_member`, `remove_member`, `promote_admin`, `demote_admin`, `update_info`, `rotate_sender_key` |
| `group_id` | string | 32-char Crockford Base32 group identifier (20 raw bytes encoded) |
| `target_iid` | string | (For member actions) IID of affected member |
| `version` | string | Format: `"<logical_clock>.<actor_suffix>"` where actor_suffix is first 8 chars of actor's IID |

**Authentication:** Group state updates are authenticated via the **PUSE envelope signature**. There is no separate content-level signature field. The sender's identity (from PUSE) is verified against the group's admin list.

**Version Ordering and Conflict Resolution (Normative):**

Group state changes MUST be ordered deterministically to ensure all members converge to the same state. The total ordering is: [REQ-MSG-121]

1. **Parse version:** Extract `logical_clock` (integer) and `actor_suffix` (string) from `"<logical_clock>.<actor_suffix>"`
2. **Compare `logical_clock`:** As unsigned integers (NOT lexicographically). Higher clock = later version.
3. **If same `logical_clock`:** Compare `actor_suffix` lexicographically (case-sensitive, bytewise). Smaller suffix = earlier version.
4. **If same `version` string** (possible from retries/duplicates): Compare full `actor_iid` (from PUSE sender) lexicographically. Smaller IID = earlier version.
5. **If same `actor_iid`:** Compare `action` type lexicographically. Smaller action string = earlier version.

**Note:** This provides a total ordering even when `actor_suffix` (8 chars) collides, which is possible since it's truncated from the 32-char IID.

**Example:** Version `"10.a1b2c3d4"` > `"2.z9y8x7w6"` because `10 > 2` numerically (not string comparison).

**Authorization:** Authorized roles may send `group_state_update` messages per the action table:

| Action | Required Role |
|--------|---------------|
| `add_member` | owner, admin, or moderator |
| `remove_member` | owner or admin (or self-removal) |
| `promote_admin` | owner only |
| `demote_admin` | owner only |
| `update_info` | owner or admin |
| `rotate_sender_key` | owner or admin |

Recipients MUST verify the PUSE sender has the required role before applying the update. [REQ-MSG-122]

See `spec/03-messaging-sync/group-messaging.md` for detailed action semantics.

---

### 8.7 Group ID Binding Validation (Normative)

For all group-addressed messages (where PUSE header `flags.recipient_type = 0x01`):

1. Extract `recipient_raw` (20 bytes) from PUSE header bytes 26-45
2. Compute `header_group_id = CrockfordBase32Lower(recipient_raw)`
3. If the decrypted plaintext contains a `content.group_id` field:
   - Receivers MUST verify `content.group_id == header_group_id` [REQ-MSG-123]
   - If mismatch: reject with error `INVALID_GROUP_ID_MISMATCH`

This applies to message types: `group_state_update`, `sender_key_share`, `group_invite`, and any future group-scoped types.

**Rationale:** Prevents routing confusion from buggy senders or corrupted envelopes.

---

## 9. Error Handling

### 9.1 Envelope Errors

| Error | Code | Condition | Recovery |
|-------|------|-----------|----------|
| `INVALID_MAGIC` | 0x301 | Magic bytes != "PUSE" | Reject envelope |
| `UNSUPPORTED_VERSION` | 0x302 | Version not recognized | Reject envelope |
| `INVALID_SIGNATURE` | 0x303 | Signature verification failed | Reject envelope |
| `DECRYPTION_FAILED` | 0x304 | AEAD decryption failed | Reject envelope |
| `ENVELOPE_TOO_LARGE` | 0x305 | Exceeds 1 MB | Reject envelope |
| `INVALID_EXTENSION` | 0x306 | Unknown or malformed extension | Reject envelope |

### 9.2 Ratchet Errors

| Error | Code | Condition | Recovery |
|-------|------|-----------|----------|
| `SESSION_NOT_FOUND` | 0x310 | No ratchet state for peer | Request session reset |
| `TOO_MANY_SKIPPED` | 0x311 | Exceeded max_skip | Request session reset |
| `INVALID_CHAIN_INDEX` | 0x312 | Index regression or overflow | Reject message |
| `SESSION_CORRUPTED` | 0x313 | State inconsistency | Request session reset |

### 9.3 Group Errors

| Error | Code | Condition | Recovery |
|-------|------|-----------|----------|
| `UNKNOWN_SENDER_KEY` | 0x320 | Key ID not found | Request key from sender |
| `ITERATION_REPLAY` | 0x321 | Iteration <= last seen | Reject message |
| `NOT_A_MEMBER` | 0x322 | Sender not in group | Reject message |
| `INSUFFICIENT_ROLE` | 0x323 | Action requires higher role | Reject action |
| `INVALID_GROUP_ID_MISMATCH` | 0x324 | content.group_id != header recipient | Reject message |

### 9.4 Mailbox Errors

| Error | HTTP | Condition | Recovery |
|-------|------|-----------|----------|
| `UNAUTHORIZED` | 401 | Invalid or expired token | Re-authenticate |
| `RATE_LIMITED` | 429 | Exceeded rate limits | Back off and retry |
| `MAILBOX_FULL` | 507 | Recipient quota exceeded | Retry later |
| `MESSAGE_TOO_LARGE` | 413 | Envelope > 1 MB | Fragment or reduce |

### 9.5 Transport Integration

PUSE envelopes are delivered over QUIC connections using stream type 0x03 (Message).

**Stream Framing (see RFC-0002 and layer-integration.md):**

```
Stream Header (written once at stream start):
┌──────────────────────────────┐
│ Stream Type: 0x03            │ 1 byte
└──────────────────────────────┘

Each Message Frame (repeated):
┌──────────────────────────────┐
│ Length (big-endian)          │ 4 bytes
├──────────────────────────────┤
│ PUSE Envelope                │ <length> bytes
└──────────────────────────────┘
```

**Delivery Rules:**
1. Open a bidirectional stream with type 0x03
2. Write PUSE envelope as a single frame (4-byte length prefix + raw bytes)
3. Multiple envelopes MAY be sent on the same stream [REQ-MSG-124]
4. Stream closes gracefully after last envelope or on error

**Mailbox Delivery:**
For offline recipients, use HTTP-based mailbox (§7). The PUSE envelope is stored as an opaque binary blob via the mailbox REST API.

---

## 10. Security Considerations

### 10.1 Forward Secrecy

Forward secrecy is achieved through:
- **Ephemeral keys in 2DH**: Initial messages use one-time keys
- **DH ratchet**: Each direction change generates new key material
- **Chain ratchet**: Each message derives fresh message key

**Window:** Messages before the last DH ratchet step remain vulnerable if current keys are compromised.

### 10.2 Non-Repudiation

PUSE signatures provide **strong non-repudiation**. Recipients can prove to third parties that the sender authored a message. This is intentional for accountability.

Applications requiring deniability should:
- Use application-layer MAC-based authentication
- Not rely on PUSE signatures for message authentication

### 10.3 Replay Protection

Replay protection mechanisms:
1. **Message ID**: 16-byte UUID, tracked in 7-day cache
2. **Ratchet chain index**: Monotonically increasing per chain
3. **Sender key iteration**: Monotonically increasing per key

### 10.4 Metadata Exposure

Visible to network observers and mailboxes:
- Sender IID
- Recipient IID
- Message size
- Timing

**Not** visible:
- Message content
- Message type
- Attachments (after decryption)

### 10.5 Key Compromise Scenarios

| Scenario | Impact | Mitigation |
|----------|--------|------------|
| Identity signing key | Attacker can sign messages | Key rotation + revocation |
| Identity encryption key | Attacker can initiate sessions | Key rotation |
| Ratchet state | Attacker can decrypt future messages until ratchet | DH ratchet provides recovery |
| Sender key | Attacker can decrypt group messages until rotation | Regular rotation |

### 10.6 Denial of Service

| Attack | Mitigation |
|--------|------------|
| Large envelopes | 1 MB limit, reject before parsing |
| Many skipped keys | max_skip limit (100), session reset |
| Rapid key rotation | Rate limit key_update messages |
| Mailbox flooding | Per-sender rate limits |

---

## 11. Test Vectors

**Authoritative test vectors are defined in `spec/00-shared/test-vectors.md`.** The examples below illustrate wire format structure; implementers MUST validate against the shared test vectors. [REQ-MSG-125]

### 11.1 PUSE Envelope Wire Format

**Wire format structure (see §3.2 for byte-level specification):**
```
PUSE Header:
  magic (4 bytes):          "PUSE" (0x50555345)
  version (1 byte):         0x01
  flags (1 byte):           0x00 (no compression, single recipient)
  sender_iid (20 bytes):    Truncated SHA256 of sender's genesis key
  recipient_iid (20 bytes): Truncated SHA256 of recipient's genesis key
  message_id (16 bytes):    UUID v4 bytes
  extension_length (2 bytes): Big-endian uint16
  header_extension (variable): Ratchet header for Double Ratchet

Encrypted Payload:
  nonce (12 bytes):         ChaCha20-Poly1305 nonce
  ciphertext_length (4 bytes): Big-endian uint32
  ciphertext (variable):    AEAD-encrypted JSON payload

Trailer:
  signature (64 bytes):     Ed25519 signature over header + payload
```

**Note:** Implementers should construct test cases using keys from `spec/00-shared/test-vectors.md` (Alice/Bob identities) and validate encryption/decryption round-trips match.

### 11.2 Chain Step KDF

**Input:**
```
chain_key (32 bytes, hex): 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
```

**Expected output:**
```
message_key = HMAC-SHA256(chain_key, 0x01)
new_chain_key = HMAC-SHA256(chain_key, 0x02)
```

Implementers should compute these values using their HMAC-SHA256 implementation and verify against a reference.

### 11.3 2DH Initial Derivation

See `spec/00-shared/test-vectors.md` Test Vector 6 (X3DH Key Agreement) for authoritative pre-computed values.

**Process Overview:**
```
ikm = dh1 || dh2  (64 bytes, X25519 outputs)
salt = sorted(iid_a, iid_b) concatenated (40 bytes)
prk = HKDF-Extract(salt, ikm)
derived = HKDF-Expand(prk, "post-urbit-x3dh-v1", 64)
root_key = derived[0:32]
initial_chain_key = derived[32:64]
```

Implementers MUST validate against the pre-computed test vectors to ensure correct implementation. [REQ-MSG-126]

---

## 12. Implementation Notes

### 12.1 Recommended Libraries

| Language | Cryptography | Notes |
|----------|--------------|-------|
| Rust | ring, sodiumoxide | ring for performance |
| Go | x/crypto | Standard library |
| JavaScript | @noble/ed25519, @noble/hashes | Pure JS, audited |
| Python | pynacl, cryptography | pynacl wraps libsodium |

### 12.2 Performance Targets

| Operation | Target |
|-----------|--------|
| PUSE encrypt (1 KB) | < 1 ms |
| PUSE decrypt (1 KB) | < 1 ms |
| 2DH key exchange | < 5 ms |
| Ratchet step | < 0.5 ms |

### 12.3 Memory Safety

1. **Zero sensitive data**: Securely wipe keys, plaintext after use
2. **Constant-time operations**: Use constant-time comparison for signatures
3. **Limit allocations**: Reject oversized envelopes before full parsing

### 12.4 State Persistence

Ratchet state MUST be persisted atomically: [REQ-MSG-127]
- Use transactional storage (SQLite, etc.)
- Never persist partial state updates
- Encrypt at rest with device key

---

## 13. References

### 13.1 Normative References

- RFC 2119: Key words for use in RFCs
- RFC 5869: HKDF (HMAC-based Key Derivation Function)
- RFC 8032: Ed25519 Digital Signatures
- RFC 8439: ChaCha20 and Poly1305
- RFC-0001: Post-Urbit Identity Document
- RFC-0002: Post-Urbit Transport Protocol
- [Signal Protocol Specification](https://signal.org/docs/): Double Ratchet (used as-is), X3DH (simplified to 2DH)

### 13.2 Informative References

- [The Double Ratchet Algorithm](https://signal.org/docs/specifications/doubleratchet/)
- [The X3DH Key Agreement Protocol](https://signal.org/docs/specifications/x3dh/)
- [libsodium](https://libsodium.org/): Cryptographic library

---

## Appendix A: Domain Separator Registry

All domain separators used in this RFC:

| Context | Domain Separator | Length |
|---------|------------------|--------|
| Root chain KDF | `post-urbit-ratchet-v1` | 21 bytes |
| 2DH initial | `post-urbit-x3dh-v1` | 18 bytes |
| Sender key KDF | `post-urbit-sender-key-v1:` | 25 bytes + binding |
| Mailbox token | `post-urbit-mailbox-token-v1` | 27 bytes |

---

## Appendix B: Wire Format Summary

### PUSE Envelope

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | Magic ("PUSE") |
| 4 | 1 | Version |
| 5 | 1 | Flags |
| 6 | 20 | Sender IID (raw) |
| 26 | 20 | Recipient IID (raw) |
| 46 | 16 | Message ID |
| 62 | 2 | Header Extension Length |
| 64 | var | Header Extension |
| 64+ext | 12 | Nonce |
| 76+ext | 4 | Ciphertext Length |
| 80+ext | var | Ciphertext |
| end-64 | 64 | Signature |

### Header Extensions

**Initial (0x00):** 1 + 32 = 33 bytes
**Ratchet (0x01):** 1 + 32 + 4 + 4 = 41 bytes
**Group (0x02):** 1 + 16 + 4 = 21 bytes

---

*End of RFC-0003*
