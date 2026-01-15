# UX & Packaging Layer Overview

## Purpose

The UX & Packaging layer provides the operational foundation for running a personal node. It covers:

- **Node daemon**: The long-running process that hosts identity, messaging, sync, and app runtime
- **Admin UI**: Web-based interface for node configuration and management
- **App packaging**: Format, signing, and distribution of applications
- **Deployment**: Installation paths for different environments
- **Observability**: Logging, metrics, and health monitoring

## Design Principles

### Self-Hostable First

The node must be deployable by technically-capable individuals on:
- Personal hardware (Raspberry Pi, NUC, old laptop)
- VPS/cloud instances
- Containerized environments (Docker, Kubernetes)
- Managed hosting (for less technical users)

### Progressive Complexity

Simple operations should be simple. Advanced features should be discoverable but not required:
- **Minimal config**: Works out-of-box with sensible defaults
- **Full control**: Every setting is configurable for power users
- **No hidden magic**: All operations are explainable and auditable

### Security by Default

Default configurations prioritize security over convenience:
- HTTPS for all web interfaces
- Authentication required for admin access
- Conservative permissions on new apps
- Automatic security updates enabled by default

### Local-First Operation

The node operates independently when network is unavailable:
- All data persisted locally
- Operations queue for later sync
- No cloud dependencies for core functionality

## Component Files

| File | Purpose |
|------|---------|
| `node-daemon.md` | Daemon architecture, lifecycle, configuration |
| `admin-ui.md` | Web interface specification, API surface |
| `app-distribution.md` | Package format, signing, repositories |
| `deployment.md` | Installation guides for different platforms |
| `observability.md` | Logging, metrics, health checks, alerting |
| `interfaces.md` | TypeScript interface definitions |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         User Interfaces                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
│  │  Admin UI   │  │   CLI Tool  │  │  Apps (Web) │  │  Mobile App │ │
│  │  (React)    │  │  (Terminal) │  │  (Browser)  │  │  (Native)   │ │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘ │
│         │                │                │                │         │
├─────────▼────────────────▼────────────────▼────────────────▼─────────┤
│                       Node HTTP API                                   │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  /admin/*   │   /apps/*    │   /api/*    │  /health  │  /metrics │ │
│  └─────────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│                        Node Daemon                                    │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  Identity │ Transport │ Messaging │ Sync │ App Runtime │ Storage │ │
│  └─────────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│                    Platform Services                                  │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  Filesystem │ Networking │ Process Mgmt │ Crypto (TPM/Keychain) │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

## Node HTTP API

The daemon exposes an HTTP API for local management and remote access (when enabled).

### Endpoint Categories

| Path Prefix | Purpose | Authentication |
|-------------|---------|----------------|
| `/admin/*` | Node configuration, app management | Session cookie (browser) or Bearer token (CLI) |
| `/apps/*` | Per-app web UI serving | Session or app token |
| `/api/v1/*` | Programmatic API for external tools | API key or session |
| `/health` | Health check endpoint | None (rate limited) |
| `/metrics` | Prometheus metrics | Optional (configurable) |

### Authentication Methods

| Method | Use Case | Details |
|--------|----------|---------|
| Session cookie | Browser-based Admin UI | HttpOnly cookie + CSRF protection |
| Admin token | CLI tools, automation | Bearer header, no session |
| API key | External integrations | Bearer header, scoped permissions |
| App token | Per-app delegated access | Scoped to specific app |

See `node-daemon.md` § Authentication for the complete specification including CSRF protection and session management.

## App Distribution Model

### Package Format

Applications are distributed as signed `.postapp` packages:

```
myapp-1.0.0.postapp
├── manifest.json      # App metadata, capabilities (see 04-app-runtime)
├── main.wasm          # Compiled WebAssembly module
├── assets/            # Static assets (icons, images)
├── ui/                # Optional web UI files
└── SIGNATURE          # Ed25519 signature by author
```

### Signing Model

```
Package signing:
1. Author creates manifest.json with file_hashes
2. Author signs SHA256(canonical_manifest_json) with their identity signing key
3. SIGNATURE file contains: author_iid + signature + timestamp

Package verification:
1. Verify SIGNATURE using author's identity document (from DHT or local cache)
2. Verify all file hashes match manifest.file_hashes
3. Check author_iid against any blocklists
4. Optionally verify against trusted publisher list
```

### Distribution Channels

| Channel | Trust Level | Use Case |
|---------|-------------|----------|
| **Direct install** | User verifies author | Installing from known developer |
| **Repository** | Repository curates | Community app store |
| **Enterprise** | Org controls policy | Corporate deployments |
| **Sideload** | User accepts risk | Development, testing |

## Dependencies

| Dependency | Provider | Usage |
|------------|----------|-------|
| Identity | 02-identity-trust | Author verification, node identity |
| Transport | 01-transport-connectivity | P2P communication |
| Messaging | 03-messaging-sync | Notifications, sync |
| App Runtime | 04-app-runtime | WASM execution, capabilities |

## Security Model

### Node Security

| Threat | Mitigation |
|--------|------------|
| Unauthorized admin access | Admin token + TLS + optional IP allowlist |
| App escapes sandbox | WASM isolation, capability enforcement |
| Malicious app package | Signature verification, capability review |
| Supply chain attack | Content-addressed packages, author verification |
| Data exfiltration | No network access for apps, messaging requires capability |

### Admin UI Security

| Threat | Mitigation |
|--------|------------|
| XSS | CSP headers, React auto-escaping |
| CSRF | SameSite cookies, CSRF tokens |
| Session hijacking | Secure cookies, short sessions, re-auth for sensitive ops |
| Brute force | Rate limiting, account lockout |

## Performance Targets

| Metric | Target |
|--------|--------|
| Node cold start | < 5 seconds |
| Admin UI load | < 2 seconds |
| App install | < 10 seconds (typical package) |
| Health check response | < 100ms |
| Memory baseline | < 128MB (no apps loaded) |

## Configuration Hierarchy

Settings are loaded in order (later overrides earlier):

1. **Built-in defaults**: Sensible out-of-box values
2. **Config file**: `~/.postnode/config.toml` or `/etc/postnode/config.toml`
3. **Environment variables**: `POSTNODE_*` prefix
4. **Command-line flags**: For one-off overrides

### Config File Format

```toml
[node]
data_dir = "~/.postnode/data"
log_level = "info"

[identity]
# IID is derived from genesis key, not configured
auto_publish = true
publish_interval_hours = 24

[network]
listen_addr = "0.0.0.0:4433"
admin_listen_addr = "127.0.0.1:8080"
enable_upnp = true
relay_servers = ["relay.example.com:4433"]

[admin]
enabled = true
require_tls = true
session_timeout_hours = 24

[apps]
auto_update = true
allow_sideload = false
storage_quota_default = "100mb"

[security]
min_tls_version = "1.3"
ip_allowlist = []  # Empty = allow all local

[observability]
metrics_enabled = true
metrics_path = "/metrics"
log_format = "json"  # or "text"
```
