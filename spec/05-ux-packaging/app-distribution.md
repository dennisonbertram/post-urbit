# App Distribution

## Overview

App distribution covers how applications are packaged, signed, discovered, and installed on nodes. The system supports multiple distribution channels with varying trust models.

## Package Format

### File Structure

Applications are distributed as `.postapp` files, which are ZIP archives with a specific structure:

```
myapp-1.0.0.postapp (ZIP archive)
├── manifest.json           # Required: App metadata and configuration
├── main.wasm               # Required: Primary WASM module
├── SIGNATURE               # Required: Package signature
├── assets/                 # Optional: Static assets
│   ├── icon.png            # 512x512 PNG, app icon
│   ├── icon-small.png      # 64x64 PNG, small icon
│   └── screenshots/        # App screenshots
│       ├── 1.png
│       └── 2.png
├── ui/                     # Optional: Web UI files
│   ├── index.html
│   ├── app.js
│   └── styles.css
├── locales/                # Optional: Localization files
│   ├── en.json
│   └── es.json
└── README.md               # Optional: User documentation
```

### Size Limits

| Component | Limit |
|-----------|-------|
| Total package | 100 MB |
| main.wasm | 50 MB |
| Single asset | 10 MB |
| manifest.json | 64 KB |
| Total UI files | 20 MB |

### Manifest Extensions for Distribution

Beyond the fields in `04-app-runtime/manifest-schema.md`, distribution adds:

```json
{
  "distribution": {
    "min_node_version": "1.0.0",
    "max_node_version": "2.0.0",
    "platforms": ["linux", "darwin", "windows"],
    "architectures": ["amd64", "arm64"],
    "update_url": "https://example.com/apps/myapp/updates.json",
    "changelog": "assets/CHANGELOG.md"
  },
  "files": {
    "hashes": {
      "main.wasm": "sha256:abc123...",
      "assets/icon.png": "sha256:def456...",
      "ui/index.html": "sha256:789abc..."
    },
    "total_size": 1234567
  }
}
```

## Signing Model

### Package Signing

Every `.postapp` must be signed by the author's identity signing key.

```
SIGNATURE File Format (UTF-8 text):
┌─────────────────────────────────────────────────────────────────┐
│ author_iid: k5xq7z4m2n3p5r6s7t2u3v4w5x2y3z7a                    │
│ timestamp: 2025-01-15T12:00:00Z                                 │
│ signature: <base64-ed25519-signature-64-bytes>                  │
│ signed_manifest_hash: sha256:<hex-hash-of-canonical-manifest>   │
└─────────────────────────────────────────────────────────────────┘
```

### Signing Process

```
1. Create manifest.json with all file hashes in files.hashes
2. Canonicalize manifest.json (JCS - JSON Canonicalization Scheme)
3. Compute: manifest_hash = SHA256(canonical_manifest_bytes)
4. Create signing payload:
   payload = "postapp-signature-v1:" || manifest_hash || ":" || timestamp
5. Sign: signature = Ed25519_Sign(author_signing_key, payload)
6. Write SIGNATURE file
7. Package all files into ZIP archive
```

### Verification Process

```
1. Extract SIGNATURE file from package
2. Parse author_iid, timestamp, signature, signed_manifest_hash
3. Validate timestamp:
   - NOT in future (with 5 minute clock skew tolerance)
   - NOT before author's identity genesis (sanity check)
   - NOTE: Old signatures are valid if key was valid at signing time
4. Extract and parse manifest.json
5. Canonicalize manifest.json
6. Compute manifest_hash = SHA256(canonical_manifest_bytes)
7. Verify: manifest_hash == signed_manifest_hash
8. Fetch author's identity document (from DHT or local cache)
9. Check author's signing key at timestamp was valid:
   - Lookup key that was active at timestamp in identity document
   - Verify key was not revoked before timestamp
10. Reconstruct payload = "postapp-signature-v1:" || manifest_hash || ":" || timestamp
11. Verify Ed25519_Verify(author_signing_key, payload, signature)
12. For each file in package:
    - Compute hash
    - Verify hash matches manifest.files.hashes[filename]
13. Check author_iid is not blocklisted
```

### Timestamp Semantics

The package signature timestamp has the following semantics:

| Check | Behavior | Rationale |
|-------|----------|-----------|
| Future timestamp | Reject (>5 min ahead) | Prevent pre-dated signatures |
| Old timestamp (packages) | **Accept** | Allow installing older versions, cached/archived packages |
| Old timestamp (update manifests) | Warn if >7 days | Freshness hint for update checks only |
| Key validity at timestamp | Verify | Ensure key wasn't compromised before signing |

**Why old package signatures are allowed:**
- Users may install from offline storage or backups
- Older versions may be deliberately installed for compatibility
- Repositories may serve stable versions for extended periods

**Freshness warnings (optional):**
When installing from a direct URL (not cached/local file), the UI may display:
- "This package was signed X days ago" if >30 days old
- User can proceed if they trust the source

### Key Rotation Handling

If author rotated keys since signing:
- Verify signature against key that was current at `timestamp`
- Use `keys.signing.history` from author's identity document
- Keys older than history retention (default: 2 years) cannot be verified
- Key revocation with timestamp T invalidates all signatures after T

## Distribution Channels

### Channel Types

| Channel | Trust Model | Use Case |
|---------|-------------|----------|
| **Direct** | User verifies author | Installing from known developer |
| **Repository** | Repository curates | Community app store |
| **Enterprise** | Org policy controls | Corporate deployments |
| **Sideload** | User accepts all risk | Development, testing |

### Direct Installation

User provides package URL or file directly:

```
Installation Flow:
1. User provides .postapp URL or file
2. Node downloads/reads package
3. Verify signature
4. Show author info + permissions to user
5. User confirms installation
6. Extract and install
```

### Repository Model

Repositories are curated collections of apps with additional metadata.

**Repository Manifest (repository.json):**
```json
{
  "repository": {
    "name": "Community Apps",
    "id": "community.postnode.org",
    "operator_iid": "k5xq7z4m...",
    "url": "https://apps.postnode.org",
    "description": "Community-curated applications",
    "policies": {
      "review_required": true,
      "automated_scanning": true,
      "signature_verification": true
    }
  },
  "apps": [
    {
      "id": "com.example.notes",
      "name": "Notes",
      "author_iid": "abc123...",
      "latest_version": "1.2.0",
      "download_url": "https://apps.postnode.org/packages/notes-1.2.0.postapp",
      "listing": {
        "description": "A simple note-taking app",
        "category": "productivity",
        "rating": 4.5,
        "downloads": 1234,
        "screenshots": ["url1", "url2"],
        "added_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-15T00:00:00Z"
      },
      "versions": [
        {
          "version": "1.2.0",
          "download_url": "...",
          "size": 1234567,
          "released_at": "2025-01-15T00:00:00Z",
          "changelog": "Bug fixes and performance improvements"
        }
      ]
    }
  ],
  "signature": "<repository-operator-signature>"
}
```

**Repository Manifest Signing:**

Repository manifests MUST be signed by the operator's identity signing key, following the same pattern as package signing:

```
Repository Signature Process:
1. Create repository.json WITHOUT the "signature" field
2. Canonicalize using JCS (JSON Canonicalization Scheme)
3. Compute: manifest_hash = SHA256(canonical_json_bytes)
4. Create payload:
   payload = "postnode-repo-v1:" || manifest_hash || ":" || timestamp
5. Sign: signature = Ed25519_Sign(operator_signing_key, payload)
6. Add signature field:
   "signature": {
     "operator_iid": "<operator_iid>",
     "timestamp": "<RFC3339>",
     "sig": "<base64-ed25519-signature>"
   }
```

**Verification:**
- Fetch operator's identity document from DHT
- Verify operator's key was valid at signature timestamp
- Freshness check: warn if signature >7 days old (repository should be refreshed)
- Cache repository manifest with TTL (default: 1 hour)

**Repository Trust:**
```typescript
interface TrustedRepository {
  id: string;
  operatorIid: IdentityIdentifier;
  operatorKeyFingerprint: string;  // Pinned key (optional)
  url: string;
  trustLevel: 'full' | 'prompt' | 'disabled';
  autoUpdate: boolean;
  addedAt: Timestamp;
}

// Trust levels:
// - full: Install without additional prompts
// - prompt: Show repository endorsement but still prompt
// - disabled: Do not install from this repository

// Key pinning (optional):
// If operatorKeyFingerprint is set, ONLY accept signatures from that specific key
// This protects against operator key compromise (at cost of manual update on rotation)
```

### Enterprise Distribution

Organizations can enforce policies on app installation:

```json
{
  "enterprise_policy": {
    "org_id": "acme-corp",
    "policy_version": 1,
    "rules": {
      "allowed_repositories": ["corp.acme.com/apps"],
      "blocked_repositories": ["*"],  // Block all except allowed
      "allowed_authors": ["k5xq7z4m..."],  // IT-approved developers
      "blocked_authors": [],
      "required_capabilities_review": true,
      "max_storage_per_app": "500mb",
      "allowed_capabilities": [
        "storage:app",
        "messaging:send",
        "messaging:subscribe"
      ],
      "blocked_capabilities": [
        "system:background",
        "notifications:*"
      ]
    },
    "signature": "<org-signing-key-signature>"
  }
}
```

### Sideloading

For development and testing, nodes can install unsigned packages:

```toml
# config.toml
[apps]
allow_sideload = true  # Default: false
sideload_warning = true  # Show warning on sideload
```

Sideloaded apps:
- Marked with "Sideloaded" badge in UI
- Cannot access certain capabilities by default
- Are not auto-updated
- Show prominent security warning

## Update Mechanism

### Update Discovery

Apps can specify an update URL in their manifest:

```json
{
  "distribution": {
    "update_url": "https://example.com/apps/myapp/updates.json"
  }
}
```

**Update Manifest (updates.json):**
```json
{
  "app_id": "com.example.myapp",
  "author_iid": "k5xq7z4m...",
  "latest": {
    "version": "1.3.0",
    "download_url": "https://example.com/apps/myapp-1.3.0.postapp",
    "size": 1234567,
    "released_at": "2025-01-20T00:00:00Z",
    "min_node_version": "1.0.0",
    "changelog": "New features and bug fixes",
    "critical": false
  },
  "history": [
    {
      "version": "1.2.0",
      "download_url": "...",
      "released_at": "2025-01-15T00:00:00Z"
    }
  ],
  "signature": {
    "author_iid": "k5xq7z4m...",
    "timestamp": "2025-01-20T00:00:00Z",
    "sig": "<base64-ed25519-signature>"
  }
}
```

**Update Manifest Signing:**

Update manifests MUST be signed by the app author (same `author_iid` as the installed app):

```
Update Signature Process:
1. Create updates.json WITHOUT the "signature" field
2. Canonicalize using JCS
3. Compute: manifest_hash = SHA256(canonical_json_bytes)
4. Create payload:
   payload = "postnode-update-v1:" || app_id || ":" || manifest_hash || ":" || timestamp
5. Sign: signature = Ed25519_Sign(author_signing_key, payload)
6. Add signature object (see above)
```

**Update Verification:**
- `author_iid` MUST match installed app's author (prevents takeover)
- `app_id` MUST match installed app's ID
- Signature must be valid for author's current or historical key
- Freshness: warn if signature >7 days old (but don't reject)

### Update Flow

```
Update Check (background, every 24h by default):
1. For each installed app with update_url:
   a. Fetch updates.json
   b. Verify signature matches app author
   c. Compare versions
   d. If newer version available and compatible:
      - If auto_update enabled: queue download
      - Otherwise: notify user

Update Installation:
1. Download new package
2. Verify signature (must be same author_iid)
3. Check compatibility (node version, platform)
4. If permissions changed: prompt user
5. Stop running app instance
6. Backup current app data
7. Install new version
8. Migrate data if needed (app provides migration)
9. Start new version
10. If startup fails: rollback to previous version
```

### Version Comparison

Versions follow Semantic Versioning (semver):
- `MAJOR.MINOR.PATCH` (e.g., `1.2.3`)
- Pre-release: `1.2.3-beta.1`
- Build metadata: `1.2.3+build.456`

```typescript
function shouldUpdate(current: string, available: string): boolean {
  // Parse versions
  const curr = parseSemver(current);
  const avail = parseSemver(available);

  // Never downgrade
  if (compareSemver(avail, curr) <= 0) return false;

  // Update if newer
  return true;
}
```

## Security Considerations

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Malicious package | Signature verification, capability review |
| Compromised author key | Key rotation detection, blocklist |
| Supply chain attack | Content-addressed packages, reproducible builds |
| Repository compromise | Signed repository manifests, client-side verification |
| Downgrade attack | Version monotonicity check |
| MITM on download | HTTPS + signature verification |

### Author Blocklist

Nodes maintain a blocklist of compromised or malicious authors:

```typescript
interface Blocklist {
  // Local blocklist (user-managed)
  local: BlocklistEntry[];

  // Community blocklist (from trusted source)
  community?: {
    source_url: string;
    last_updated: Timestamp;
    entries: BlocklistEntry[];
  };
}

interface BlocklistEntry {
  author_iid: IdentityIdentifier;
  reason: string;
  added_at: Timestamp;
  apps_affected: string[];  // Specific apps, or "*" for all
}
```

### Capability Review

Before installation, node shows capability summary:

```
┌─────────────────────────────────────────────────────────────────┐
│  Install "Notes App" by Alice?                                   │
├─────────────────────────────────────────────────────────────────┤
│  Author: Alice (k5xq7z4m...)  ✓ Verified                        │
│  Version: 1.2.0                                                  │
│  Size: 1.2 MB                                                    │
├─────────────────────────────────────────────────────────────────┤
│  This app requests:                                              │
│                                                                  │
│  ● Storage (required)                                            │
│    To save your notes locally                                    │
│                                                                  │
│  ● Sync (required)                                               │
│    To sync notes across your devices                             │
│                                                                  │
│  ○ Notifications (optional)                                      │
│    To remind you about notes                                     │
├─────────────────────────────────────────────────────────────────┤
│  [Cancel]                              [Install with Selected]   │
└─────────────────────────────────────────────────────────────────┘
```

## CLI Commands

```
postnode apps - Manage applications

USAGE:
    postnode apps <COMMAND>

COMMANDS:
    list                List installed apps
    install <SOURCE>    Install an app (URL, file, or app_id from repo)
    uninstall <APP_ID>  Uninstall an app
    update [APP_ID]     Update app(s)
    info <APP_ID>       Show app details
    search <QUERY>      Search repositories

OPTIONS:
    --repo <URL>        Use specific repository
    --force             Skip confirmation prompts
    --sideload          Allow unsigned packages

EXAMPLES:
    postnode apps list
    postnode apps install https://example.com/myapp.postapp
    postnode apps install ./myapp.postapp --sideload
    postnode apps install com.example.notes --repo https://apps.postnode.org
    postnode apps update
    postnode apps uninstall com.example.myapp
```

## Repository API

For repository operators, the expected API:

```
GET /repository.json
  Returns: Repository manifest with app listings

GET /packages/{app_id}-{version}.postapp
  Returns: Package file

GET /apps/{app_id}/updates.json
  Returns: Update manifest for specific app

GET /apps/{app_id}/reviews
  Returns: User reviews (optional feature)

POST /apps/{app_id}/report
  Body: { reason: string, details: string }
  Reports problematic app to repository operator
```
