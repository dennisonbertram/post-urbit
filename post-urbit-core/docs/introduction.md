# What is Post-Urbit?

Post-Urbit is your personal server for the decentralized internet. It's a piece of software that runs on hardware you control—a laptop, a Raspberry Pi, a cloud VM—and becomes your permanent digital home.

## The Problem

Today, your digital life is scattered across dozens of companies:

- Your messages live on WhatsApp's servers
- Your photos live on Google's servers
- Your documents live on Dropbox's servers
- Your social graph lives on Facebook's servers
- Your identity is verified by Apple, Google, or your government

You don't own any of it. You rent access to your own data. These companies can:

- Read your messages (even "encrypted" ones often aren't)
- Lock you out of your account
- Sell your data to advertisers
- Shut down and take your data with them
- Be compelled by governments to hand over your information
- Change their terms of service whenever they want

Every app you use creates another silo. Another login. Another company with a copy of your data. Another point of failure.

## The Solution

Post-Urbit inverts this model. Instead of your data living on many servers owned by others, it lives on one server owned by you.

```
Traditional Model:                    Post-Urbit Model:

   You                                    You
    │                                      │
    ├── Google (email, docs, photos)       └── Your Node
    ├── Facebook (social, messages)             │
    ├── Dropbox (files)                         ├── Messages
    ├── Slack (work chat)                       ├── Files
    ├── Twitter (posts)                         ├── Contacts
    ├── Bank (finances)                         ├── Social
    └── ... (50 more services)                  ├── Apps
                                                └── Everything
    = 50 companies have your data
    = 50 points of failure               = You have your data
    = 50 terms of service                = One system
    = No interoperability                = Full interoperability
```

## What Post-Urbit Does

### 1. Gives You a Permanent Identity

Your Post-Urbit identity (IID) is derived from cryptographic keys that you generate and control. No company issues it. No government grants it. It's yours by virtue of mathematics.

```
Your IID: b1n7cfscgashm32xx7eaxw0y09gy0y2v
         └── Derived from your keys
             └── Which you control
                 └── Forever
```

This identity persists across devices, across apps, across time. When you message someone, they know it's you—not because Google verified your phone number, but because only you possess the private key that signed that message.

### 2. Encrypts Everything End-to-End

Every message between Post-Urbit nodes is encrypted using the Signal protocol (double ratchet). Not even your own node can be compelled to reveal message contents without the recipient's keys.

- **Forward secrecy**: Compromising today's keys doesn't reveal yesterday's messages
- **Break-in recovery**: Compromising today's keys doesn't compromise tomorrow's messages
- **No metadata leakage**: Messages route directly between nodes

### 3. Connects You Peer-to-Peer

Post-Urbit nodes connect directly to each other over QUIC (a modern transport protocol). There are no central servers routing your traffic. When you're online, you communicate directly. When you're offline, messages wait in your contacts' mailboxes until you return.

```
Alice's Node ←────────────────→ Bob's Node
             Direct Connection
             No Intermediary
```

### 4. Runs Your Applications

Post-Urbit isn't just infrastructure—it's a platform. Applications run as WebAssembly modules inside your node's sandbox. These apps use your data through controlled APIs:

| Capability | What It Enables |
|------------|-----------------|
| `storage` | Apps can store and retrieve data |
| `messaging` | Apps can send/receive messages |
| `contacts` | Apps can see your contact list |
| `sync` | Apps can sync data across your devices |
| `notifications` | Apps can alert you |

Apps request capabilities. You grant them. They can only do what you allow.

### 5. Syncs Across Your Devices

Your Post-Urbit identity can span multiple devices—your phone, laptop, home server. Data syncs between them using CRDTs (Conflict-free Replicated Data Types), so edits merge automatically even when made offline.

```
Your Phone ←──────→ Your Laptop
     ↑                   ↑
     └───────→ Your Server ←───────┘
              (always-on)
```

## What Post-Urbit Enables

### Communication Without Platforms

Build a chat app that works like Signal but runs on your own infrastructure. Your messages, your servers, your rules. No company can read them, ban you, or shut down the service.

### Social Without Surveillance

Build a social network where posts flow directly between people who follow each other. No algorithmic feed optimizing for engagement. No ads. No data harvesting. Just people sharing with people.

### Collaboration Without SaaS

Build document editors, project management tools, or wikis that sync between team members without uploading everything to someone else's cloud.

### Commerce Without Intermediaries

Build payment requests, invoices, and receipts that flow directly between parties. Split expenses with friends without Venmo taking a cut or knowing your spending habits.

### Personal AI Without Data Harvesting

Run AI assistants that have full access to your data—your messages, calendar, files—without sending any of it to OpenAI or Google. Your AI, trained on your data, running on your hardware.

### The Apps You Actually Want

| App Type | Without Post-Urbit | With Post-Urbit |
|----------|-------------------|-----------------|
| **Notes** | Notion has your data | Your data stays local |
| **Chat** | WhatsApp reads metadata | True E2E, no intermediary |
| **Calendar** | Google sees your schedule | Private, synced across devices |
| **Photos** | iCloud has your memories | Your photos, your storage |
| **Passwords** | 1Password is a target | Local vault, synced securely |
| **Social** | Twitter controls your reach | Direct to followers |

## How It Works (Simplified)

1. **You run a node** on hardware you control
2. **You get an identity** derived from keys the node generates
3. **You add contacts** by exchanging identities with other node operators
4. **You install apps** that run in a sandbox on your node
5. **Apps use your data** through capability-controlled APIs
6. **Data syncs** across your devices via CRDT replication
7. **Messages flow** directly between nodes, encrypted end-to-end

## The Vision

Imagine a world where:

- Your digital identity is as permanent as your physical existence
- Your data follows you, not the other way around
- Apps compete on features, not network effects
- Communication happens directly, not through intermediaries
- You can leave any service instantly, taking everything with you
- Your children inherit your digital estate, not "your account has been memorialized"

This is what Post-Urbit enables. Not by asking companies to be nicer, but by making them unnecessary.

## Getting Started

```bash
# Build the node
cargo build --release

# Initialize with a data directory
./target/release/post-urbit-core --data-dir ~/my-node

# Your node is now running
# Access the API at http://localhost:4433
```

Your node generates an identity on first run. From there, you can:

1. Add contacts by exchanging IIDs
2. Install apps from repositories
3. Start communicating directly

See the [HTTP API Reference](./api/http-api.md) for details.

## Comparison

| | Centralized Services | Federated (Matrix, Mastodon) | Post-Urbit |
|---|---------------------|------------------------------|------------|
| **Who holds data?** | Company | Server operator | You |
| **Identity** | Platform-issued | Server-issued | Self-sovereign |
| **Encryption** | Maybe | Optional | Always, E2E |
| **Interoperability** | None | Protocol-level | Protocol-level |
| **Apps** | Platform builds | Limited | Open ecosystem |
| **Exit cost** | High (lose data) | Medium (can migrate) | Zero (you have it all) |

## Philosophy

Post-Urbit is built on a few core beliefs:

1. **Ownership is non-negotiable.** If you don't control your keys, you don't control your data.

2. **Simplicity enables adoption.** Running a node should be as easy as running an app.

3. **Privacy is the default.** Encryption and local-first storage shouldn't be opt-in.

4. **Interoperability breaks silos.** Open protocols beat walled gardens.

5. **Software should be infrastructure.** Apps are temporary; your data is permanent.

## Learn More

- [Identity System](./identity.md) - How self-sovereign identity works
- [Transport Layer](./transport.md) - How nodes connect securely
- [Messaging Protocol](./messaging.md) - How E2E encryption works
- [Building Apps](./apps/building-apps.md) - How to build on Post-Urbit
- [HTTP API](./api/http-api.md) - How to interact with your node

---

*Post-Urbit: Your server. Your data. Your rules.*
