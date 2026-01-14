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

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

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

**Size constraints:**
- Minimum envelope size: 160 bytes (empty extension, 16-byte ciphertext)
- Maximum envelope size: 1,048,576 bytes (1 MB)
- Maximum header extension: 1024 bytes
- Maximum ciphertext: 1,048,352 bytes

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

### 3.4 Header Extensions

Header extensions carry cryptographic parameters needed before decryption.

**Framing Rule:** Exactly ONE header extension MUST be present per envelope. The `Header Extension Length` field MUST equal the fixed size for the given extension type. Implementations MUST reject envelopes where `ext_len == 0` or where `ext_len` does not match the expected size for the extension type.

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

### 3.5 Nonce Generation

The 12-byte nonce MUST be constructed as:

```
Nonce:
┌────────────────────────────────────────┐
│ Timestamp (seconds since epoch, BE)    │ 4 bytes
├────────────────────────────────────────┤
│ Random                                 │ 8 bytes
└────────────────────────────────────────┘
```

**Requirements:**
- Timestamp SHOULD be current time (for replay correlation)
- Receivers MUST NOT reject messages based on timestamp age (messages may be delivered via mailbox days later)
- Receivers MAY reject messages with timestamps more than 24 hours in the future
- Random bytes MUST come from a CSPRNG
- (key, nonce) pairs MUST never be reused

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

The 16-byte Poly1305 tag is included in the ciphertext.

### 3.7 Signature

The signature covers all bytes of the envelope BEFORE the signature field:

```
signed_data = envelope[0 : total_length - 64]

signature = Ed25519_Sign(sender_signing_key, signed_data)
```

**Note:** The signature is NOT over a hash - it signs the raw envelope bytes directly. Ed25519 internally hashes the message.

### 3.8 Parsing Order

Receivers MUST parse in this order for streaming support:

1. Read fixed header (64 bytes): magic through header_extension_length
2. Read header_extension (length from step 1)
3. Read nonce (12 bytes)
4. Read ciphertext_length (4 bytes)
5. Read ciphertext (length from step 4)
6. Read signature (64 bytes)
7. Verify signature over bytes from steps 1-5
8. Verify sender identity (fetch identity document for sender IID)
9. Derive message key based on extension type
10. Decrypt ciphertext

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
    """
    # Track previous chain length for PN
    previous_chain_length = 0

    # If no sending chain, perform DH ratchet step
    if state.sending_chain_key is None:
        # Record previous chain length before ratcheting
        previous_chain_length = state.sending_chain_index

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
    # PN = messages sent in previous chain; N = current message number (0-indexed)
    header = RatchetHeader(
        dh_public=state.dh_sending_key.public,
        pn=previous_chain_length,
        n=n
    )

    # Encrypt (nonce generated per §3.5)
    nonce = generate_nonce()
    ciphertext = chacha20_poly1305_encrypt(
        key=message_key,
        nonce=nonce,
        aad=header.encode(),
        plaintext=plaintext
    )

    # Securely delete message key
    secure_zero(message_key)

    return header, nonce, ciphertext
```

**Message Key Derivation:** The message key for message number N is derived by applying `kdf_chain_step` once to the chain key. After derivation, the chain key advances to enable the next message. This means message N=0 uses the first `kdf_chain_step` output from the initial chain key.

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
    # Check for skipped message key
    skip_key = f"{header.dh_public.hex()}:{header.n}"
    if skip_key in state.skipped_keys:
        entry = state.skipped_keys.pop(skip_key)
        plaintext = chacha20_poly1305_decrypt(
            key=entry.message_key,
            nonce=nonce,
            aad=header.encode(),
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

    # Decrypt
    plaintext = chacha20_poly1305_decrypt(
        key=message_key,
        nonce=nonce,
        aad=header.encode(),
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

The initiator generates:
- **Ephemeral Key (EK):** One-time X25519 key pair

### 5.3 Protocol

**Alice wants to message Bob (first time):**

1. Alice retrieves Bob's identity document
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
2. Bob extracts Alice's identity key: `IK_A = alice.keys.encryption.current`
3. Bob extracts Alice's ephemeral key from header extension: `EK_A_public`
4. Bob computes same DH outputs:
   ```
   DH1 = X25519(IK_B_private, IK_A)    // Same as Alice's DH1
   DH2 = X25519(IK_B_private, EK_A)    // Bob identity × Alice ephemeral
   ```
5. Bob derives same initial keys
6. Bob initializes ratchet state
7. Bob decrypts message

### 5.4 Security Properties

| Property | Guarantee |
|----------|-----------|
| Forward secrecy | Yes (via ephemeral key) |
| Key confirmation | No (Alice doesn't know Bob received) |
| Replay protection | Via message ID and nonce |
| Identity binding | DH1 binds to both identity keys |

### 5.5 Initial Ratchet State Setup (Normative)

After the 2DH exchange completes, both parties MUST initialize their ratchet state as follows:

**Alice (initiator):**
```python
def initialize_ratchet_initiator(
    root_key: bytes,           # 32 bytes from kdf_initial
    initial_chain_key: bytes,  # 32 bytes from kdf_initial
    bob_identity_key: bytes,   # 32 bytes, Bob's IK public
    bob_iid: bytes             # 20 bytes, Bob's IID
) -> RatchetState:
    state = RatchetState()
    state.peerIid = bob_iid

    # Alice sets Bob's identity key as receiving key
    state.dhReceivingKey = bob_identity_key

    # Alice has NO sending key yet (will generate on first send)
    state.dhSendingKey = None

    # Root key from 2DH
    state.rootKey = root_key

    # Alice uses initial_chain_key as her SENDING chain
    # (she sends first, Bob receives)
    state.sendingChainKey = initial_chain_key
    state.sendingChainIndex = 0

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

**Domain separator:** `post-urbit-sender-key-v1:` (24 ASCII bytes) + binding data

### 6.4 Key Distribution

Sender keys are distributed via 1:1 ratchet messages:

```json
{
  "type": "sender_key_share",
  "content": {
    "group_id": "<20-bytes-base64>",
    "sender_iid": "<20-bytes-base64>",
    "key_id": "<16-bytes-base64>",
    "chain_key": "<32-bytes-base64>",
    "iteration": 0
  }
}
```

**Distribution triggers:**
- Group creation: Creator shares with all initial members
- New member: All existing members share their keys
- Key rotation: Member shares new key with all others

### 6.5 Key Rotation

Sender keys SHOULD rotate:
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
    nonce = generate_nonce()
    ciphertext = chacha20_poly1305_encrypt(
        key=message_key,
        nonce=nonce,
        aad=header.encode(),
        plaintext=plaintext
    )

    secure_zero(message_key)
    return header, nonce, ciphertext
```

### 6.7 Group Decryption Flow

```python
def group_decrypt(sender_iid: bytes, group_id: bytes, header: GroupHeader,
                  nonce: bytes, ciphertext: bytes,
                  sender_keys: dict) -> bytes:
    """
    Decrypt a group message.
    """
    # Look up sender key
    key_lookup = f"{sender_iid.hex()}:{header.sender_key_id.hex()}"
    if key_lookup not in sender_keys:
        raise UnknownSenderKeyError()

    sender_key = sender_keys[key_lookup]

    # Verify iteration (prevent replay)
    if header.iteration <= sender_key.last_seen_iteration:
        raise ReplayDetectedError()

    # Advance chain to match iteration
    while sender_key.local_iteration < header.iteration:
        sender_key.chain_key, _ = kdf_sender_key(
            sender_key.chain_key,
            group_id,
            sender_iid,
            sender_key.key_id
        )
        sender_key.local_iteration += 1

    # Derive message key
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
        aad=header.encode(),
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

Each identity MAY publish mailbox endpoints in their identity document:

```json
{
  "endpoints": [
    {
      "type": "mailbox",
      "url": "https://mailbox.example.com",
      "priority": 1
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

**Signature construction:**

**Domain separator:** `post-urbit-mailbox-token-v1` (27 ASCII bytes)

```python
def create_mailbox_token(iid: str, mailbox_url: str, signing_key: bytes) -> str:
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

### 7.4 Mailbox API

#### 7.4.1 Store Message

```
POST /messages
Authorization: Bearer <token>
Content-Type: application/octet-stream

<PUSE envelope bytes>
```

Response:
```json
{
  "message_id": "<uuid>",
  "stored_at": "<RFC3339>",
  "expires_at": "<RFC3339>"
}
```

#### 7.4.2 Retrieve Messages

```
GET /messages?since=<timestamp>&limit=100
Authorization: Bearer <token>
```

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
  "has_more": false
}
```

#### 7.4.3 Delete Messages

```
DELETE /messages
Authorization: Bearer <token>
Content-Type: application/json

{
  "message_ids": ["<uuid>", "<uuid>"]
}
```

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

**Note:** `message_id` is in the envelope header, not the plaintext.

### 8.2 Type Registry

| Type | Description | Content Schema |
|------|-------------|----------------|
| `text` | Plain text message | `{ "text": string, "mentions": Mention[] }` |
| `rich` | Rich text (markdown) | `{ "markdown": string, "mentions": Mention[] }` |
| `media` | Media reference | `{ "media_type": string, "url": string, "key": string, ... }` |
| `reaction` | Emoji reaction | `{ "emoji": string, "message_id": string }` |
| `receipt` | Delivery/read receipt | `{ "receipt_type": string, "message_ids": string[] }` |
| `typing` | Typing indicator | `{ "typing": boolean }` |
| `key_update` | Ratchet key update | `{ "new_ephemeral_public": string }` |
| `sender_key_share` | Group key distribution | See §6.4 |
| `group_event` | Group membership change | `{ "event": string, ... }` |
| `app` | Application-specific | `{ "app_id": string, "data": any }` |

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

### 9.4 Mailbox Errors

| Error | HTTP | Condition | Recovery |
|-------|------|-----------|----------|
| `UNAUTHORIZED` | 401 | Invalid or expired token | Re-authenticate |
| `RATE_LIMITED` | 429 | Exceeded rate limits | Back off and retry |
| `MAILBOX_FULL` | 507 | Recipient quota exceeded | Retry later |
| `MESSAGE_TOO_LARGE` | 413 | Envelope > 1 MB | Fragment or reduce |

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

### 11.1 PUSE Envelope Construction

**Inputs:**
```
sender_iid_raw (20 bytes, hex): 586a763f2c82b31a0c5de9dcaef01e0261e0785b
recipient_iid_raw (20 bytes, hex): a1b2c3d4e5f60718293a4b5c6d7e8f9001020304
message_id (16 bytes, hex): 550e8400e29b41d4a716446655440000
message_key (32 bytes, hex): 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
nonce (12 bytes, hex): 00000000aabbccdd11223344
plaintext: {"type":"text","content":{"text":"Hello"}}
signing_key_seed (32 bytes, hex): 0000000000000000000000000000000000000000000000000000000000000000
```

**Ratchet Header Extension (type 0x01):**
```
dh_public (32 bytes, hex): 0000000000000000000000000000000000000000000000000000000000000001
pn (4 bytes, hex): 00000000
n (4 bytes, hex): 00000001
```

**Header Extension (41 bytes, hex):**
```
01                                                              // type
0000000000000000000000000000000000000000000000000000000000000001  // dh_public
00000000                                                        // pn
00000001                                                        // n
```

**Expected envelope (before signature):**
```
5055534501 00                               // magic + version + flags
586a763f2c82b31a0c5de9dcaef01e0261e0785b    // sender_iid
a1b2c3d4e5f60718293a4b5c6d7e8f9001020304    // recipient_iid
550e8400e29b41d4a716446655440000            // message_id
0029                                        // extension_length (41)
01...01 00000000 00000001                   // header_extension
00000000aabbccdd11223344                    // nonce
<ciphertext_length>                         // 4 bytes
<ciphertext>                                // encrypted payload
```

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

**Inputs:**
```
dh1 (32 bytes, hex): [X25519 output 1]
dh2 (32 bytes, hex): [X25519 output 2]
iid_a (20 bytes, hex): 586a763f2c82b31a0c5de9dcaef01e0261e0785b
iid_b (20 bytes, hex): a1b2c3d4e5f60718293a4b5c6d7e8f9001020304
```

**Process:**
```
ikm = dh1 || dh2  (64 bytes)
salt = sorted(iid_a, iid_b) concatenated (40 bytes)
prk = HKDF-Extract(salt, ikm)
derived = HKDF-Expand(prk, "post-urbit-x3dh-v1", 64)
root_key = derived[0:32]
initial_chain_key = derived[32:64]
```

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

Ratchet state MUST be persisted atomically:
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
| Sender key KDF | `post-urbit-sender-key-v1:` | 24 bytes + binding |
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
