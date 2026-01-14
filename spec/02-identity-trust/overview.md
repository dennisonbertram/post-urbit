# Identity & Trust Layer Overview

## Purpose

The Identity & Trust layer provides the foundational identity primitive for the entire system. Every other layer depends on it:

- **Transport** needs identity for mutual authentication
- **Messaging** needs identity for E2E encryption key derivation
- **Sync** needs identity for authorization and signing
- **Apps** needs identity for capability grants

## Core Design Principles

1. **Self-sovereign**: Users generate their own identities without permission
2. **Rotatable**: Keys can be changed without losing identity continuity
3. **Recoverable**: Identity survives device loss via preconfigured recovery
4. **Revocable**: Compromised keys can be declared invalid
5. **Portable**: Identity can move between nodes/providers
6. **Minimal**: Only essential claims; extensible for optional metadata

## Component Files

| File | Purpose |
|------|---------|
| `identity-document-schema.md` | The canonical identity document format |
| `key-rotation.md` | Protocol for rotating keys safely |
| `recovery-mechanisms.md` | Methods for recovering identity after key loss |
| `revocation.md` | Declaring keys compromised |
| `name-resolution.md` | Human-friendly names (DNS, aliases) |
| `interfaces.md` | API surface for identity operations |

## Dependencies

- **Requires**: Cryptographic primitives (Ed25519, X25519)
- **Required by**: All other layers

## Key Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Signature algorithm | Ed25519 | Widely reviewed, fast, small signatures |
| Encryption key exchange | X25519 | Proven, compatible with modern protocols |
| Document format | Canonical JSON | Human-readable, deterministic serialization |
| Identity binding | Public key hash | No external registry required for base identity |
