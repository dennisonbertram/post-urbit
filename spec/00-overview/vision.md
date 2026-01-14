# Post-Urbit Vision

## Goal

A network where anyone can run a personal node (on a VPS, home server, or eventually a device) that provides:

1. **Identity you control** - portable, recoverable, revocable
2. **Secure communication** - 1:1 and group messaging
3. **Local-first applications** - your data lives with you; sync is selective
4. **Interoperation across nodes** - without a central owner
5. **Governance as an explicit layer** - not an emergent property of address ownership

## Non-Goals

- Reproducing Urbit's hierarchical address space (galaxies/stars/planets) as a power structure
- Creating an identity token with resale value
- Shipping a novel language/runtime just to be different (unless strictly necessary)

## System Principles (Design Constraints)

| Principle | Meaning |
|-----------|---------|
| Identity ≠ routing ≠ governance | These are separate concerns with separate solutions |
| No permanent root keys | No single point of compromise or control |
| Exit is always possible | Portable identity + portable data |
| Scarcity maps to real resources only | Bandwidth, storage, compute - not artificial tokens |
| Composable trust | Users choose trust providers; no mandatory "lords" |

## Architecture Layers

```
┌─────────────────────────────────────────┐
│  5. UX, Packaging, Operations           │
├─────────────────────────────────────────┤
│  4. App Runtime & Permissions           │
├─────────────────────────────────────────┤
│  3. Messaging & Sync                    │
├─────────────────────────────────────────┤
│  2. Identity & Trust                    │
├─────────────────────────────────────────┤
│  1. Transport & Connectivity            │
└─────────────────────────────────────────┘
```

## Technology Choices (Starting Points)

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Node implementation | Rust or Go | Security + performance (Rust) or simplicity + ops (Go) |
| Transport | QUIC | Encrypted, multiplexed, modern |
| Crypto | Modern, widely-reviewed primitives | No novel cryptography |
| Sandbox | WASM/WASI | Portable, sandboxed, deterministic |
| Data sync | CRDTs | Conflict-free, local-first friendly |
| Discovery | DHT + optional directory | Directory is convenience, not authority |
