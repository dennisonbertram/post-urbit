# Post-Urbit Documentation

Post-Urbit is a decentralized personal node infrastructure. You run a node, you own your data, you control your compute.

## Documentation

### Getting Started
- [Running a Node](./running-a-node.md) *(coming soon)*
- [Configuration](./configuration.md) *(coming soon)*

### Core Concepts
- [Identity & IIDs](./identity.md) *(coming soon)*
- [Transport & Connections](./transport.md) *(coming soon)*
- [Messaging Protocol](./messaging.md) *(coming soon)*

### Building Apps
- [**Building Apps Guide**](./apps/building-apps.md) - Complete guide to building WASM apps

### API Reference
- [HTTP API](./api/http.md) *(coming soon)*
- [Host API for Apps](./api/host.md) *(coming soon)*

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      Applications                             │
│   ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐            │
│   │  Notes  │ │  Chat   │ │ Calendar│ │  Custom │            │
│   └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘            │
│        └───────────┴──────────┴───────────┘                  │
│                         │                                     │
│  ┌──────────────────────▼───────────────────────────────┐    │
│  │                 WASM Runtime                          │    │
│  │   storage | messaging | contacts | sync | notify     │    │
│  └──────────────────────┬───────────────────────────────┘    │
│                         │                                     │
├─────────────────────────┼────────────────────────────────────┤
│                         │           Node Core                 │
│  ┌──────────────────────▼───────────────────────────────┐    │
│  │                  HTTP API                             │    │
│  └──────────────────────┬───────────────────────────────┘    │
│                         │                                     │
│  ┌──────────┐ ┌─────────▼──┐ ┌──────────┐ ┌──────────┐       │
│  │ Identity │ │  Messaging │ │  Mailbox │ │   Sync   │       │
│  └────┬─────┘ └─────┬──────┘ └────┬─────┘ └────┬─────┘       │
│       └─────────────┼─────────────┴────────────┘             │
│                     │                                         │
│  ┌──────────────────▼───────────────────────────────────┐    │
│  │              QUIC Transport (TLS 1.3)                 │    │
│  └──────────────────────────────────────────────────────┘    │
│                         │                                     │
│  ┌──────────────────────▼───────────────────────────────┐    │
│  │                    DHT                                │    │
│  └──────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

## Key Principles

1. **Self-Sovereign Identity** - Your IID is derived from your cryptographic keys. No central authority.

2. **End-to-End Encryption** - All messages use double-ratchet encryption. Only you and the recipient can read them.

3. **You Own Your Data** - Data lives on your node. Apps are just code that runs on your data.

4. **Capability-Based Security** - Apps request permissions. You grant only what's needed.

5. **Peer-to-Peer** - Nodes connect directly. No central servers routing your traffic.

## Quick Links

- [GitHub Repository](https://github.com/dennisonbertram/post-urbit)
- [RFC Specifications](../specs/)
- [Issue Tracker](https://github.com/dennisonbertram/post-urbit/issues)
