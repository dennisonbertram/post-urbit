# Success Criteria

## Functional Requirements (MVP Scope)

### Node
- [ ] Runs as a daemon with an admin UI
- [ ] Stores user data and app state
- [ ] Supports encrypted backup and restore

### Identity
- [ ] Cryptographic identity with rotation and recovery
- [ ] Human-friendly names (not necessarily globally unique at first)
- [ ] Verifiable profiles and key discovery

### Connectivity
- [ ] Peer discovery
- [ ] NAT traversal and optional relays
- [ ] "Store-and-forward" delivery for intermittently online nodes

### Messaging
- [ ] End-to-end encrypted 1:1
- [ ] End-to-end encrypted groups
- [ ] Device/node key changes without breaking everything

### App Runtime
- [ ] Sandboxed apps with explicit permissions
- [ ] Simple API for messaging, storage, contacts, and notifications

### Data Model / Sync
- [ ] Local-first storage
- [ ] Selective replication (per-app, per-peer)
- [ ] Conflict-safe merges (CRDT or equivalent)

## Operational Requirements

- [ ] Upgradeable with signed releases
- [ ] Observable (logs/metrics) without leaking user content
- [ ] Abuse controls without centralized moderation

## User Journey Success Criteria

A user can:

1. **Install a node** - Single command or container deployment
2. **Claim an identity** - Generate keys, set human-friendly name
3. **Add a contact** - Exchange identity documents, verify
4. **Message reliably across NAT** - Direct or via relay, transparently
5. **Install an app** - From trusted source, sandboxed
6. **Sync app data to a second node** - Own data replicates to own nodes
7. **Recover identity after key loss** - Using preconfigured recovery method

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
