# Transport & Connectivity Layer Overview

## Purpose

The Transport & Connectivity layer provides reliable, secure communication between nodes. It handles:

- **Connection establishment** over the internet, including NAT traversal
- **Peer authentication** binding transport connections to cryptographic identities
- **Message delivery** for higher layers (identity updates, messaging, sync)
- **Relay services** for nodes that cannot establish direct connections

## Design Principles

1. **QUIC-first**: Modern, encrypted, multiplexed transport as the foundation
2. **NAT-friendly**: Work across home networks, mobile, corporate firewalls
3. **Relay is fallback, not authority**: Relays provide connectivity, not control
4. **Identity-bound**: Every connection is authenticated to an IID
5. **Multi-path**: Support multiple connectivity options (direct, relay, mailbox)

## Component Files

| File | Purpose |
|------|---------|
| `quic-integration.md` | QUIC configuration and usage |
| `nat-traversal.md` | Hole punching, STUN-like discovery |
| `relay-protocol.md` | Relay service protocol and trust model |
| `peer-handshake.md` | Identity-authenticated connection establishment |
| `interfaces.md` | API surface for transport operations |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Higher Layers                             │
│         (Identity, Messaging, Sync, Apps)                    │
├─────────────────────────────────────────────────────────────┤
│                   Transport API                              │
│    connect() | send() | receive() | listen()                 │
├─────────────────────────────────────────────────────────────┤
│              Connection Manager                              │
│    Path selection | Retry logic | Multi-path                 │
├──────────────┬──────────────┬──────────────┬────────────────┤
│    Direct    │    Relay     │   Mailbox    │     DHT        │
│    (QUIC)    │   (QUIC)     │   (HTTPS)    │   (lookup)     │
├──────────────┴──────────────┴──────────────┴────────────────┤
│                NAT Traversal                                 │
│    STUN-like | Hole punching | Port mapping                  │
├─────────────────────────────────────────────────────────────┤
│                    Network (UDP/IP)                          │
└─────────────────────────────────────────────────────────────┘
```

## Connection Types

| Type | Description | Use Case |
|------|-------------|----------|
| **Direct** | QUIC connection between nodes | Both nodes reachable |
| **Relay** | QUIC via relay server | NAT prevents direct connection |
| **Mailbox** | Store-and-forward (HTTPS) | Recipient offline |

## DHT Integration

The Transport layer provides DHT services for peer discovery. See `00-shared/layer-integration.md` for the authoritative DHT record format.

**DHT Record Format:**
- Key: `SHA256("post-urbit:identity:" || iid)`
- Value: IDOC binary envelope
- TTL: 86400 seconds (24 hours)
- Signature: Ed25519 by document's signing key

DHT nodes verify the signature before storing, preventing unauthorized updates.

## Key Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Transport protocol | QUIC | Built-in encryption, multiplexing, 0-RTT |
| NAT traversal | ICE-lite + custom STUN | Proven techniques, no TURN dependency |
| Relay trust model | Untrusted, encrypted payloads | Relays can't read content |
| Connection auth | TLS + identity proof | Bind transport to IID |
| Addressing | IID + endpoint hints | IID is stable, endpoints change |
| DHT storage | Full IDOC, signed | Verifiable records, spam prevention |

## Dependencies

### Requires from Identity Layer
- `IdentityDocument` with endpoints for peer discovery
- Signing keys for authentication
- IID for addressing

### Provides to Higher Layers
- Authenticated bidirectional streams
- Message delivery (reliable, ordered)
- Connection state events
- Peer reachability information

## Security Model

1. **End-to-end encryption**: QUIC TLS for transport, additional E2E for messages
2. **Peer authentication**: Every connection proves identity ownership
3. **Relay blindness**: Relays see encrypted blobs, not content
4. **No traffic analysis protection**: Metadata (who talks to whom) visible to relays
5. **Denial of service**: Rate limiting at all layers
