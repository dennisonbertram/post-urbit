# Messaging & Sync Layer Overview

## Purpose

The Messaging & Sync layer provides:
1. **Secure messaging**: End-to-end encrypted 1:1 and group communication
2. **Data synchronization**: Replicate application state across devices and peers
3. **Offline support**: Store-and-forward via mailbox servers

## Design Principles

### End-to-End Encryption

All message content is encrypted such that only intended recipients can decrypt. Transport layer (QUIC TLS) provides transport security; this layer adds E2E encryption on top.

| Property | Guarantee |
|----------|-----------|
| **Confidentiality** | Only sender and recipients can read content |
| **Authenticity** | Recipients can verify sender identity |
| **Integrity** | Tampering is detectable |
| **Forward secrecy** | Compromise of current keys doesn't reveal past messages |
| **Non-repudiation** | Sender's signature proves authorship (NOT deniable) |

**Note on Non-Repudiation**: This system uses Ed25519 signatures on all messages, providing strong non-repudiation. Recipients can prove to third parties that the sender authored a message. This is a deliberate design choice favoring accountability over deniability. Applications requiring deniability should build additional layers (e.g., session MACs instead of signatures for message authentication).

### Offline-First

The system assumes peers are frequently offline. Messages are stored locally and synchronized when connectivity resumes.

### Eventually Consistent

For sync operations, the system provides eventual consistency with conflict resolution. Not all applications need strong consistency.

## Layer Components

```
┌─────────────────────────────────────────────────────────────┐
│                     Applications                             │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐ │
│  │   1:1 Messaging │  │ Group Messaging │  │   Sync       │ │
│  └────────┬────────┘  └────────┬────────┘  └──────┬───────┘ │
│           │                    │                   │         │
│  ┌────────▼────────────────────▼───────────────────▼───────┐ │
│  │                  Secure Envelope                        │ │
│  │         (E2E encryption, signing, framing)              │ │
│  └─────────────────────────┬───────────────────────────────┘ │
│                            │                                 │
├────────────────────────────▼─────────────────────────────────┤
│                     Transport Layer                          │
│              (QUIC streams, relay, mailbox)                  │
└──────────────────────────────────────────────────────────────┘
```

## Component Overview

### Secure Envelope

The foundation for all messages. Provides:
- Authenticated encryption (ChaCha20-Poly1305)
- Sender authentication (Ed25519 signatures)
- Key derivation (X25519 + HKDF)
- Message framing and versioning

### 1:1 Messaging

Direct person-to-person messaging:
- Double ratchet for forward secrecy
- Message ordering with sequence numbers
- Read receipts and typing indicators
- Offline delivery via mailbox

### Group Messaging

Multi-party conversations:
- Sender keys for efficient encryption
- Group membership management
- Admin roles and permissions
- Message history for new members

### Sync Protocol

Application data replication:
- CRDT-based conflict resolution
- Merkle trees for efficient sync
- Selective sync (partial datasets)
- Cross-device synchronization

## Dependencies

| Dependency | Provider | Usage |
|------------|----------|-------|
| Identity | 02-identity-trust | Sender/recipient identity, signing keys, encryption keys |
| Transport | 01-transport-connectivity | QUIC streams, connection management |
| Mailbox | 00-shared/layer-integration | Offline message storage |

## Stream Types

This layer uses these QUIC stream types (defined in 01-transport-connectivity):

| Stream Type | ID | Usage |
|-------------|-----|-------|
| Message | 0x03 | 1:1 and group messages |
| Sync | 0x04 | Data synchronization |
| Bulk | 0x05 | Large file transfers |

## Security Model

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Passive eavesdropper | E2E encryption |
| Active MITM | Identity verification, signed messages |
| Compromised relay | E2E encryption (relay sees encrypted blobs) |
| Compromised mailbox | E2E encryption (mailbox sees encrypted blobs) |
| Key compromise | Forward secrecy limits damage |
| Replay attacks | Message sequence numbers, nonces |

### Non-Goals

- **Anonymity**: Sender/recipient IIDs are visible in routing metadata
- **Traffic analysis resistance**: Timing and size patterns visible
- **Guaranteed delivery**: Best-effort with retries and timeouts

## Message Lifecycle

```
Sender                   Network                  Recipient
  │                         │                         │
  │  1. Compose message     │                         │
  │  2. Encrypt (E2E)       │                         │
  │  3. Sign                │                         │
  │  4. Frame               │                         │
  │                         │                         │
  │ ───────── Send ───────► │                         │
  │                         │                         │
  │                         │  (if online)            │
  │                         │ ────────────────────────►
  │                         │                         │
  │                         │  (if offline → mailbox) │
  │                         │ ──► Mailbox ───────────►│
  │                         │                         │
  │                         │                         │  5. Receive
  │                         │                         │  6. Verify signature
  │                         │                         │  7. Decrypt
  │                         │                         │  8. Process
  │                         │                         │
  │ ◄────────── ACK ─────── │ ◄──────────────────────
  │                         │                         │
```

## Performance Targets

| Metric | Target |
|--------|--------|
| Message encryption latency | < 5ms |
| Message decryption latency | < 5ms |
| Group message (1000 members) | < 50ms |
| Sync throughput | > 1000 ops/sec |
| Message size limit | 1 MB (larger via bulk transfer) |
