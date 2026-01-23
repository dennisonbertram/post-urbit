# Post-Urbit Documentation

Post-Urbit is your personal server for the decentralized internet. You run a node, you own your data, you control your compute.

## Start Here

**[Introduction: What is Post-Urbit?](./introduction.md)** - The vision, the problem we're solving, and what this enables.

## Documentation

### Core Concepts

| Document | Description |
|----------|-------------|
| [Identity & IIDs](./identity.md) | IID derivation, identity documents, key management, rotation, social recovery |
| [Transport Layer](./transport.md) | QUIC transport, TLS 1.3, identity handshake, glare resolution, NAT traversal |
| [Messaging Protocol](./messaging.md) | PUSE envelope format, double ratchet encryption, 1:1 and group messages |
| [Mailbox & DHT](./mailbox-and-dht.md) | Async message delivery, bearer tokens, distributed hash table |
| [Sync & Runtime](./sync-and-runtime.md) | CRDT sync protocol, WASM sandbox, app lifecycle, capabilities |

### Building Apps

| Document | Description |
|----------|-------------|
| [Building Apps Guide](./apps/building-apps.md) | Complete guide to building WASM apps with examples |

### API Reference

| Document | Description |
|----------|-------------|
| [HTTP API Reference](./api/http-api.md) | Complete REST API documentation with examples |

### Frontend (Shell)

| Document | Description |
|----------|-------------|
| [Shell README](../packages/shell/README.md) | Frontend setup, API integration, and component overview |
| [Visual Design Spec](./specs/10-VISUAL_DESIGN.md) | System 7 design guidelines and component specifications |

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      Applications                             │
│   ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐            │
│   │  Notes  │ │  Chat   │ │ Calendar│ │  Custom │            │
│   └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘            │
│        └───────────┴──────────┴───────────┘                  │
│                         │                                     │
│  ┌──────────────────────▼───────────────────────────────────┐│
│  │                 WASM Runtime (Sandbox)                    ││
│  │   storage | messaging | contacts | sync | notify          ││
│  └──────────────────────┬───────────────────────────────────┘│
│                         │                                     │
├─────────────────────────┼────────────────────────────────────┤
│                         │           Node Core                 │
│  ┌──────────────────────▼───────────────────────────────────┐│
│  │                   HTTP API                                ││
│  │    /admin/v1/*  (management)   /messages/*  (mailbox)     ││
│  └──────────────────────┬───────────────────────────────────┘│
│                         │                                     │
│  ┌──────────┐ ┌─────────▼──┐ ┌──────────┐ ┌──────────┐       │
│  │ Identity │ │  Messaging │ │  Mailbox │ │   Sync   │       │
│  │          │ │  (PUSE)    │ │          │ │  (CRDT)  │       │
│  └────┬─────┘ └─────┬──────┘ └────┬─────┘ └────┬─────┘       │
│       └─────────────┼─────────────┴────────────┘             │
│                     │                                         │
│  ┌──────────────────▼───────────────────────────────────────┐│
│  │              QUIC Transport (TLS 1.3)                     ││
│  │   Identity Handshake | Glare Resolution | NAT Traversal   ││
│  └──────────────────────────────────────────────────────────┘│
│                         │                                     │
│  ┌──────────────────────▼───────────────────────────────────┐│
│  │                    DHT                                    ││
│  │   Identity Lookup | Peer Discovery | Endpoint Resolution  ││
│  └──────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────┘
```

## Key Principles

1. **Self-Sovereign Identity** - Your IID is derived from your cryptographic keys. No central authority.

2. **End-to-End Encryption** - All messages use double-ratchet encryption. Only you and the recipient can read them.

3. **You Own Your Data** - Data lives on your node. Apps are just code that runs on your data.

4. **Capability-Based Security** - Apps request permissions. You grant only what's needed.

5. **Peer-to-Peer** - Nodes connect directly. No central servers routing your traffic.

## Quick Start

### Backend

```bash
# Build the node
cargo build --release

# Run the node (starts HTTP API on localhost:4433)
cargo run

# Or run the release build
./target/release/post-urbit-core

# Development mode (bypasses authentication)
cargo run -- run --dev
```

### Frontend

```bash
# Navigate to shell package
cd packages/shell

# Install dependencies and start dev server
npm install
npm run dev
```

Open `http://localhost:5173` to access the System 7-styled shell interface.

See the [HTTP API Reference](./api/http-api.md) for endpoint documentation.

## Source Files

| Component | Source | Lines |
|-----------|--------|-------|
| Identity | `src/identity.rs` | ~1,700 |
| Transport | `src/transport.rs` | ~1,600 |
| Messaging | `src/messaging.rs` | ~500 |
| Mailbox | `src/mailbox*.rs` | ~900 |
| DHT | `src/dht.rs` | ~700 |
| Sync | `src/sync.rs` | ~700 |
| WASM Runtime | `src/runtime*.rs` | ~1,500 |
| HTTP API | `src/node_http.rs` | ~2,000 |

## Quick Links

- [GitHub Repository](https://github.com/dennisonbertram/post-urbit)
- [Technical Specifications](./specs/)
- [Architecture Decision Records](./adrs/)
- [Issue Tracker](https://github.com/dennisonbertram/post-urbit/issues)
