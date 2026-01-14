# App Manifest Schema

## Overview

Every application includes a manifest file (`manifest.json`) that declares metadata, capabilities, entry points, and resources. The manifest is verified during installation.

## Manifest Structure

```json
{
  "manifest_version": 1,
  "app": {
    "id": "com.example.myapp",
    "name": "My Application",
    "version": "1.0.0",
    "description": "A brief description of what the app does",
    "author": {
      "name": "Developer Name",
      "iid": "k5xq7z4m2n3p5r6s7t2u3v4w5x2y3z7a",
      "url": "https://example.com"
    },
    "license": "MIT",
    "homepage": "https://example.com/myapp",
    "repository": "https://github.com/example/myapp"
  },
  "runtime": {
    "entry": "main.wasm",
    "memory": {
      "initial_pages": 16,
      "maximum_pages": 1024
    },
    "fuel": {
      "user_action": 1000000,
      "background_task": 100000
    }
  },
  "capabilities": {
    "required": [
      "storage:app",
      "messaging:send",
      "messaging:subscribe"
    ],
    "optional": [
      "contacts:read:limited",
      "notifications:show"
    ],
    "reasons": {
      "messaging:send": "To send messages to your contacts",
      "messaging:subscribe": "To receive messages from your contacts",
      "contacts:read:limited": "To show which contacts also use this app"
    }
  },
  "storage": {
    "quota": "50mb",
    "shared_namespaces": ["photos", "documents"]
  },
  "ui": {
    "icon": "assets/icon.png",
    "screenshots": [
      "assets/screenshot1.png",
      "assets/screenshot2.png"
    ],
    "category": "social",
    "content_rating": "everyone"
  },
  "handlers": {
    "message_types": ["com.example.myapp.custom"],
    "file_types": [".myapp"],
    "url_schemes": ["myapp://"]
  },
  "background": {
    "enabled": true,
    "triggers": [
      {
        "type": "interval",
        "interval_seconds": 3600
      },
      {
        "type": "message",
        "message_type": "com.example.myapp.*"
      }
    ]
  },
  "dependencies": {
    "node_version": ">=1.0.0",
    "api_version": "1"
  },
  "files": {
    "hashes": {
      "main.wasm": "sha256:a1b2c3d4...",
      "assets/icon.png": "sha256:e5f6a7b8..."
    },
    "total_size": 1048576
  }
}
// NOTE: Signature is in separate SIGNATURE file, NOT in manifest.json
// See 05-ux-packaging/app-distribution.md for signing/verification
```

## Field Specifications

### Manifest Version

```typescript
interface ManifestVersion {
  manifest_version: 1;  // Must be exactly 1 for this schema version
}
```

### App Metadata

```typescript
interface AppMetadata {
  app: {
    // Unique identifier (reverse domain notation)
    id: string;           // Pattern: ^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$
                          // Max length: 64 characters

    // Human-readable name
    name: string;         // Max length: 50 characters

    // Semantic version
    version: string;      // Pattern: ^\d+\.\d+\.\d+(-[a-z0-9.]+)?$

    // Brief description
    description: string;  // Max length: 200 characters

    // Author information
    author: {
      name: string;       // Max length: 100 characters
      iid?: string;       // Optional identity identifier
      url?: string;       // Optional website
    };

    // License identifier (SPDX or custom)
    license: string;

    // Optional URLs
    homepage?: string;
    repository?: string;
  };
}
```

### Runtime Configuration

```typescript
interface RuntimeConfig {
  runtime: {
    // Entry point WASM file (relative to package root)
    entry: string;        // Must end with .wasm

    // Memory configuration
    memory?: {
      initial_pages: number;   // Default: 16 (1MB)
      maximum_pages: number;   // Default: 1024 (64MB)
    };

    // Fuel configuration (overrides defaults)
    fuel?: {
      user_action?: number;
      background_task?: number;
      app_start?: number;
    };
  };
}
```

### Capabilities

```typescript
interface CapabilitiesConfig {
  capabilities: {
    // Required capabilities (must be granted at install)
    required: string[];

    // Optional capabilities (can be requested at runtime)
    optional?: string[];

    // Human-readable reasons for each capability
    reasons?: Record<string, string>;
  };
}
```

### Storage Configuration

```typescript
interface StorageConfig {
  storage?: {
    // Requested storage quota
    quota: string;        // Pattern: ^\d+(kb|mb|gb)$
                          // Default: 100mb

    // Shared namespaces to access (requires capability)
    shared_namespaces?: string[];
  };
}
```

### UI Configuration

```typescript
interface UIConfig {
  ui?: {
    // App icon (relative path)
    icon: string;         // Required, must be PNG, 256x256 recommended

    // Screenshots for app store
    screenshots?: string[];  // Max 5

    // Category for organization
    category: AppCategory;

    // Content rating
    content_rating: ContentRating;
  };
}

type AppCategory =
  | 'social'
  | 'productivity'
  | 'utilities'
  | 'games'
  | 'media'
  | 'finance'
  | 'health'
  | 'education'
  | 'other';

type ContentRating =
  | 'everyone'        // All ages
  | 'teen'            // 13+
  | 'mature';         // 18+
```

### Handler Registration

```typescript
interface HandlersConfig {
  handlers?: {
    // Message types this app handles
    message_types?: string[];    // Pattern matching supported

    // File extensions this app can open
    file_types?: string[];       // e.g., [".txt", ".myformat"]

    // URL schemes this app handles
    url_schemes?: string[];      // e.g., ["myapp://"]
  };
}
```

### Background Execution

```typescript
interface BackgroundConfig {
  background?: {
    // Whether background execution is enabled
    enabled: boolean;

    // What triggers background execution
    triggers: BackgroundTrigger[];
  };
}

type BackgroundTrigger =
  | { type: 'interval'; interval_seconds: number }  // Min: 60, Max: 86400
  | { type: 'message'; message_type: string }       // Pattern matching
  | { type: 'sync'; document_type: string }         // Sync events
  | { type: 'boot' };                               // Node startup
```

### Dependencies

```typescript
interface DependenciesConfig {
  dependencies?: {
    // Minimum node version required
    node_version?: string;   // Semver range

    // API version required
    api_version: string;     // Currently "1"

    // Other apps this app depends on
    apps?: Record<string, string>;  // app_id -> version range
  };
}
```

### Package Signature (Normative)

**IMPORTANT:** Package signing uses the **SIGNATURE file approach**, NOT an embedded manifest.signature field.

The authoritative specification is in `05-ux-packaging/app-distribution.md`. Key points:

1. **SIGNATURE file is REQUIRED** in the `.postapp` ZIP archive
2. **manifest.json does NOT contain a signature field**
3. The SIGNATURE file contains:
   - `author_iid`: Signer's identity identifier
   - `timestamp`: When the package was signed
   - `signature`: Ed25519 signature over the signing payload
   - `signed_manifest_hash`: SHA256 of canonical manifest.json

### File Hashes

File hashes are REQUIRED in manifest.json for package integrity:

```typescript
interface FilesConfig {
  // Content hashes for all files in package
  files: {
    hashes: Record<string, string>;  // path -> "sha256:<hex>"
    total_size: number;               // Total uncompressed size in bytes
  };
}
```

Example:
```json
{
  "files": {
    "hashes": {
      "main.wasm": "sha256:a1b2c3d4e5f6...",
      "assets/icon.png": "sha256:f6e5d4c3b2a1..."
    },
    "total_size": 1234567
  }
}
```

### Verification Flow

See `05-ux-packaging/app-distribution.md` § Verification Process for the complete verification algorithm:

1. Extract and verify SIGNATURE file
2. Verify manifest hash matches signed_manifest_hash
3. Verify each file hash matches manifest.files.hashes
4. Verify author's identity and signing key validity

## Manifest Validation

### Required Fields

The following fields are required:

- `manifest_version`
- `app.id`
- `app.name`
- `app.version`
- `app.description`
- `app.author.name`
- `app.license`
- `runtime.entry`
- `capabilities.required` (may be empty array)
- `dependencies.api_version`
- `files` (must include at least entry point)
_(signature is in SIGNATURE file, not manifest.json)_

### Validation Rules

```typescript
interface ManifestValidator {
  validate(manifest: unknown): ValidationResult;
}

interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
  warnings: ValidationWarning[];
}

interface ValidationError {
  field: string;
  code: string;
  message: string;
}

interface ValidationWarning {
  field: string;
  code: string;
  message: string;
}
```

### Validation Error Codes

| Code | Description |
|------|-------------|
| `MISSING_REQUIRED` | Required field is missing |
| `INVALID_TYPE` | Field has wrong type |
| `INVALID_FORMAT` | Field doesn't match required format |
| `INVALID_VALUE` | Field value is out of allowed range |
| `INVALID_CAPABILITY` | Unknown capability requested |
| `INVALID_SIGNATURE` | Signature verification failed |
| `INCOMPATIBLE_VERSION` | Incompatible manifest/API version |

### Signature Verification

```typescript
function verifyManifestSignature(manifest: Manifest): boolean {
  // 1. Extract signature (but keep files)
  const { signature, ...rest } = manifest;

  // 2. Canonicalize manifest including files (JCS - RFC 8785)
  const canonical = canonicalize(rest);

  // 3. Verify Ed25519 signature
  return ed25519Verify(
    base64Decode(signature.public_key),
    utf8Encode(canonical),
    base64Decode(signature.signature)
  );
}

function verifyPackageIntegrity(
  manifest: Manifest,
  packageFiles: Map<string, Uint8Array>
): boolean {
  // 1. Verify manifest signature first
  if (!verifyManifestSignature(manifest)) {
    return false;
  }

  // 2. Verify each file hash
  for (const [path, expectedHash] of Object.entries(manifest.files)) {
    const fileData = packageFiles.get(path);
    if (!fileData) {
      return false;  // Missing file
    }

    const actualHash = 'sha256:' + hex(sha256(fileData));
    if (actualHash !== expectedHash) {
      return false;  // Hash mismatch
    }
  }

  // 3. Verify no extra files (optional, for strict mode)
  // for (const path of packageFiles.keys()) {
  //   if (!(path in manifest.files) && path !== 'manifest.json') {
  //     return false;  // Unexpected file
  //   }
  // }

  return true;
}
```

## Manifest Examples

### Minimal Manifest

```json
{
  "manifest_version": 1,
  "app": {
    "id": "com.example.minimal",
    "name": "Minimal App",
    "version": "1.0.0",
    "description": "A minimal example application",
    "author": { "name": "Example Developer" },
    "license": "MIT"
  },
  "runtime": {
    "entry": "main.wasm"
  },
  "capabilities": {
    "required": []
  },
  "dependencies": {
    "api_version": "1"
  },
  "files": {
    "main.wasm": "sha256:a1b2c3d4e5f6789..."
  },
  "signature": {
    "algorithm": "ed25519",
    "public_key": "...",
    "signature": "..."
  }
}
```

### Chat Application Manifest

```json
{
  "manifest_version": 1,
  "app": {
    "id": "com.example.chat",
    "name": "Simple Chat",
    "version": "2.1.0",
    "description": "A simple peer-to-peer chat application",
    "author": {
      "name": "Chat Developers",
      "iid": "k5xq7z4m2n3p5r6s7t2u3v4w5x2y3z7a",
      "url": "https://chat.example.com"
    },
    "license": "Apache-2.0",
    "homepage": "https://chat.example.com",
    "repository": "https://github.com/example/chat"
  },
  "runtime": {
    "entry": "chat.wasm",
    "memory": {
      "initial_pages": 32,
      "maximum_pages": 512
    }
  },
  "capabilities": {
    "required": [
      "storage:app",
      "messaging:send",
      "messaging:subscribe",
      "contacts:read:limited"
    ],
    "optional": [
      "notifications:show",
      "notifications:sound",
      "system:background"
    ],
    "reasons": {
      "messaging:send": "To send chat messages to your contacts",
      "messaging:subscribe": "To receive chat messages from your contacts",
      "contacts:read:limited": "To show which of your contacts use this app",
      "notifications:show": "To notify you of new messages when the app is in the background",
      "system:background": "To check for new messages periodically"
    }
  },
  "storage": {
    "quota": "200mb"
  },
  "ui": {
    "icon": "assets/icon.png",
    "screenshots": [
      "assets/screenshot1.png",
      "assets/screenshot2.png"
    ],
    "category": "social",
    "content_rating": "everyone"
  },
  "handlers": {
    "message_types": ["com.example.chat.message", "com.example.chat.typing"],
    "url_schemes": ["chat://"]
  },
  "background": {
    "enabled": true,
    "triggers": [
      { "type": "message", "message_type": "com.example.chat.*" },
      { "type": "interval", "interval_seconds": 300 }
    ]
  },
  "dependencies": {
    "node_version": ">=1.0.0",
    "api_version": "1"
  },
  "files": {
    "chat.wasm": "sha256:b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M...",
    "assets/icon.png": "sha256:kL3mN4pQ5rS6tU7vW8xY9zA0bC1dE2fG3hI4jK5lM6n...",
    "assets/screenshot1.png": "sha256:O7pQ8rS9tU0vW1xY2zA3bC4dE5fG6hI7jK8lM9nO0pQ...",
    "assets/screenshot2.png": "sha256:rS6tU7vW8xY9zA0bC1dE2fG3hI4jK5lM6nO7pQ8rS9t..."
  },
  "signature": {
    "algorithm": "ed25519",
    "public_key": "b7YHv0KMZrt8VK4m5FJw6Qx2pL9dN3hR1sA0cE4gI8M",
    "signature": "kL3mN4pQ5rS6tU7vW8xY9zA0bC1dE2fG3hI4jK5lM6nO7pQ8rS9tU0vW1xY2zA3bC4dE5fG6hI7jK8lM9nO0pQr"
  }
}
```

## Package Format

**Canonical format:** `.postapp` (ZIP archive) - see `05-ux-packaging/app-distribution.md` for complete specification.

### Directory Structure

```
myapp-1.0.0.postapp (ZIP archive)
├── manifest.json           # Required: App manifest with signature
├── SIGNATURE               # Required: Package signature file
├── main.wasm               # Required: Entry point
├── assets/                 # Optional: Static assets
│   ├── icon.png
│   ├── icon-small.png
│   └── screenshots/
├── ui/                     # Optional: Web UI files
│   ├── index.html
│   └── ...
├── locales/                # Optional: Localization
│   └── ...
└── README.md               # Optional: Documentation
```

### Content Addressing

Packages are identified by their content hash:

```
package_hash = SHA256(package_bytes)
package_id = "sha256:" + hex(package_hash)
```

### Package Signing

The package signature binds author identity to package contents.

**SIGNATURE file** (in package root): Author's signature over manifest hash with timestamp
- See `05-ux-packaging/app-distribution.md` for signing/verification process
- Payload: `"postapp-signature-v1:" || HEX(manifest_hash) || ":" || timestamp`
- `manifest_hash` is SHA-256 of JCS-canonical manifest.json, hex-encoded in payload
- `timestamp` is RFC3339 UTC (e.g., `2025-01-15T12:00:00Z`)

**manifest.json does NOT contain a signature field.** All signature verification uses the SIGNATURE file exclusively.

**Rationale**: A single authoritative signature location avoids ambiguity. The manifest remains content-only; signing is handled by the package wrapper.

### Package Verification

```typescript
interface PackageVerifier {
  // Verify package integrity and signature
  verify(packageBytes: Uint8Array): VerificationResult;
}

interface VerificationResult {
  valid: boolean;
  manifest: Manifest;
  contentHash: string;
  authorIid: string;
  signedAt: string;
  errors: string[];
}
```

Verification steps:
1. Extract ZIP archive
2. Parse SIGNATURE file (author_iid, timestamp, signature, signed_manifest_hash)
3. Parse manifest.json
4. Validate manifest schema
5. Canonicalize manifest.json (JCS)
6. Verify manifest hash matches SIGNATURE.signed_manifest_hash
7. Fetch author's identity document
8. Verify signature using author's signing key valid at timestamp
9. Verify each file hash in manifest.files matches actual file
10. Verify entry point exists and is valid WASM
