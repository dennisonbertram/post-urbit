# Post-Urbit

Post-Urbit is a personal computing node for a user-owned, decentralized internet.
It pairs a Rust backend with a System 7-style web shell built in React/TypeScript.

![Post-Urbit Shell](post-urbit-core/screenshots/shell.png)

## What it is

- Self-sovereign identity (keys you control)
- End-to-end encrypted messaging
- Peer-to-peer networking (QUIC + mailbox for offline delivery)
- WASM app sandbox with explicit permissions
- Desktop-style shell in your browser

## Vision

A network where anyone can run a personal node that provides:

- **Identity you control** — portable, recoverable, revocable
- **Secure communication** — 1:1 and group messaging
- **Local-first applications** — your data lives with you; sync is selective
- **Interoperation across nodes** — without a central owner

### Design Principles

| Principle | Meaning |
|-----------|---------|
| Identity ≠ routing ≠ governance | Separate concerns with separate solutions |
| No permanent root keys | No single point of compromise or control |
| Exit is always possible | Portable identity + portable data |
| Scarcity maps to real resources | Bandwidth, storage, compute — not artificial tokens |
| Composable trust | Users choose trust providers; no mandatory "lords" |

### Architecture

```
┌─────────────────────────────────────────┐
│  6. Governance                          │
├─────────────────────────────────────────┤
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

## Quick Start

Prerequisites: Rust 1.70+ and Node.js 18+

**Backend:**
```bash
cd post-urbit-core
cargo run -- run --admin-password-hash '<argon2id_hash>'
```

**Frontend:**
```bash
cd post-urbit-core/packages/shell
npm install
npm run dev
```

Open `http://localhost:5173`. The frontend proxies API calls to the backend.

**Development mode** (bypasses auth, local only):
```bash
cd post-urbit-core
cargo run -- run --dev
```

**Ports:**
- Frontend: `http://localhost:5173`
- HTTP API: `http://localhost:8080`
- QUIC transport: UDP `4433`

## Repository Structure

```
post-urbit/
├── post-urbit-core/          # Main implementation
│   ├── src/                  # Rust backend source
│   │   ├── main.rs           # CLI entry point
│   │   ├── node.rs           # Node initialization & runtime
│   │   ├── node_http.rs      # HTTP API server
│   │   ├── identity.rs       # Identity management
│   │   ├── transport.rs      # QUIC networking
│   │   └── ...
│   ├── packages/
│   │   └── shell/            # React/TypeScript frontend
│   │       ├── src/
│   │       │   ├── components/   # UI components
│   │       │   ├── api/          # API hooks & types
│   │       │   └── context/      # React contexts
│   │       └── ...
│   ├── docs/                 # Technical documentation
│   └── screenshots/          # UI screenshots
│
├── spec/                     # Specifications & design documents
│   ├── 00-overview/          # Project overview & architecture
│   ├── 01-transport-connectivity/
│   ├── 02-identity-trust/
│   ├── 03-messaging-sync/
│   ├── 04-app-runtime/
│   ├── 05-ux-packaging/
│   ├── 06-rfcs/
│   └── progress.md           # Implementation progress tracking
│
└── post-urbit-spikes/        # Experimental prototypes
```

## Documentation

- [Introduction](post-urbit-core/docs/introduction.md)
- [HTTP API Reference](post-urbit-core/docs/api/http-api.md)
- [Building Apps](post-urbit-core/docs/apps/building-apps.md)
- [Identity System](post-urbit-core/docs/identity.md)
- [Messaging Protocol](post-urbit-core/docs/messaging.md)
- [Transport Layer](post-urbit-core/docs/transport.md)
- [Mailbox & DHT](post-urbit-core/docs/mailbox-and-dht.md)
- [Visual Design Spec](post-urbit-core/docs/specs/10-VISUAL_DESIGN.md)

For CLI options: `cargo run -- --help` from post-urbit-core.

## Built with LLMs

Post-Urbit is an experiment in AI-assisted development. The entire codebase — Rust backend, React frontend, specifications, and documentation — was developed collaboratively with Claude (Anthropic's AI assistant).

This includes:
- **Architecture design** — Layer definitions, protocol choices, security model
- **Implementation** — All Rust and TypeScript code, from transport layer to UI components
- **Code review** — Using external LLM review (GPT-5.2) piped through the codebase
- **Debugging** — Identifying and fixing race conditions, auth issues, UI bugs
- **Documentation** — API docs, specs, and this README

The development workflow uses [Claude Code](https://claude.ai/claude-code) with browser automation, allowing the AI to:
- Read and write code across the full stack
- Run builds and tests
- Take screenshots and iterate on UI
- Commit and push to git

This project demonstrates what's possible when treating AI as a collaborative development partner rather than just a code completion tool.

## Contributing

See [CONTRIBUTING.md](post-urbit-core/CONTRIBUTING.md) for development guidelines.
