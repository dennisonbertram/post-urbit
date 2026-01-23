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

## Quick start

Prereqs: Rust 1.70+ and Node.js 18+

Backend:
```bash
cd post-urbit-core
cargo run -- run --admin-password-hash '<argon2id_hash>'
```

Frontend:
```bash
cd post-urbit-core/packages/shell
npm install
npm run dev
```

Open `http://localhost:5173`. The frontend proxies API calls to the backend.

Notes:
- HTTP API: `http://localhost:8080`
- QUIC transport: UDP port `4433` by default
- Sessions are cookie-based with CSRF protection

Development mode (insecure, local only):
```bash
cd post-urbit-core
cargo run -- run --dev
```

## Repository Structure

```
post-urbit/
├── post-urbit-core/     # Main implementation
│   ├── src/             # Rust backend
│   ├── packages/shell/  # React frontend
│   └── docs/            # Documentation
├── spec/                # Specifications and design docs
└── post-urbit-spikes/   # Experimental prototypes
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

For CLI options, run `cargo run -- --help` from post-urbit-core.

## Contributing

See [CONTRIBUTING.md](post-urbit-core/CONTRIBUTING.md) for development guidelines.
