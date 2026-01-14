# App Runtime Layer Overview

## Purpose

The App Runtime layer enables third-party applications to run on a personal node with controlled access to user data, messaging, and sync capabilities. Applications are sandboxed with explicit capability-based permissions.

## Design Principles

### Principle of Least Privilege

Applications receive only the permissions they need and request. Users grant permissions explicitly.

### Sandboxed Execution

Applications run in isolated WASM/WASI sandboxes with no direct access to the host filesystem, network, or other applications.

### Local-First Data

Application data is stored locally on the node and optionally synced via the Sync Protocol (layer 03). Applications cannot directly access the network.

### Deterministic Execution

WASM execution is deterministic, enabling reproducible behavior and potential future features like computational proofs.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Application Layer                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
│  │   App A     │  │   App B     │  │   App C     │  │   System    │ │
│  │  (WASM)     │  │  (WASM)     │  │  (WASM)     │  │   Apps      │ │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘ │
│         │                │                │                │         │
├─────────▼────────────────▼────────────────▼────────────────▼─────────┤
│                      Runtime Host API                                 │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  Storage │ Messaging │ Contacts │ Sync │ Notifications │ System │ │
│  └─────────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│                    Capability Enforcement                            │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  Permission checks │ Resource limits │ Audit logging            │ │
│  └─────────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│                     WASM/WASI Runtime                                │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  Wasmtime │ Memory isolation │ Fuel metering │ Instance mgmt    │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                   Lower Layers (01-03)                               │
│  Transport │ Identity │ Messaging & Sync                            │
└─────────────────────────────────────────────────────────────────────┘
```

## Specification Documents

| Document | Purpose |
|----------|---------|
| `abi.md` | **Authoritative ABI specification** - imports, exports, memory model, async model |
| `wasm-sandbox.md` | Sandbox configuration, WASI capabilities, execution limits |
| `capability-system.md` | Permission model, enforcement, method-to-capability mapping |
| `api-surface.md` | Host API methods, request/response formats |
| `manifest-schema.md` | App package manifest format, validation |
| `interfaces.md` | TypeScript interface definitions |

## Components

### WASM Sandbox

The isolated execution environment for applications:
- WebAssembly core execution (Wasmtime recommended)
- WASI filesystem abstraction (virtualized, no `/data` - use Host Storage API)
- Fuel-based execution limits
- Memory quotas
- Instance lifecycle management
- Polling-based async model (no callbacks, no reentrancy)

### Capability System

Permission enforcement:
- Manifest-declared capabilities (required vs optional)
- User-granted permissions at install or first use
- Runtime capability checking (authoritative mapping in `capability-system.md`)
- Revocation support with defined behavior

### Host API

The interface between applications and the node:
- Storage API (key-value with versioning)
- Messaging API (send, subscribe - no separate receive capability)
- Contacts API (read, with permission levels)
- Sync API (documents, CRDT operations)
- Notifications API (alerts, badges)
- System API (time, random, deterministic random, identity info)
- Inter-App API (invoke, share with explicit exports)

### App Lifecycle

Application installation, execution, and management:
- Package format and verification
- Installation process
- Startup/shutdown
- Updates and migrations
- Uninstallation and data cleanup

## Dependencies

| Dependency | Provider | Usage |
|------------|----------|-------|
| Identity | 02-identity-trust | App signing, user identity |
| Messaging | 03-messaging-sync | Inter-app and user messaging |
| Sync | 03-messaging-sync | App data replication |
| Storage | Node daemon | Local persistence |

## Security Model

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Malicious app accessing user data | Capability-based permissions, user consent |
| App reading other apps' data | Process isolation, separate storage namespaces |
| Resource exhaustion (DoS) | Fuel metering, memory limits, storage quotas |
| Supply chain attack | Signed packages, content-addressed dependencies |
| Privilege escalation | No host code execution, restricted WASI |
| Data exfiltration | No direct network access, messaging requires permission |

### Non-Goals

- **Full system isolation**: Apps share the same node process; we rely on WASM sandbox, not OS-level isolation
- **Real-time guarantees**: No hard timing guarantees for app execution
- **Cross-node app execution**: Apps run on individual nodes, not distributed compute

## Performance Targets

| Metric | Target |
|--------|--------|
| App cold start | < 500ms |
| Host API call overhead | < 1ms |
| Storage read/write | < 10ms for typical operations |
| Memory per app | Configurable, default 64MB max |
| Fuel per invocation | Configurable per operation type |
