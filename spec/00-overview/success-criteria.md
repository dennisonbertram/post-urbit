# Success Criteria

## Functional Requirements (MVP Scope)

### Node
- [ ] SC-NODE-01: Runs as a daemon with an admin UI
- [ ] SC-NODE-02: Stores user data and app state
- [ ] SC-NODE-03: Supports encrypted backup and restore

### Identity
- [ ] SC-ID-01: Cryptographic identity with rotation and recovery
- [ ] SC-ID-02: Human-friendly names (not necessarily globally unique at first)
- [ ] SC-ID-03: Verifiable profiles and key discovery

### Connectivity
- [ ] SC-CONN-01: Peer discovery
- [ ] SC-CONN-02: NAT traversal and optional relays
- [ ] SC-CONN-03: "Store-and-forward" delivery for intermittently online nodes

### Messaging
- [ ] SC-MSG-01: End-to-end encrypted 1:1
- [ ] SC-MSG-02: End-to-end encrypted groups
- [ ] SC-MSG-03: Device/node key changes without breaking everything

### App Runtime
- [ ] SC-APP-01: Sandboxed apps with explicit permissions
- [ ] SC-APP-02: Simple API for messaging, storage, contacts, and notifications

### Data Model / Sync
- [ ] SC-SYNC-01: Local-first storage
- [ ] SC-SYNC-02: Selective replication (per-app, per-peer)
- [ ] SC-SYNC-03: Conflict-safe merges (CRDT or equivalent)

## Operational Requirements

- [ ] SC-OPS-01: Upgradeable with signed releases
- [ ] SC-OPS-02: Observable (logs/metrics) without leaking user content
- [ ] SC-OPS-03: Abuse controls without centralized moderation

## User Journey Success Criteria

A user can:

1. **SC-JOURNEY-01: Install a node** - Single command or container deployment
2. **SC-JOURNEY-02: Claim an identity** - Generate keys, set human-friendly name
3. **SC-JOURNEY-03: Add a contact** - Exchange identity documents, verify
4. **SC-JOURNEY-04: Message reliably across NAT** - Direct or via relay, transparently
5. **SC-JOURNEY-05: Install an app** - From trusted source, sandboxed
6. **SC-JOURNEY-06: Sync app data to a second node** - Own data replicates to own nodes
7. **SC-JOURNEY-07: Recover identity after key loss** - Using preconfigured recovery method

## Specification Completeness Criteria

Each component is "done" when:

| Criterion | Description |
|-----------|-------------|
| Data Structures | Complete schemas with types, constraints, examples |
| Interfaces | Function signatures, I/O types, error conditions |
| State Machines | All states, transitions, edge cases |
| Wire Formats | Byte-level encoding where applicable |
| Error Handling | All failure modes and recovery procedures |
| Dependencies | What it requires from other components |
| Test Scenarios | Concrete cases that prove correctness |
| Security | Threat surface documented |
| No TBDs | All questions resolved |

These criteria apply to components in MVP scope (see `spec/progress.md`). Deferred layers are treated as `n/a` until started.
