# Building Post-Urbit Apps

Post-Urbit apps are WebAssembly modules that run in a sandboxed environment on your personal node. This guide walks through building a simple Notes app.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                     Your Node                           │
│  ┌───────────────────────────────────────────────────┐  │
│  │                 WASM Sandbox                      │  │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐           │  │
│  │  │ Notes   │  │  Chat   │  │ Calendar│  ...      │  │
│  │  │  App    │  │   App   │  │   App   │           │  │
│  │  └────┬────┘  └────┬────┘  └────┬────┘           │  │
│  │       │            │            │                 │  │
│  └───────┼────────────┼────────────┼─────────────────┘  │
│          │            │            │                    │
│  ┌───────▼────────────▼────────────▼─────────────────┐  │
│  │              Host API Layer                        │  │
│  │  storage | messaging | contacts | sync | notify   │  │
│  └───────────────────────────────────────────────────┘  │
│          │                                              │
│  ┌───────▼───────────────────────────────────────────┐  │
│  │              Node Core Services                    │  │
│  │  DHT | QUIC Transport | Identity | Mailbox        │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

Apps cannot access the filesystem, network, or system directly. They can only use the capabilities granted to them through the Host API.

## Package Structure

A Post-Urbit app is distributed as a `.postapp` file (a ZIP archive):

```
notes.postapp
├── manifest.json        # Required: App metadata and permissions
├── signature.json       # Required: Cryptographic signature
├── main.wasm           # Required: WASM binary (max 50 MB)
└── ui/                  # Optional: Web UI assets (max 20 MB total)
    ├── index.html
    ├── app.js
    └── style.css
```

## Example: Notes App

### manifest.json

```json
{
  "manifest_version": 1,
  "app": {
    "id": "com.example.notes",
    "name": "Notes",
    "version": "1.0.0",
    "description": "A simple note-taking app with sync",
    "author": {
      "name": "Your Name",
      "iid": "your-iid-here",
      "url": "https://example.com"
    },
    "license": "MIT",
    "homepage": "https://github.com/example/notes",
    "repository": "https://github.com/example/notes"
  },
  "runtime": {
    "entry": "main.wasm",
    "memory": {
      "initial_pages": 16,
      "maximum_pages": 256
    },
    "fuel": {
      "user_action": 1000000,
      "background_task": 100000,
      "app_start": 500000
    }
  },
  "capabilities": {
    "required": ["storage"],
    "optional": ["sync", "messaging"],
    "reasons": {
      "storage": "Store your notes locally on your node",
      "sync": "Sync notes across your devices",
      "messaging": "Share notes with contacts"
    }
  },
  "dependencies": {
    "api_version": "1.0",
    "node_version": ">=0.1.0"
  },
  "files": {
    "hashes": {
      "main.wasm": "sha256:abc123...",
      "ui/index.html": "sha256:def456..."
    },
    "total_size": 102400
  }
}
```

### App Code (Rust → WASM)

Here's a minimal notes app in Rust that compiles to WASM:

```rust
// src/lib.rs
use postapp_sdk::prelude::*;

#[derive(Serialize, Deserialize)]
struct Note {
    id: String,
    title: String,
    content: String,
    created_at: u64,
    updated_at: u64,
}

// Called when the app starts
#[postapp::init]
fn init() {
    log("Notes app initialized");
}

// Create a new note
#[postapp::export]
fn create_note(title: String, content: String) -> Result<Note, Error> {
    let note = Note {
        id: generate_id(),
        title,
        content,
        created_at: now(),
        updated_at: now(),
    };

    // Store in app's key-value storage
    storage::set(&format!("note:{}", note.id), &note)?;

    // Add to index
    let mut index: Vec<String> = storage::get("note_index").unwrap_or_default();
    index.push(note.id.clone());
    storage::set("note_index", &index)?;

    Ok(note)
}

// List all notes
#[postapp::export]
fn list_notes() -> Result<Vec<Note>, Error> {
    let index: Vec<String> = storage::get("note_index").unwrap_or_default();
    let mut notes = Vec::new();

    for id in index {
        if let Some(note) = storage::get::<Note>(&format!("note:{}", id))? {
            notes.push(note);
        }
    }

    // Sort by updated_at descending
    notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(notes)
}

// Get a single note
#[postapp::export]
fn get_note(id: String) -> Result<Option<Note>, Error> {
    storage::get(&format!("note:{}", id))
}

// Update a note
#[postapp::export]
fn update_note(id: String, title: String, content: String) -> Result<Note, Error> {
    let key = format!("note:{}", id);
    let mut note: Note = storage::get(&key)?
        .ok_or(Error::NotFound)?;

    note.title = title;
    note.content = content;
    note.updated_at = now();

    storage::set(&key, &note)?;
    Ok(note)
}

// Delete a note
#[postapp::export]
fn delete_note(id: String) -> Result<(), Error> {
    storage::delete(&format!("note:{}", id))?;

    let mut index: Vec<String> = storage::get("note_index").unwrap_or_default();
    index.retain(|i| i != &id);
    storage::set("note_index", &index)?;

    Ok(())
}

// Share a note with a contact (requires messaging capability)
#[postapp::export]
fn share_note(id: String, recipient_iid: String) -> Result<(), Error> {
    let note: Note = storage::get(&format!("note:{}", id))?
        .ok_or(Error::NotFound)?;

    // Send as a message
    messaging::send(
        &recipient_iid,
        "application/x-notes-share",
        &serde_json::to_vec(&note)?,
    )?;

    Ok(())
}

// Handle incoming shared notes
#[postapp::on_message(content_type = "application/x-notes-share")]
fn handle_shared_note(sender: &str, payload: &[u8]) -> Result<(), Error> {
    let note: Note = serde_json::from_slice(payload)?;

    // Store with a different prefix to distinguish shared notes
    storage::set(&format!("shared:{}:{}", sender, note.id), &note)?;

    notify::show(&format!("{} shared a note: {}", sender, note.title))?;

    Ok(())
}
```

### Building the WASM

```bash
# Install the target
rustup target add wasm32-unknown-unknown

# Build
cargo build --target wasm32-unknown-unknown --release

# The output is at target/wasm32-unknown-unknown/release/notes.wasm
```

### Packaging

```bash
# Create the package structure
mkdir -p package/ui
cp target/wasm32-unknown-unknown/release/notes.wasm package/main.wasm
cp manifest.json package/
cp -r ui/* package/ui/

# Sign the manifest (using your node's identity)
curl -X POST http://localhost:4433/api/v1/apps/sign-manifest \
  -H "Authorization: Bearer $TOKEN" \
  -d @package/manifest.json > package/signature.json

# Create the .postapp file
cd package && zip -r ../notes.postapp . && cd ..
```

## Host API Reference

### Storage API

Apps get isolated key-value storage. Keys are scoped to the app.

```rust
// Set a value (JSON serialized)
storage::set("key", &value)?;

// Get a value
let value: Option<T> = storage::get("key")?;

// Delete a key
storage::delete("key")?;

// List keys by prefix
let keys: Vec<String> = storage::list("prefix:")?;

// Atomic batch operations
storage::batch(|tx| {
    tx.set("key1", &val1)?;
    tx.set("key2", &val2)?;
    tx.delete("key3")?;
    Ok(())
})?;
```

### Messaging API

Send and receive messages to/from other Post-Urbit nodes.

```rust
// Send a message
messaging::send(recipient_iid, content_type, payload)?;

// Register a message handler (via attribute macro)
#[postapp::on_message(content_type = "application/x-myapp")]
fn handle(sender: &str, payload: &[u8]) -> Result<(), Error> {
    // Process incoming message
}
```

### Contacts API

Access the user's contact list (with permission).

```rust
// List contacts
let contacts: Vec<Contact> = contacts::list()?;

// Get a specific contact
let contact: Option<Contact> = contacts::get(iid)?;

// Check if someone is a contact
let is_contact: bool = contacts::is_contact(iid)?;
```

### Sync API

CRDT-based data synchronization across devices.

```rust
// Register a CRDT document
sync::register("notes", CrdtType::LWWMap)?;

// Update (automatically syncs)
sync::update("notes", |doc| {
    doc.set("key", value);
})?;

// Subscribe to remote changes
#[postapp::on_sync(document = "notes")]
fn handle_sync(changes: &Changes) {
    // React to changes from other devices
}
```

### Notifications API

Show notifications to the user.

```rust
// Simple notification
notify::show("You have a new message")?;

// Rich notification
notify::show_rich(Notification {
    title: "New Note Shared",
    body: "Alice shared 'Meeting Notes' with you",
    icon: "note",
    actions: vec![
        Action { id: "view", label: "View" },
        Action { id: "dismiss", label: "Dismiss" },
    ],
})?;
```

### Network API

Make HTTP requests to external APIs. Network access requires explicit capabilities.

**Declaring Network Capabilities:**

In your `manifest.json`, declare which domains your app needs:

```json
{
  "capabilities": {
    "required": [
      "network:https:api.example.com",
      "network:https:*.openweathermap.org"
    ]
  },
  "secrets": {
    "api_key": {
      "description": "API key for example.com",
      "required": true,
      "inject": {
        "domains": ["api.example.com"],
        "header": "Authorization",
        "header_prefix": "Bearer "
      }
    }
  }
}
```

**Making Requests:**

```rust
// Simple GET request
let response = network::fetch(FetchRequest {
    url: "https://api.example.com/data".to_string(),
    method: "GET".to_string(),
    ..Default::default()
})?;

// POST with JSON body
let response = network::fetch_json(FetchJsonRequest {
    url: "https://api.example.com/messages".to_string(),
    method: "POST".to_string(),
    body: json!({
        "model": "gpt-4",
        "messages": [{"role": "user", "content": "Hello!"}]
    }),
    ..Default::default()
})?;
```

**Secret Injection:**

Secrets (like API keys) are never exposed to your app code. The host injects them automatically:

```json
{
  "secrets": {
    "anthropic_key": {
      "description": "Anthropic API key",
      "required": true,
      "inject": {
        "domains": ["api.anthropic.com"],
        "header": "x-api-key"
      }
    }
  }
}
```

When your app calls `api.anthropic.com`, the host automatically adds the `x-api-key` header.

**Security Restrictions:**

- Only declared domains are accessible
- Localhost and private IPs are always blocked
- HTTPS is strongly recommended
- Rate limiting applies (100 req/min, 10,000 req/day per domain)

## Installing Apps

### Via HTTP API

```bash
# Install from a .postapp file
curl -X POST http://localhost:4433/api/v1/apps/install \
  -H "Authorization: Bearer $TOKEN" \
  -F "package=@notes.postapp"

# Install from a repository
curl -X POST http://localhost:4433/api/v1/apps/install \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"repository": "https://apps.example.com", "app_id": "com.example.notes"}'

# List installed apps
curl http://localhost:4433/api/v1/apps \
  -H "Authorization: Bearer $TOKEN"

# Uninstall
curl -X DELETE http://localhost:4433/api/v1/apps/com.example.notes \
  -H "Authorization: Bearer $TOKEN"
```

### Calling App Functions

```bash
# Call an exported function
curl -X POST http://localhost:4433/api/v1/apps/com.example.notes/call/create_note \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"title": "My Note", "content": "Hello world"}'

# Response
{
  "result": {
    "id": "abc123",
    "title": "My Note",
    "content": "Hello world",
    "created_at": 1705708800,
    "updated_at": 1705708800
  }
}
```

## Security Model

1. **Sandboxed Execution**: WASM runs in an isolated sandbox with no direct system access
2. **Capability-Based Permissions**: Apps declare required capabilities; users grant them at install
3. **Fuel Metering**: Computation is metered to prevent runaway apps
4. **Memory Limits**: Apps have bounded memory (default 16 MB initial, 256 MB max)
5. **Signed Packages**: Apps are signed by authors; signatures verified at install
6. **Revocation**: Authors can revoke compromised app versions via identity key rotation

## App Ideas

| Category | App | Capabilities |
|----------|-----|--------------|
| Productivity | Notes | storage, sync |
| Productivity | Tasks/Todo | storage, sync, notify |
| Productivity | Calendar | storage, sync, contacts, notify |
| Communication | Chat | messaging, contacts, notify |
| Communication | Email Bridge | messaging, storage |
| Social | Microblog | storage, messaging, contacts |
| Social | Photo Sharing | storage, messaging |
| Finance | Expense Tracker | storage, sync |
| Finance | Invoice Manager | storage, messaging |
| Automation | Webhook Handler | messaging, storage |
| Automation | Scheduled Tasks | storage, notify |
| AI | LLM Assistant | storage, network:https:api.anthropic.com |
| AI | Weather Agent | storage, network:https:api.weather.gov, notify |
| Integration | RSS Reader | storage, network:https:*, notify |
| Integration | GitHub Notifier | storage, network:https:api.github.com, notify |

## Next Steps

1. Check out the [postapp-sdk](https://github.com/example/postapp-sdk) for the Rust SDK
2. Browse the [app repository](https://apps.postmesh.org) for examples
3. Join the developer chat to get help
