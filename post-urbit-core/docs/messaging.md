# Post-Urbit Messaging Protocol

This document provides comprehensive developer documentation for the Post-Urbit Secure Envelope (PUSE) messaging protocol, including envelope format, encryption, and practical implementation guidance.

## Table of Contents

1. [Overview](#overview)
2. [PUSE Envelope Format](#puse-envelope-format)
3. [Encryption System](#encryption-system)
4. [Message Types](#message-types)
5. [Envelope Lifecycle](#envelope-lifecycle)
6. [Size Limits and Chunking](#size-limits-and-chunking)
7. [Error Handling](#error-handling)
8. [Code Examples](#code-examples)
9. [Reference](#reference)

---

## Overview

Post-Urbit uses the **PUSE (Post-Urbit Secure Envelope)** format for all encrypted messaging. The protocol provides:

- **End-to-end encryption** using ChaCha20-Poly1305
- **Forward secrecy** via the Double Ratchet algorithm
- **Authentication** through Ed25519 signatures
- **Support for 1:1 and group messaging**

```
┌────────────────────────────────────────────────────────────────┐
│                      PUSE Envelope                              │
├────────────────────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                    Header (plaintext)                     │  │
│  │  Magic │ Version │ Flags │ Sender │ Recipient │ Msg ID   │  │
│  │  Extension Length │ Header Extension │ Nonce │ CT Length │  │
│  └──────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                 Ciphertext (encrypted)                    │  │
│  │                   [ message payload ]                     │  │
│  └──────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                 Signature (64 bytes)                      │  │
│  │                 Ed25519 over header+ciphertext            │  │
│  └──────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

---

## PUSE Envelope Format

### Header Structure

The PUSE header is defined in `src/messaging.rs` (lines 12-21):

```rust
pub struct PUSEHeader {
    pub flags: u8,              // Message type and options
    pub sender_iid: [u8; 20],   // Sender's Identity ID
    pub recipient_iid: [u8; 20], // Recipient's Identity ID
    pub message_id: [u8; 16],   // Unique message identifier
    pub header_extension: Vec<u8>, // Encryption metadata
    pub nonce: [u8; 12],        // ChaCha20 nonce
    pub ciphertext_length: u32, // Length of encrypted payload
}
```

### Binary Layout

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | Magic | `PUSE` (0x50555345) |
| 4 | 1 | Version | Protocol version (currently 1) |
| 5 | 1 | Flags | Message type and options |
| 6 | 20 | Sender IID | 20-byte identity hash |
| 26 | 20 | Recipient IID | 20-byte identity hash |
| 46 | 16 | Message ID | UUID for deduplication |
| 62 | 2 | Extension Length | Big-endian u16 |
| 64 | N | Header Extension | Encryption parameters |
| 64+N | 12 | Nonce | ChaCha20 nonce |
| 76+N | 4 | Ciphertext Length | Big-endian u32 |
| 80+N | M | Ciphertext | Encrypted payload |
| 80+N+M | 64 | Signature | Ed25519 signature |

### Flags Byte

The flags byte (offset 5) contains message metadata:

```
Bit 7 6 5 4 3 2 1 0
    ─────── ─── ───
       │     │   └── Recipient Type (2 bits)
       │     └────── Reserved (2 bits)
       └──────────── Reserved (4 bits, MUST be 0)
```

**Recipient Types** (bits 0-1):
- `0x00` - 1:1 direct message
- `0x01` - Group message
- `0x02` - Reserved
- `0x03` - Reserved

Per `src/messaging.rs` (lines 150-153), reserved bits 5-7 MUST be zero:

```rust
// REQ-MSG-033/034: Validate flags byte - reserved bits (5-7) MUST be zero
if (flags & 0xE0) != 0 {
    return Err(PostUrbitError::InvalidInput("puse reserved flags not zero"));
}
```

### Header Extensions

Header extensions carry encryption-specific metadata. Three extension types are defined in `src/messaging.rs` (lines 30-35):

```rust
pub enum HeaderExtension {
    Initial { ephemeral: [u8; 32] },           // First message
    Ratchet { dh_public: [u8; 32], pn: u32, n: u32 }, // Ongoing
    Group { key_id: [u8; 16], iteration: u32 }, // Group messaging
}
```

#### Extension Type Codes

| Type | Code | Size | Description |
|------|------|------|-------------|
| Initial | `0x00` | 33 bytes | First message establishing session |
| Ratchet | `0x01` | 41 bytes | Ongoing 1:1 conversation |
| Group | `0x02` | 21 bytes | Group message |

#### Initial Extension (0x00)

Used for the first message in a conversation:

```
┌─────────┬─────────────────────────────────┐
│  0x00   │     Ephemeral Public Key        │
│ 1 byte  │          32 bytes               │
└─────────┴─────────────────────────────────┘
```

Built by `build_initial_extension()` (lines 37-42).

#### Ratchet Extension (0x01)

Used for subsequent messages with Double Ratchet state:

```
┌─────────┬─────────────────────────────────┬──────────┬──────────┐
│  0x01   │      DH Public Key              │    PN    │    N     │
│ 1 byte  │         32 bytes                │ 4 bytes  │ 4 bytes  │
└─────────┴─────────────────────────────────┴──────────┴──────────┘
```

- **PN** (Previous N): Message count from previous sending chain
- **N**: Current message number in sending chain

Built by `build_ratchet_extension()` (lines 44-51).

#### Group Extension (0x02)

Used for group messages with Sender Keys:

```
┌─────────┬────────────────────┬─────────────┐
│  0x02   │      Key ID        │  Iteration  │
│ 1 byte  │     16 bytes       │   4 bytes   │
└─────────┴────────────────────┴─────────────┘
```

- **Key ID**: Identifies the sender key being used
- **Iteration**: Must be > 0 (line 98-100)

Built by `build_group_extension()` (lines 53-62).

### Extension/Recipient Type Validation

The recipient type in flags MUST match the extension type (lines 182-208):

| Recipient Type | Allowed Extensions |
|----------------|-------------------|
| `0x00` (1:1) | Initial (0x00) or Ratchet (0x01) |
| `0x01` (Group) | Group (0x02) only |
| `0x02`, `0x03` | Reserved - error |

---

## Encryption System

### Algorithms Used

Post-Urbit uses a combination of cryptographic primitives:

| Purpose | Algorithm | Key Size |
|---------|-----------|----------|
| Symmetric Encryption | ChaCha20-Poly1305 | 256-bit |
| Key Agreement | X25519 | 256-bit |
| Signatures | Ed25519 | 256-bit |
| Key Derivation | HKDF-SHA256 | 256-bit |
| Chain Key Derivation | HMAC-SHA256 | 256-bit |

### Double Ratchet Protocol

The Double Ratchet provides forward secrecy and break-in recovery. Implementation is in `src/ratchet.rs`.

```
┌──────────────────────────────────────────────────────────────────┐
│                    Double Ratchet State                           │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│   ┌─────────────────────────────────────────────────────────┐    │
│   │                    Root Chain                            │    │
│   │  root_key ──[DH]──► new_root_key + sending_chain_key    │    │
│   └─────────────────────────────────────────────────────────┘    │
│                              │                                    │
│          ┌───────────────────┴───────────────────┐               │
│          ▼                                       ▼               │
│   ┌──────────────────┐                 ┌──────────────────┐      │
│   │  Sending Chain   │                 │ Receiving Chain  │      │
│   │  chain_key       │                 │ chain_key        │      │
│   │       │          │                 │       │          │      │
│   │  [HMAC 0x01]     │                 │  [HMAC 0x01]     │      │
│   │       ▼          │                 │       ▼          │      │
│   │  message_key     │                 │  message_key     │      │
│   │       │          │                 │                  │      │
│   │  [HMAC 0x02]     │                 │                  │      │
│   │       ▼          │                 │                  │      │
│   │  new_chain_key   │                 │                  │      │
│   └──────────────────┘                 └──────────────────┘      │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

#### Session Initialization (X3DH-style)

Initial key agreement uses a simplified 2-DH pattern (lines 95-121 in `ratchet.rs`):

**Initiator side:**
```rust
// DH1: Initiator's identity key with responder's identity key
let dh1 = identity_private.diffie_hellman(peer_identity_public);
// DH2: Initiator's ephemeral key with responder's identity key
let dh2 = ephemeral_private.diffie_hellman(peer_identity_public);
```

**Responder side:**
```rust
// DH1: Responder's identity key with initiator's identity key
let dh1 = identity_private.diffie_hellman(peer_identity_public);
// DH2: Responder's identity key with initiator's ephemeral key
let dh2 = identity_private.diffie_hellman(peer_ephemeral_public);
```

Initial key derivation (`kdf_initial`, lines 34-66):

```rust
pub fn kdf_initial(
    dh1: &[u8; 32],
    dh2: &[u8; 32],
    iid_a: &[u8; 20],
    iid_b: &[u8; 20],
) -> Result<([u8; 32], [u8; 32])>
```

- Concatenates DH1 and DH2 as input key material
- Uses sorted IIDs as salt (ensuring both parties derive same keys)
- Expands with HKDF using info `"post-urbit-x3dh-v1"`
- Returns `(root_key, chain_key)`

#### Chain Key Stepping

Each message derives a unique key from the chain (`kdf_chain_step`, lines 16-20):

```rust
pub fn kdf_chain_step(chain_key: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let message_key = hmac_sha256(chain_key, &[0x01]);
    let new_chain_key = hmac_sha256(chain_key, &[0x02]);
    (new_chain_key, message_key)
}
```

This ensures:
- Each message gets a unique encryption key
- Compromise of one message key doesn't reveal others
- Chain state can only move forward

#### DH Ratchet

When a new DH public key is received, the root chain ratchets (`kdf_root`, lines 22-32):

```rust
pub fn kdf_root(root_key: &[u8; 32], dh_output: &[u8]) -> Result<([u8; 32], [u8; 32])> {
    let hk = Hkdf::<Sha256>::new(Some(root_key), dh_output);
    // Expand to 64 bytes: new_root || chain_key
    hk.expand(RATCHET_INFO, &mut out)?;
    Ok((new_root, chain_key))
}
```

Info string: `"post-urbit-ratchet-v1"`

### Ratchet State

The full ratchet state (lines 141-153 in `ratchet.rs`):

```rust
pub struct RatchetState {
    pub peer_iid: [u8; 20],
    pub dh_sending_key: RatchetKeyPair,
    pub dh_receiving_key: PublicKey,
    pub root_key: [u8; 32],
    pub sending_chain_key: Option<[u8; 32]>,
    pub sending_chain_index: u32,
    pub previous_sending_chain_length: u32,
    pub receiving_chains: HashMap<[u8; 32], ReceivingChain>,
    skipped_keys: HashMap<SkippedKeyId, [u8; 32]>,
    pub max_skip: u32,  // Default: 100
}
```

### Out-of-Order Message Handling

The protocol handles messages arriving out of order by:

1. **Skipped key storage**: When message N+2 arrives before N+1, keys for skipped messages are derived and stored
2. **Maximum skip limit**: Prevents DoS via excessive key storage (default 100)
3. **Key lookup**: Before deriving new keys, check skipped_keys map

See `ratchet_decrypt()` (lines 293-347).

### Encryption/Decryption Functions

**Encryption** (`encrypt_puse_payload`, lines 253-269):

```rust
pub fn encrypt_puse_payload(
    message_key: &[u8; 32],
    header_extension: &[u8],  // Used as AAD
    nonce: &[u8; 12],
    plaintext: &[u8],
) -> Result<Vec<u8>>
```

- Uses ChaCha20-Poly1305 AEAD
- Header extension is authenticated but not encrypted (AAD)
- Returns ciphertext with authentication tag

**Decryption** (`decrypt_puse_payload`, lines 271-287):

```rust
pub fn decrypt_puse_payload(
    message_key: &[u8; 32],
    header_extension: &[u8],
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> Result<Vec<u8>>
```

---

## Message Types

### 1:1 Direct Messages

For private conversations between two identities:

1. **First message**: Uses Initial extension with ephemeral key
2. **Subsequent messages**: Uses Ratchet extension with DH ratchet state
3. **Flags**: Recipient type = `0x00`

### Group Messages

For multi-party conversations using the Sender Keys model:

1. **Sender Keys**: Each group member maintains their own sending chain
2. **Key Distribution**: Keys distributed via 1:1 messages to members
3. **Rotation**: Keys rotate after 100 messages or 7 days (see `src/group.rs`, lines 46-54)

```rust
pub fn should_rotate_sender_key(key: &SenderKey, now: DateTime<Utc>) -> Result<bool> {
    let too_many_messages = key.iteration >= 100;
    let too_old = now.signed_duration_since(created_at).num_days() >= 7;
    Ok(too_many_messages || too_old)
}
```

Group key derivation (`kdf_sender_key`, lines 68-93 in `ratchet.rs`):

```rust
pub fn kdf_sender_key(
    chain_key: &[u8; 32],
    group_id: &[u8; 20],
    sender_iid: &[u8; 20],
    key_id: &[u8; 16],
) -> ([u8; 32], [u8; 32])
```

Info format: `"post-urbit-sender-key-v1:{group_id}:{sender_iid}:{key_id}"`

### Group Roles and Permissions

Group membership is managed via roles (see `src/group.rs`, lines 87-109):

| Role | Add Member | Remove Member | Promote Admin | Update Info |
|------|------------|---------------|---------------|-------------|
| Owner | Yes | Yes | Yes | Yes |
| Admin | Yes | Yes | No | Yes |
| Moderator | Yes | Members only | No | No |
| Member | No | Self only | No | No |

---

## Envelope Lifecycle

### Creating and Sending

```
┌────────────┐     ┌────────────┐     ┌────────────┐     ┌────────────┐
│  Plaintext │────►│   Derive   │────►│  Encrypt   │────►│    Sign    │
│   Message  │     │ Message Key│     │  Payload   │     │  Envelope  │
└────────────┘     └────────────┘     └────────────┘     └────────────┘
                         │
                         ▼
                   Ratchet State
                   (updated)
```

**Step-by-step:**

1. **Generate message ID**: 16-byte UUID for deduplication
2. **Derive message key**: From ratchet state or sender key
3. **Build header extension**: Include ratchet parameters
4. **Generate nonce**: 12 random bytes
5. **Encrypt payload**: ChaCha20-Poly1305 with AAD
6. **Build envelope**: Assemble header + ciphertext
7. **Sign envelope**: Ed25519 over header + ciphertext
8. **Update state**: Advance ratchet chain

Implementation in `build_puse_envelope()` (lines 289-312):

```rust
pub fn build_puse_envelope(
    signing_key: &SigningKey,
    mut header: PUSEHeader,
    message_key: &[u8; 32],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let ciphertext = encrypt_puse_payload(
        message_key,
        &header.header_extension,
        &header.nonce,
        plaintext,
    )?;
    header.ciphertext_length = ciphertext.len() as u32;

    let mut bytes = encode_puse_header(&header)?;
    bytes.extend_from_slice(&ciphertext);

    let signature: Signature = signing_key.sign(&bytes);
    bytes.extend_from_slice(&signature.to_bytes());
    Ok(bytes)
}
```

### Receiving and Decrypting

```
┌────────────┐     ┌────────────┐     ┌────────────┐     ┌────────────┐
│  Envelope  │────►│  Verify    │────►│  Derive    │────►│  Decrypt   │
│   Bytes    │     │ Signature  │     │ Message Key│     │  Payload   │
└────────────┘     └────────────┘     └────────────┘     └────────────┘
                                            │
                                            ▼
                                      Ratchet State
                                      (updated)
```

**Step-by-step:**

1. **Decode envelope**: Parse binary format
2. **Validate structure**: Check magic, version, flags, lengths
3. **Verify signature**: Ed25519 verification against sender keys
4. **Parse header extension**: Extract encryption parameters
5. **Derive message key**: From ratchet state (may involve DH ratchet)
6. **Decrypt payload**: ChaCha20-Poly1305 with AAD verification
7. **Update state**: Store skipped keys, advance chains

Decoding in `decode_puse_envelope()` (lines 132-251).

Signature verification in `verify_puse_signature()` (lines 314-347):

```rust
pub fn verify_puse_signature(envelope_bytes: &[u8], signing_keys: &[String]) -> Result<()>
```

---

## Size Limits and Chunking

### Maximum Envelope Size

Per the protocol specification (line 129-130):

```rust
/// Maximum PUSE envelope size (1 MB) per RFC-0003 3.1
const PUSE_MAX_ENVELOPE_SIZE: usize = 1_048_576;
```

Enforced during decoding (lines 133-136):

```rust
if bytes.len() > PUSE_MAX_ENVELOPE_SIZE {
    return Err(PostUrbitError::InvalidInput("puse envelope too large"));
}
```

### Header Extension Limits

Header extensions are limited to 1024 bytes (lines 353-355):

```rust
if extension.len() > 1024 {
    return Err(PostUrbitError::InvalidInput("header extension too large"));
}
```

### Minimum Envelope Size

The minimum valid envelope is approximately 141 bytes (line 137-138):

```
4 (magic) + 1 (version) + 1 (flags) + 20 (sender) + 20 (recipient) +
16 (message_id) + 2 (ext_len) + 1 (min extension) + 12 (nonce) +
4 (ct_len) + 64 (signature) = 145 bytes minimum
```

### Large Message Strategy

For messages exceeding 1 MB:

1. **Chunking**: Split into multiple PUSE envelopes
2. **Chunk metadata**: Include sequence number and total count
3. **Reassembly**: Buffer and reorder at receiver
4. **Deduplication**: Use message_id for reassembly correlation

*(Note: Chunking protocol details may be application-specific)*

---

## Error Handling

### Error Types

Defined in `src/error.rs`:

```rust
pub enum PostUrbitError {
    InvalidInput(&'static str),    // Format/validation errors
    InvalidEncoding(&'static str), // Base64/encoding errors
    Crypto(&'static str),          // Cryptographic failures
    Io(String),                    // I/O errors
}
```

### Common Errors and Causes

| Error | Cause | Resolution |
|-------|-------|------------|
| `"puse magic"` | Magic bytes not "PUSE" | Check message format |
| `"puse version"` | Unsupported protocol version | Upgrade client |
| `"puse reserved flags not zero"` | Invalid flags | Re-encode message |
| `"puse envelope too large"` | Exceeds 1 MB | Chunk the message |
| `"puse envelope too short"` | Truncated data | Check transmission |
| `"puse signature invalid"` | Signature verification failed | Check sender keys |
| `"puse decrypt"` | Decryption failed | Keys out of sync |
| `"header extension required"` | Missing extension | Include ratchet data |
| `"too many skipped"` | >100 out-of-order messages | Resync session |

### Recovery Strategies

**Decryption Failure:**
1. Check if sender's identity has rotated keys
2. Verify you have the correct receiving chain
3. Consider session re-establishment

**Signature Verification Failure:**
1. Fetch latest identity document for sender
2. Check historical signing keys
3. Verify message wasn't tampered with

**Out-of-Order Limit Exceeded:**
1. Request message replay from sender
2. Re-establish ratchet session
3. Log for analysis

---

## Code Examples

### Creating a First Message (Initial)

```rust
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

// Generate ephemeral key for session establishment
let ephemeral_private = StaticSecret::random_from_rng(OsRng);
let ephemeral_public = PublicKey::from(&ephemeral_private);

// Derive initial keys (simplified - real code needs identity keys)
let (dh1, dh2) = two_dh_initiator(&identity_private, &ephemeral_private, &peer_public);
let (root_key, chain_key) = kdf_initial(&dh1, &dh2, &my_iid, &peer_iid)?;

// Initialize ratchet state
let mut ratchet = RatchetState::initialize_initiator(
    root_key,
    chain_key,
    peer_identity_public,
    peer_iid,
    ephemeral_private,
);

// Get first message key
let initial = ratchet.initial_message_key()?;

// Build header extension
let extension = build_initial_extension(initial.ephemeral_public);

// Build envelope
let header = PUSEHeader {
    flags: 0x00, // 1:1 message
    sender_iid: my_iid,
    recipient_iid: peer_iid,
    message_id: generate_uuid(),
    header_extension: extension,
    nonce: generate_nonce(),
    ciphertext_length: 0, // Set by build_puse_envelope
};

let envelope = build_puse_envelope(
    &signing_key,
    header,
    &initial.message_key,
    b"Hello, this is my first message!",
)?;
```

### Sending a Ratcheted Message

```rust
// Encrypt subsequent message (ratchet already initialized)
let (ratchet_header, message_key) = ratchet.ratchet_encrypt()?;

// Build ratchet extension
let extension = build_ratchet_extension(
    ratchet_header.dh_public,
    ratchet_header.pn,
    ratchet_header.n,
);

let header = PUSEHeader {
    flags: 0x00,
    sender_iid: my_iid,
    recipient_iid: peer_iid,
    message_id: generate_uuid(),
    header_extension: extension,
    nonce: generate_nonce(),
    ciphertext_length: 0,
};

let envelope = build_puse_envelope(
    &signing_key,
    header,
    &message_key,
    b"This message uses the double ratchet!",
)?;
```

### Receiving and Decrypting a Message

```rust
// Decode the envelope
let envelope = decode_puse_envelope(&received_bytes)?;

// Verify signature against sender's known keys
verify_puse_signature(&received_bytes, &sender_signing_keys)?;

// Parse the header extension to get ratchet parameters
let ext = parse_header_extension(&envelope.header.header_extension)?;

// Derive message key based on extension type
let message_key = match ext {
    HeaderExtension::Initial { ephemeral } => {
        // Initialize as responder
        let ephemeral_pub = PublicKey::from(ephemeral);
        let (dh1, dh2) = two_dh_responder(
            &my_identity_private,
            &sender_identity_public,
            &ephemeral_pub,
        );
        let (root, chain) = kdf_initial(&dh1, &dh2, &sender_iid, &my_iid)?;

        // Initialize ratchet state
        ratchet = RatchetState::initialize_responder(
            root,
            chain,
            ephemeral_pub,
            sender_iid,
            StaticSecret::random_from_rng(OsRng),
        );

        ratchet.initial_receive_message_key()?
    }
    HeaderExtension::Ratchet { dh_public, pn, n } => {
        let header = RatchetHeader { dh_public, pn, n };
        ratchet.ratchet_decrypt(&header)?
    }
    HeaderExtension::Group { key_id, iteration } => {
        // Look up sender key for this group/sender
        sender_key.advance(&group_id)?
    }
};

// Decrypt the payload
let plaintext = decrypt_puse_payload(
    &message_key,
    &envelope.header.header_extension,
    &envelope.header.nonce,
    &envelope.ciphertext,
)?;

println!("Decrypted: {}", String::from_utf8_lossy(&plaintext));
```

### Sending a Group Message

```rust
use crate::group::{generate_sender_key, SenderKey};

// Generate or retrieve sender key for this group
let mut sender_key = generate_sender_key(my_iid, "2025-01-20T12:00:00Z")?;

// Advance key to get message key
let message_key = sender_key.advance(&group_id)?;

// Build group extension
let extension = build_group_extension(sender_key.key_id, sender_key.iteration)?;

let header = PUSEHeader {
    flags: 0x01, // Group message
    sender_iid: my_iid,
    recipient_iid: group_id_as_bytes, // Group ID in recipient field
    message_id: generate_uuid(),
    header_extension: extension,
    nonce: generate_nonce(),
    ciphertext_length: 0,
};

let envelope = build_puse_envelope(
    &signing_key,
    header,
    &message_key,
    b"Hello everyone in the group!",
)?;

// Check if key needs rotation
if should_rotate_sender_key(&sender_key, Utc::now())? {
    // Generate new sender key and distribute to group members
    let new_key = generate_sender_key(my_iid, Utc::now().to_rfc3339().as_str())?;
    // ... distribute via 1:1 messages to each member
}
```

---

## Reference

### Key Source Files

| File | Purpose |
|------|---------|
| `src/messaging.rs` | PUSE envelope format, encryption/decryption |
| `src/ratchet.rs` | Double Ratchet implementation, key derivation |
| `src/group.rs` | Group messaging, sender keys, roles |
| `src/encoding.rs` | Base64, Crockford Base32 encoding |
| `src/error.rs` | Error types |
| `src/identity.rs` | Identity management, signing keys |
| `src/transport.rs` | QUIC transport, TLS binding |
| `src/messaging_service.rs` | High-level messaging service |

### Constants

| Constant | Value | Location |
|----------|-------|----------|
| `PUSE_MAGIC` | `b"PUSE"` | messaging.rs:9 |
| `PUSE_VERSION` | `1` | messaging.rs:10 |
| `PUSE_MAX_ENVELOPE_SIZE` | 1,048,576 bytes | messaging.rs:130 |
| `RATCHET_INFO` | `"post-urbit-ratchet-v1"` | ratchet.rs:10 |
| `X3DH_INFO` | `"post-urbit-x3dh-v1"` | ratchet.rs:11 |
| `SENDER_KEY_INFO` | `"post-urbit-sender-key-v1:"` | ratchet.rs:12 |
| Max skip messages | 100 | ratchet.rs:190 |
| Group key rotation | 100 messages / 7 days | group.rs:51-52 |

### Test Vectors

Test vector 10 (Initial message, lines 435-492 in messaging.rs):
- Signing seed: `033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40`
- Sender IID: `586a763f2c82b31a0c5de9dcaef01e0261e0785b`
- Recipient IID: `d15c5160257b140ed4bf313fbf92eef8a266de56`
- Plaintext: `"hello"`
- Expected envelope: `505553450100586a763f2c82b31a0c5de9dcaef01e0261e0785b...`

Test vector 11 (Ratchet message, lines 494-553):
- Same keys, second message with ratchet extension
- Plaintext: `"hello again"`

### Related Documentation

- [Identity Protocol](./identity.md) - IID derivation, key management
- [Transport Layer](./transport.md) - QUIC, TLS binding
- [Building Apps](./apps/building-apps.md) - WASM app integration
