# WASM Sandbox

## Overview

Applications run in isolated WebAssembly sandboxes using WASI (WebAssembly System Interface). This document specifies the sandbox configuration, resource limits, and execution model.

## Runtime Selection

**Recommended**: Wasmtime (bytecodealliance/wasmtime)

| Feature | Requirement |
|---------|-------------|
| WASI support | wasi_snapshot_preview1 + preview2 |
| Fuel metering | Required for execution limits |
| Memory limits | Required for isolation |
| Component model | Future: for better composition |
| Async support | Required for non-blocking host calls |

## WASI Capabilities

**Important:** To maintain capability enforcement and audit logging, time and randomness are accessed through the Host API, not raw WASI calls. See `abi.md` for the authoritative ABI specification.

### Enabled WASI Capabilities

| Capability | Reason | Notes |
|------------|--------|-------|
| `fd_read`, `fd_write` | For virtualized filesystem | Gated by `storage:app` |
| `poll_oneoff` | Async waiting | Used internally by `host.poll` |
| `proc_exit` | Clean termination | Always allowed |

### Disabled WASI Capabilities

| Capability | Reason | Alternative |
|------------|--------|-------------|
| `clock_time_get` | Capability enforcement | Use `system.get_time` Host API |
| `random_get` | Capability enforcement + audit | Use `system.get_random` Host API |
| `sock_*` | No direct network access | Use messaging layer |
| `path_open` (host paths) | No host filesystem access | Use virtual filesystem |
| `environ_*` | No environment variables | Use `system.get_app_info` |
| `args_*` | Args passed via host API | Use `handle` input |
| `proc_raise` | No signal handling | N/A |

### Rationale for Disabling WASI Time/Random

1. **Capability enforcement**: Apps must have `system:time` or `system:random` capability
2. **Audit logging**: All accesses are logged for user review
3. **Deterministic replay**: Host API provides both crypto-random and deterministic modes
4. **Privacy**: Wall clock access reveals timezone; capability lets users control this

## Storage Architecture

Apps have two storage mechanisms. **The Host Storage API is the primary mechanism** for persistent data; the virtual filesystem is for assets and ephemeral data only.

### Storage Mechanisms

| Mechanism | Purpose | Persistence | Capability |
|-----------|---------|-------------|------------|
| **Host Storage API** | Structured data, app state | Persistent, versioned | `storage:app` |
| Virtual FS `/app/*` | Package assets | Read-only | None (always allowed) |
| Virtual FS `/cache/*` | Ephemeral cache | Cleared on restart | None (always allowed) |
| Virtual FS `/tmp/*` | Temporary files | Cleared on exit | None (always allowed) |

**Key differences:**
- Host Storage API supports optimistic concurrency (versions)
- Host Storage API has audit logging
- Host Storage API works with sync layer for multi-device
- Virtual FS is purely local, no sync, no versioning

### Quota Enforcement

Quotas are enforced by the manifest `storage.quota` field and apply to:
- All Host Storage API writes
- Virtual FS `/cache/*` writes

The quota is a single unified limit. `/tmp/*` has a separate 5MB limit that doesn't count against quota.

## Filesystem Virtualization

Apps see a virtual filesystem for assets and temporary data only.

### Virtual Filesystem Layout

```
/                           # Root (read-only)
├── app/                    # App package contents (read-only)
│   ├── main.wasm          # App entry point
│   ├── assets/            # Static assets
│   └── ...
├── cache/                  # Ephemeral cache (read-write, cleared on restart)
│   └── ...
└── tmp/                    # Temporary files (read-write, cleared on exit)
    └── ...
```

**Note:** There is no `/data/` virtual filesystem path. Apps use the Host Storage API for persistent data. See `api-surface.md` for storage methods.

### Filesystem Limits

| Resource | Default | Configurable |
|----------|---------|--------------|
| Cache storage (`/cache`) | Counts against quota | Yes, via quota |
| Temp storage (`/tmp`) | 5 MB | No |
| Max file size | 10 MB | No |
| Max open files | 32 | No |
| Max path length | 255 bytes | No |

See manifest `storage.quota` for persistent storage limits (Host Storage API).

### Filesystem Operations

All filesystem operations are translated to the node's storage layer:

```typescript
interface VirtualFilesystem {
  // Path must start with /app/ (read-only), /cache/, or /tmp/
  open(path: string, flags: OpenFlags): Result<FileDescriptor, FsError>;
  read(fd: FileDescriptor, buffer: Uint8Array): Result<number, FsError>;
  write(fd: FileDescriptor, data: Uint8Array): Result<number, FsError>;
  close(fd: FileDescriptor): Result<void, FsError>;
  stat(path: string): Result<FileStat, FsError>;
  readdir(path: string): Result<DirEntry[], FsError>;
  mkdir(path: string): Result<void, FsError>;
  unlink(path: string): Result<void, FsError>;
  rename(oldPath: string, newPath: string): Result<void, FsError>;
}

type OpenFlags = {
  read: boolean;
  write: boolean;
  create: boolean;
  truncate: boolean;
  append: boolean;
};

interface FileStat {
  size: number;
  isDirectory: boolean;
  modifiedAt: number;  // Unix timestamp
}

interface DirEntry {
  name: string;
  isDirectory: boolean;
}

type FsError =
  | 'NOT_FOUND'
  | 'PERMISSION_DENIED'
  | 'QUOTA_EXCEEDED'
  | 'TOO_MANY_OPEN_FILES'
  | 'INVALID_PATH'
  | 'IS_DIRECTORY'
  | 'NOT_DIRECTORY'
  | 'NOT_EMPTY'
  | 'IO_ERROR';
```

## Memory Management

### Memory Limits

| Resource | Default | Range |
|----------|---------|-------|
| Initial memory | 1 MB | 1-64 MB |
| Maximum memory | 64 MB | 1-256 MB |
| Memory growth | 1 MB increments | - |
| Stack size | 1 MB | Fixed |

### Memory Configuration

```typescript
interface MemoryConfig {
  initialPages: number;     // WASM pages (64KB each)
  maximumPages: number;     // Upper limit
  sharedMemory: boolean;    // false (single-threaded)
}

// Default configuration
const DEFAULT_MEMORY: MemoryConfig = {
  initialPages: 16,         // 1 MB
  maximumPages: 1024,       // 64 MB
  sharedMemory: false,
};
```

### Out-of-Memory Handling

When memory allocation fails:
1. WASM `memory.grow` returns -1
2. App should handle gracefully
3. Host may terminate app if it traps on OOM

## Execution Limits (Fuel)

Fuel metering prevents infinite loops and ensures fair resource sharing.

### Fuel Model

```typescript
interface FuelConfig {
  // Fuel per invocation type
  invocationFuel: {
    // User-triggered actions
    userAction: number;      // e.g., button click handler
    // Background tasks
    backgroundTask: number;  // e.g., sync callback
    // Startup
    appStart: number;        // initial startup
  };

  // Fuel costs (approximate)
  costs: {
    wasmInstruction: number;   // Per WASM instruction
    hostCallBase: number;      // Base cost for any host call
    hostCallStorage: number;   // Additional for storage ops
    hostCallMessaging: number; // Additional for messaging ops
  };

  // Refill rate
  refillRate: number;        // Fuel per second
  maxFuel: number;           // Cap on accumulated fuel
}

// Default configuration
const DEFAULT_FUEL: FuelConfig = {
  invocationFuel: {
    userAction: 1_000_000,     // 1M fuel
    backgroundTask: 100_000,   // 100K fuel
    appStart: 10_000_000,      // 10M fuel
  },
  costs: {
    wasmInstruction: 1,
    hostCallBase: 100,
    hostCallStorage: 1000,
    hostCallMessaging: 5000,
  },
  refillRate: 100_000,        // 100K/second
  maxFuel: 100_000_000,       // 100M max
};
```

### Fuel Exhaustion

When fuel runs out:
1. WASM execution is interrupted
2. `FuelExhausted` error returned to host
3. App state is preserved (transaction-safe)
4. Fuel refills over time

## Instance Lifecycle

### States

```
┌─────────────┐
│  INACTIVE   │ ← Initial state, app not loaded
└──────┬──────┘
       │ load()
       ▼
┌─────────────┐
│   LOADING   │ ← WASM module being instantiated
└──────┬──────┘
       │ success
       ▼
┌─────────────┐
│    READY    │ ← App ready to receive invocations
└──────┬──────┘
       │ invoke()
       ▼
┌─────────────┐
│   RUNNING   │ ← Executing app code
└──────┬──────┘
       │ complete/error
       ▼
┌─────────────┐
│    READY    │ ← Returns to ready state
└─────────────┘
       │ unload() or timeout
       ▼
┌─────────────┐
│  INACTIVE   │
└─────────────┘
```

### Instance Management

```typescript
interface AppInstance {
  appId: string;
  state: InstanceState;
  fuelRemaining: number;
  memoryUsed: number;
  lastInvocation: Timestamp;
}

type InstanceState =
  | 'INACTIVE'
  | 'LOADING'
  | 'READY'
  | 'RUNNING';

interface InstanceManager {
  // Load app into memory
  load(appId: string): Promise<Result<void, LoadError>>;

  // Invoke app entry point
  invoke(
    appId: string,
    entryPoint: string,
    args: Uint8Array,
    fuel: number
  ): Promise<Result<Uint8Array, InvokeError>>;

  // Unload app (free memory)
  unload(appId: string): Promise<void>;

  // Get instance info
  getInfo(appId: string): AppInstance | null;

  // List all instances
  listInstances(): AppInstance[];
}

type LoadError =
  | 'APP_NOT_FOUND'
  | 'INVALID_WASM'
  | 'MEMORY_LIMIT'
  | 'ALREADY_LOADED';

type InvokeError =
  | 'NOT_LOADED'
  | 'ENTRY_NOT_FOUND'
  | 'FUEL_EXHAUSTED'
  | 'TRAPPED'
  | 'PERMISSION_DENIED'
  | 'TIMEOUT';
```

### Instance Timeout

Idle instances are unloaded to free resources:

| Condition | Action |
|-----------|--------|
| No invocation for 5 minutes | Unload instance |
| Memory pressure | Unload least-recently-used |
| App explicitly requests | Unload immediately |

## Module Validation

Before loading, WASM modules are validated:

### Required Validations

1. **Signature verification**: Module signed by trusted publisher
2. **Size limits**: Module < 10 MB (configurable)
3. **WASM validation**: Valid WebAssembly binary
4. **Import validation**: Only uses allowed imports
5. **Export validation**: Has required entry points

### Required Exports

Apps must export these functions. See `abi.md` for the authoritative specification.

```wasm
;; Initialize the app (called once on load)
(export "_start" (func $start))

;; Handle invocation (called per user action, background, message)
;; Returns packed i64: (ptr << 32) | len
(export "handle" (func $handle (param i32 i32) (result i64)))

;; Get last error message (optional)
;; Returns packed i64: (ptr << 32) | len, or 0 if no error
(export "get_error" (func $get_error (result i64)))

;; Memory for host-app communication
(export "memory" (memory $mem))

;; Allocate memory for host to write into
(export "alloc" (func $alloc (param i32) (result i32)))

;; Free memory allocated for host
(export "dealloc" (func $dealloc (param i32 i32)))
```

### Host Imports

Apps import functions from the `host` module. See `abi.md` for the authoritative specification.

```wasm
;; Host API module - see abi.md for complete signatures
(import "host" "call" (func $host_call (param i32 i32 i32 i32) (result i32)))
(import "host" "get_result" (func $host_get_result (param i32 i32 i32) (result i32)))
(import "host" "get_result_len" (func $host_get_result_len (param i32) (result i32)))
(import "host" "poll" (func $host_poll (param i32) (result i32)))
(import "host" "log" (func $host_log (param i32 i32 i32)))
```

## Determinism Guarantees

### Deterministic Operations

| Operation | Guarantee |
|-----------|-----------|
| WASM execution | Fully deterministic |
| Memory operations | Deterministic |
| Virtual filesystem reads | Deterministic (for same state) |
| Host API results | Deterministic (for same inputs) |

### Non-Deterministic Operations

| Operation | Handling |
|-----------|----------|
| `system.get_time` | Returns wall clock (non-deterministic) |
| `system.get_random` | Returns cryptographic randomness (non-deterministic) |
| Storage writes | Order preserved within transaction |
| Messaging | Delivery order not guaranteed |

### Randomness APIs

The Host API provides two randomness sources to support both security and reproducibility:

| Method | Capability | Deterministic | Use Case |
|--------|------------|---------------|----------|
| `system.get_random` | `system:random` | No | Key generation, nonces, crypto |
| `system.get_deterministic_random` | None | Yes | Testing, simulations, games |

**`system.get_deterministic_random`:**
```typescript
interface DeterministicRandomRequest {
  seed?: Uint8Array;  // Optional seed (default: invocation-derived)
  length: number;     // Bytes to generate (max 1024)
}
```

The deterministic variant uses a PRNG seeded from:
1. Explicit seed if provided, OR
2. Hash of (app_id, invocation_id) for implicit per-invocation seed

**WARNING:** Never use `system.get_deterministic_random` for security-sensitive operations.

### Reproducibility

For debugging/auditing:
1. Capture invocation inputs
2. Ensure app uses `system.get_deterministic_random` (not `system.get_random`)
3. Replay with same inputs
4. Verify identical outputs

Note: Apps using `system.get_random` or `system.get_time` cannot be reproduced deterministically.

## Error Handling

### WASM Traps

| Trap | Host Response |
|------|---------------|
| Unreachable | Return `TRAPPED`, preserve state |
| Memory out of bounds | Return `TRAPPED`, preserve state |
| Integer divide by zero | Return `TRAPPED`, preserve state |
| Stack overflow | Return `TRAPPED`, preserve state |
| Table out of bounds | Return `TRAPPED`, preserve state |

### Recovery

After a trap:
1. Instance state is preserved (no corrupted state)
2. In-progress operations are rolled back
3. App can be reinvoked after fuel refills
4. Persistent errors may trigger app disable

## Security Hardening

### Sandbox Boundaries

```
┌────────────────────────────────────────┐
│  WASM Sandbox (per app)                │
│  ┌──────────────────────────────────┐  │
│  │  Linear Memory (isolated)        │  │
│  │  ┌────────────────────────────┐  │  │
│  │  │  App Code + Data           │  │  │
│  │  └────────────────────────────┘  │  │
│  └──────────────────────────────────┘  │
│                                        │
│  Virtual Filesystem (per app)          │
│  Host API (capability-gated)           │
└────────────────────────────────────────┘
                │
                │ Host API calls only
                ▼
┌────────────────────────────────────────┐
│  Host Process (shared)                 │
│  - Capability enforcement              │
│  - Resource accounting                 │
│  - Inter-app isolation                 │
└────────────────────────────────────────┘
```

### Attack Mitigations

| Attack | Mitigation |
|--------|------------|
| Spectre/Meltdown | WASM memory model prevents exploitation |
| Buffer overflow | WASM bounds checking |
| ROP/JOP | WASM control flow integrity |
| Timing attacks | Fuel metering normalizes timing |
| Covert channels | Limited (shared host resources) |
