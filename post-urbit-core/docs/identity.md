# Post-Urbit Identity System

This document provides comprehensive developer documentation for the Post-Urbit identity system. The identity system is the foundation of Post-Urbit's self-sovereign architecture - every node has a cryptographic identity that it controls completely.

## Table of Contents

1. [IID (Identity ID)](#iid-identity-id)
2. [Identity Documents](#identity-documents)
3. [Key Management](#key-management)
4. [Recovery](#recovery)
5. [Verification](#verification)
6. [Bootstrap/TOFU](#bootstraptofu)
7. [Code Examples](#code-examples)

---

## IID (Identity ID)

### What is an IID?

An **IID (Identity ID)** is a permanent, cryptographically-derived identifier for a Post-Urbit identity. Think of it as your permanent address in the Post-Urbit network - it never changes, even when you rotate your keys.

### How IIDs are Derived

IIDs are derived from the **genesis signing key** using the following algorithm:

```
IID = lowercase(crockford_base32(SHA256(genesis_public_key)[0..20]))
```

The derivation process (from `src/identity.rs` lines 579-584):

```rust
pub fn derive_iid(verifying_key: &VerifyingKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifying_key.as_bytes());
    let hash = hasher.finalize();
    crockford_base32_encode(&hash[..20]).to_lowercase()
}
```

### IID Format

- **Length**: 32 characters
- **Encoding**: Lowercase Crockford Base32
- **Example**: `b1n7cfscgashm32xx7eaxw0y09gy0y2v`

### Crockford Base32 Alphabet

The encoding uses Crockford's Base32 alphabet, which excludes ambiguous characters:

```
0123456789abcdefghjkmnpqrstvwxyz
```

Note: Letters `i`, `l`, `o`, and `u` are excluded to prevent confusion with `1`, `1`, `0`, and `v`.

### Why This Design?

1. **Permanent**: The IID is derived from the genesis key and never changes, even after key rotations
2. **Self-certifying**: Anyone can verify the IID by checking it against the genesis key
3. **Collision-resistant**: 160 bits of SHA-256 output provides strong collision resistance
4. **Human-readable**: Crockford Base32 is more readable than hex while staying compact

### IID Derivation Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    IID Derivation Process                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   Ed25519 Genesis Public Key (32 bytes)                         │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │ e3c7a72049df8c4623a2d4b61db1d76a6c3ea2efaae7b87e9d46... │   │
│   └─────────────────────────┬───────────────────────────────┘   │
│                             │                                    │
│                             ▼                                    │
│                      ┌─────────────┐                            │
│                      │   SHA-256   │                            │
│                      └──────┬──────┘                            │
│                             │                                    │
│                             ▼                                    │
│   SHA-256 Hash (32 bytes)                                       │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │ b1n7cf...                                               │   │
│   └─────────────────────────┬───────────────────────────────┘   │
│                             │                                    │
│                             ▼                                    │
│                    Take first 20 bytes                          │
│                             │                                    │
│                             ▼                                    │
│                ┌────────────────────────┐                       │
│                │ Crockford Base32 Encode│                       │
│                │     + lowercase        │                       │
│                └───────────┬────────────┘                       │
│                            │                                     │
│                            ▼                                     │
│   IID (32 characters)                                           │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │            b1n7cfscgashm32xx7eaxw0y09gy0y2v             │   │
│   └─────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Identity Documents

### Structure

An Identity Document (IDOC) contains all information about an identity. Here is the complete structure (from `src/identity.rs` lines 29-45):

```rust
pub struct IdentityDocument {
    pub version: u8,                        // Always 1
    pub iid: String,                        // Identity ID
    pub sequence: String,                   // Document version number
    pub timestamp: String,                  // RFC3339 timestamp
    pub keys: Keys,                         // Signing and encryption keys
    pub endpoints: Vec<Endpoint>,           // Network endpoints
    pub claims: Claims,                     // Profile information
    pub recovery: Recovery,                 // Recovery configuration
    pub extensions: serde_json::Value,      // Future extensibility
    pub recovery_proof: Option<serde_json::Value>,
    pub signatures: Signatures,             // Document signatures
}
```

### Keys Structure

```rust
pub struct Keys {
    pub signing: SigningKeys,
    pub encryption: EncryptionKeys,
}

pub struct SigningKeys {
    pub genesis: String,              // Original signing key (never changes)
    pub current: String,              // Current signing key
    pub previous: Option<String>,     // Previous key (for rotation)
    pub history: Vec<SigningKeyHistory>,  // Full key history
}

pub struct EncryptionKeys {
    pub current: String,              // Current X25519 public key
    pub previous: Vec<EncryptionKeyHistory>,  // Old keys for decryption
}
```

### Sequence Numbers (Versioning)

The `sequence` field is a monotonically increasing integer (stored as a string) that tracks document versions:

- **Genesis document**: `sequence = "0"`
- **Each update**: `sequence` increases by 1
- **No leading zeros**: `"0"` is valid, `"01"` is not (lines 1644-1651)

```rust
fn parse_sequence(value: &str) -> Result<u64> {
    if value.starts_with('0') && value != "0" {
        return Err(PostUrbitError::InvalidInput("sequence leading zeros"));
    }
    value.parse::<u64>()
        .map_err(|_| PostUrbitError::InvalidInput("sequence parse"))
}
```

### Document Size Limits

Per RFC-0001 Section 4.7 (lines 759-827):

| Field | Limit |
|-------|-------|
| Total document size | 16 KB |
| Endpoints | 10 entries |
| Signing key history | 10 entries |
| Encryption key history | 5 entries |
| claims.name | 64 UTF-8 characters |
| claims.bio | 256 UTF-8 characters |

### IDOC Envelope Format

Identity documents are stored in a binary envelope format (lines 743-790):

```
┌──────────────────────────────────────────────────────────┐
│  Bytes 0-3   │  Byte 4   │  Bytes 5-8  │  Bytes 9+      │
│  Magic       │  Version  │  Length     │  JSON Payload  │
│  "IDOC"      │  0x01     │  (u32 BE)   │  (canonical)   │
└──────────────────────────────────────────────────────────┘
```

```rust
const IDOC_MAGIC: &[u8; 4] = b"IDOC";
const IDOC_VERSION: u8 = 1;
const IDOC_MAX_SIZE: usize = 16_384;  // 16 KB
```

---

## Key Management

### Key Types

Post-Urbit uses two types of asymmetric keys:

| Purpose | Algorithm | Size |
|---------|-----------|------|
| Signing | Ed25519 | 32 bytes (public) |
| Encryption | X25519 | 32 bytes (public) |

### Genesis Key

The **genesis signing key** is the original key used to create the identity:
- Determines the IID (permanently)
- Stored in `keys.signing.genesis`
- Never changes, even after rotation
- Must equal `keys.signing.current` for sequence 0 documents

### Key Rotation

#### Signing Key Rotation

When rotating the signing key (lines 376-403):

1. Generate a new Ed25519 keypair
2. Increment sequence number
3. Set `keys.signing.previous` to the old current key
4. Set `keys.signing.current` to the new key
5. Sign the document with the **new** key (`signatures.current`)
6. Sign the document with the **old** key (`signatures.previous`)

```rust
pub async fn rotate_signing_key(&self) -> Result<KeyRotationResult> {
    let mut state = self.inner.write().await;
    let mut document = state.document.clone();
    let old_signing_key = state.signing_key.clone();
    let previous_key = document.keys.signing.current.clone();

    let new_signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let new_public = new_signing_key.verifying_key();
    let next_sequence = parse_sequence(&document.sequence)? + 1;

    document.sequence = next_sequence.to_string();
    document.timestamp = Utc::now().to_rfc3339();
    document.keys.signing.previous = Some(previous_key.clone());
    document.keys.signing.current = base64_encode(new_public.as_bytes());
    document.signatures.current = sign_idoc(&document, &new_signing_key)?;
    document.signatures.previous = Some(sign_idoc(&document, &old_signing_key)?);
    // ...
}
```

#### Encryption Key Rotation

Encryption key rotation (lines 405-438):

1. Generate a new X25519 keypair
2. Move current key to history with validity window
3. Old keys remain available for 30 days to decrypt old messages

```rust
let expires_at = (Utc::now() + chrono::Duration::days(30)).to_rfc3339();
document.keys.encryption.previous.push(EncryptionKeyHistory {
    key: previous_key.clone(),
    valid_from: state.encryption_key_valid_from.to_string(),
    valid_until: next_sequence.to_string(),
    expires_at,
});
```

### Key History Structure

```rust
pub struct SigningKeyHistory {
    pub key: String,          // Base64 public key
    pub valid_from: String,   // Sequence number when key became active
    pub valid_until: String,  // Sequence number when key was rotated out
    pub expires_at: String,   // RFC3339 timestamp when key fully expires
}
```

### Key Rotation Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                     Key Rotation Flow                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Sequence 0 (Genesis)           Sequence 1 (After Rotation)     │
│  ─────────────────────          ──────────────────────────      │
│                                                                  │
│  keys.signing:                  keys.signing:                    │
│    genesis: KeyA                  genesis: KeyA  (unchanged)     │
│    current: KeyA                  current: KeyB  (new key)       │
│    previous: null                 previous: KeyA (old key)       │
│                                                                  │
│  signatures:                    signatures:                      │
│    current: sig(KeyA)             current: sig(KeyB)  ◄── new    │
│    previous: null                 previous: sig(KeyA) ◄── old    │
│                                                                  │
│                     ┌─────────────────┐                         │
│                     │  Verification   │                         │
│                     │  Requirements   │                         │
│                     └────────┬────────┘                         │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 1. Verify signatures.current with keys.signing.current  │    │
│  │ 2. Verify signatures.previous with keys.signing.previous│    │
│  │ 3. Verify keys.signing.previous == old doc's current    │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Recovery

### Recovery Methods

Post-Urbit supports multiple recovery methods:

| Method | Description |
|--------|-------------|
| `none` | No recovery configured (default) |
| `social` | Social recovery via trusted contacts |

### Social Recovery

Social recovery allows you to recover your identity if you lose access to your signing key, by having trusted contacts (trustees) vouch for a new key.

#### Configuration

```json
{
  "method": "social",
  "config": {
    "threshold": 2,
    "trustees": [
      {"iid": "trustee1iid...", "label": "Alice"},
      {"iid": "trustee2iid...", "label": "Bob"},
      {"iid": "trustee3iid...", "label": "Carol"}
    ],
    "cooldown_hours": 72
  }
}
```

#### Requirements (lines 829-871)

| Requirement | Constraint |
|-------------|------------|
| REQ-IDOC-037 | `trustees` array required, `trustees.len() >= threshold` |
| REQ-IDOC-038 | `threshold` required |
| REQ-IDOC-039 | `threshold >= 2` |
| REQ-IDOC-040 | `24 <= cooldown_hours <= 720` (1-30 days) |

### Recovery Attestation

Trustees create attestations to authorize recovery (lines 199-212):

```rust
pub struct RecoveryAttestation {
    pub target_iid: String,         // IID being recovered
    pub trustee_iid: String,        // Trustee's IID
    pub new_signing_key: String,    // New signing public key (base64)
    pub timestamp: String,          // Attestation timestamp
    pub signature: String,          // Ed25519 signature by trustee
}
```

### Recovery Process

```
┌─────────────────────────────────────────────────────────────────┐
│                   Social Recovery Flow                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. User loses access to signing key                            │
│                                                                  │
│  2. User generates NEW signing keypair                          │
│     ┌─────────────────────────────────────────┐                 │
│     │  new_signing_key = generate_ed25519()   │                 │
│     └─────────────────────────────────────────┘                 │
│                                                                  │
│  3. User contacts trustees, shares new public key               │
│                                                                  │
│  4. Each trustee creates attestation:                           │
│     ┌─────────────────────────────────────────┐                 │
│     │  RecoveryAttestation {                  │                 │
│     │    target_iid: "user's iid",            │                 │
│     │    trustee_iid: "trustee's iid",        │                 │
│     │    new_signing_key: "new key base64",   │                 │
│     │    timestamp: "2025-01-15T00:00:00Z",   │                 │
│     │    signature: sign(trustee_key, ...)    │                 │
│     │  }                                      │                 │
│     └─────────────────────────────────────────┘                 │
│                                                                  │
│  5. Collect >= threshold attestations                           │
│                                                                  │
│  6. Wait for cooldown period (72 hours default)                 │
│                                                                  │
│  7. Submit recovery request with attestations                   │
│     ┌─────────────────────────────────────────┐                 │
│     │  verify_social_recovery(dht, iid,       │                 │
│     │                         attestations)   │                 │
│     └─────────────────────────────────────────┘                 │
│                                                                  │
│  8. If threshold met: recovery succeeds                         │
│     - New document published with new signing key               │
│     - recovery_proof contains attestations                      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Verification Algorithm (lines 1467-1590)

The `verify_social_recovery` function:

1. Fetches target identity document
2. Verifies recovery method is `"social"`
3. For each attestation:
   - Verify trustee is in the trustees list
   - Fetch trustee's identity document
   - Verify signature with trustee's signing key
   - Verify timestamp within cooldown window
   - Verify all attestations agree on `new_signing_key`
4. Count valid attestations >= threshold
5. Return the new signing key if successful

---

## Verification

### Document Verification

The core verification function `verify_document` (lines 440-513) performs these checks:

1. **IID Format**: Valid lowercase Crockford Base32
2. **IID Binding**: IID is derived from genesis key (prevents hijacking)
3. **Signature**: Current signature verifies with current key
4. **Key Rotation Continuity**: If keys changed from genesis, verify chain

```rust
pub fn verify_document(document: &IdentityDocument) -> Result<()> {
    // 1. Validate IID format
    validate_crockford_base32_lower(&document.iid)?;

    // 2. SECURITY: Verify IID is derived from genesis signing key
    let genesis_key_bytes = base64_decode(&document.keys.signing.genesis)?;
    let genesis_verifying_key = VerifyingKey::from_bytes(...)?;
    let expected_iid = derive_iid(&genesis_verifying_key);
    if expected_iid != document.iid {
        return Err(PostUrbitError::InvalidInput("iid not derived from genesis key"));
    }

    // 3. Verify current signature
    let current_key = base64_decode(&document.keys.signing.current)?;
    let verifying_key = VerifyingKey::from_bytes(...)?;
    let signed_payload = signature_payload(document)?;
    verifying_key.verify_strict(&signed_payload, &signature)?;

    // 4. For rotated keys, verify previous signature
    if sequence > 0 && current != genesis {
        verify_signature_with_key(&signed_payload, prev_sig, &previous_key)?;
    }

    Ok(())
}
```

### IID-Genesis Binding

**Critical security property**: The IID MUST be derived from the genesis key. This prevents:

- **IID Hijacking**: An attacker cannot publish a document claiming someone else's IID
- **Key Substitution**: The genesis key permanently anchors the identity

```
┌─────────────────────────────────────────────────────────────────┐
│                  IID-Genesis Binding Verification                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Document claims:                                                │
│    iid: "b1n7cfscgashm32xx7eaxw0y09gy0y2v"                      │
│    keys.signing.genesis: "base64_key_here..."                   │
│                                                                  │
│  Verification:                                                   │
│    1. Decode genesis key from base64                            │
│    2. Compute: expected_iid = derive_iid(genesis_key)           │
│    3. Check: expected_iid == document.iid                       │
│                                                                  │
│  If mismatch: REJECT document (attempted hijack)                │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Chain Verification

When verifying document updates (lines 1263-1305):

```rust
pub fn verify_document_extends(previous: &IdentityDocument,
                                current: &IdentityDocument) -> Result<()> {
    // 1. IID must be unchanged
    if current.iid != previous.iid { return Err(...); }

    // 2. Sequence must increase
    if curr_seq <= prev_seq { return Err(...); }

    // 3. Verify current signature
    IdentityManager::verify_document(current)?;

    // 4. If key changed, verify rotation
    if current.keys.signing.current != previous.keys.signing.current {
        // REQ-IDOC-020: previous key must be present
        // REQ-IDOC-021: previous key must match old doc's current
        // REQ-IDOC-022: previous signature must verify
    }

    Ok(())
}
```

### Signature Format

Signatures use Ed25519 with domain separation (lines 873-890):

```rust
const IDOC_DOMAIN_SEPARATOR: &[u8] = b"post-urbit:idoc:v1:";

fn signature_payload(document: &IdentityDocument) -> Result<Vec<u8>> {
    // Remove signatures field from document
    let mut value = serde_json::to_value(document)?;
    value.as_object_mut().unwrap().remove("signatures");

    // Create canonical JSON
    let canonical = canonical_json_value(&value)?;

    // Prepend domain separator
    let mut out = Vec::new();
    out.extend_from_slice(IDOC_DOMAIN_SEPARATOR);
    out.extend_from_slice(canonical.as_bytes());
    Ok(out)
}
```

### Domain Separators

Different document types use different domain separators:

| Domain | Separator |
|--------|-----------|
| Identity document | `post-urbit:idoc:v1:` |
| Key revocation | `post-urbit:key-revocation:v1:` |
| Identity revocation | `post-urbit:identity-revocation:v1:` |
| Device revocation | `post-urbit:device-revocation:v1:` |
| Device document | `post-urbit:device-doc:v1:` |
| Device index | `post-urbit:device-index:v1:` |
| Recovery attestation | `post-urbit:recovery-attestation:v1:` |

---

## Bootstrap/TOFU

### Trust On First Use (TOFU)

When encountering an identity for the first time, Post-Urbit uses TOFU to establish initial trust. The `bootstrap_verify` function (lines 1307-1414) implements this:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Bootstrap Verification                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Input: iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v"                │
│                                                                  │
│  Step 1: Fetch genesis document                                 │
│  ┌──────────────────────────────────────────────┐               │
│  │  key = dht_key_genesis(iid)                  │               │
│  │  genesis_doc = dht.get(key)                  │               │
│  └──────────────────────────────────────────────┘               │
│                             │                                    │
│                             ▼                                    │
│  Step 2: Verify genesis document                                │
│  ┌──────────────────────────────────────────────┐               │
│  │  - sequence == "0"                           │               │
│  │  - genesis == current key                    │               │
│  │  - IID derived from genesis key              │               │
│  │  - Signature valid                           │               │
│  └──────────────────────────────────────────────┘               │
│                             │                                    │
│                             ▼                                    │
│  Step 3: Fetch latest document                                  │
│  ┌──────────────────────────────────────────────┐               │
│  │  key = dht_key_identity(iid)                 │               │
│  │  latest_doc = dht.get(key) with highest seq  │               │
│  └──────────────────────────────────────────────┘               │
│                             │                                    │
│                             ▼                                    │
│  Step 4: Verify chain integrity                                 │
│  ┌──────────────────────────────────────────────┐               │
│  │  - latest.genesis == genesis.current         │               │
│  │  - All signatures valid                      │               │
│  │  - Key rotation properly authorized          │               │
│  └──────────────────────────────────────────────┘               │
│                             │                                    │
│                             ▼                                    │
│  Return: verified IdentityDocument                              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### TOFU Fallback

If no genesis document is available (REQ-IDOC-027, line 1407):

```rust
// TOFU case: No genesis available, accept first encountered
if IdentityManager::verify_document(&latest).is_ok() {
    return Ok(latest);
}
```

This allows identities to work even if the genesis document is temporarily unavailable, while still verifying:
- IID derived from genesis key in the document
- Current signature is valid
- Document is internally consistent

### Security Considerations

1. **First contact**: Store the genesis key on first successful bootstrap
2. **Subsequent contacts**: Verify new documents against stored genesis
3. **Conflict detection**: Reject documents with different genesis for same IID
4. **Key continuity**: Verify complete chain from genesis to current

---

## Code Examples

### Creating a New Identity

```rust
use post_urbit_core::identity::IdentityManager;

async fn create_identity() -> Result<()> {
    // Create identity manager (generates keys if none exist)
    let manager = IdentityManager::new("/path/to/data").await?;

    // Get your IID
    let iid = manager.iid().await;
    println!("Your IID: {}", iid);

    // Get your identity document
    let doc = manager.identity_document().await;
    println!("Document sequence: {}", doc.sequence);

    Ok(())
}
```

### Updating Profile Claims

```rust
async fn update_profile(manager: &IdentityManager) -> Result<()> {
    let updated = manager.update_claims(
        Some("Alice".to_string()),           // name
        Some("https://example.com/avatar.png".to_string()),  // avatar
        Some("Post-Urbit enthusiast".to_string()),  // bio
    ).await?;

    println!("Updated to sequence: {}", updated.sequence);
    Ok(())
}
```

### Rotating Keys

```rust
async fn rotate_keys(manager: &IdentityManager) -> Result<()> {
    // Rotate signing key
    let result = manager.rotate_signing_key().await?;
    println!("New signing key fingerprint: {}", result.new_key_fingerprint);
    println!("Previous key fingerprint: {}", result.previous_key_fingerprint);

    // Rotate encryption key
    let result = manager.rotate_encryption_key().await?;
    println!("New encryption key fingerprint: {}", result.new_key_fingerprint);

    Ok(())
}
```

### Setting Up Social Recovery

```rust
use post_urbit_core::identity::{Recovery, IdentityManager};
use serde_json::json;

async fn setup_recovery(manager: &IdentityManager) -> Result<()> {
    let recovery = Recovery {
        method: "social".to_string(),
        config: json!({
            "threshold": 2,
            "trustees": [
                {"iid": "trustee1iidhere...", "label": "Alice"},
                {"iid": "trustee2iidhere...", "label": "Bob"},
                {"iid": "trustee3iidhere...", "label": "Carol"}
            ],
            "cooldown_hours": 72
        }),
    };

    let updated = manager.update_recovery(recovery).await?;
    println!("Recovery configured at sequence: {}", updated.sequence);

    Ok(())
}
```

### Verifying an Identity Document

```rust
use post_urbit_core::identity::{IdentityManager, IdentityDocument};

fn verify_identity(doc: &IdentityDocument) -> Result<()> {
    // Full verification
    IdentityManager::verify_document(doc)?;

    println!("Document verified successfully");
    println!("  IID: {}", doc.iid);
    println!("  Sequence: {}", doc.sequence);
    println!("  Current key: {}", doc.keys.signing.current);

    Ok(())
}
```

### Fetching an Identity from DHT

```rust
use post_urbit_core::identity::{fetch_identity, bootstrap_verify};
use post_urbit_core::dht::Dht;

async fn lookup_identity(dht: &dyn Dht, iid: &str) -> Result<IdentityDocument> {
    // Simple fetch (returns highest valid sequence)
    if let Some(doc) = fetch_identity(dht, iid).await? {
        return Ok(doc);
    }

    // Or use bootstrap_verify for TOFU
    let doc = bootstrap_verify(dht, iid).await?;
    Ok(doc)
}
```

### Publishing Identity to DHT

```rust
use post_urbit_core::identity::{publish_genesis, publish_identity};
use post_urbit_core::dht::Dht;

async fn publish(dht: &dyn Dht, manager: &IdentityManager) -> Result<()> {
    let doc = manager.identity_document().await;

    if doc.sequence == "0" {
        // First-time publish: store genesis
        publish_genesis(dht, &doc).await?;
    } else {
        // Subsequent updates
        publish_identity(dht, &doc).await?;
    }

    Ok(())
}
```

### Creating a Recovery Attestation (as a Trustee)

```rust
use post_urbit_core::identity::{RecoveryAttestation, sign_recovery_attestation};
use chrono::Utc;

async fn create_attestation(
    manager: &IdentityManager,
    target_iid: &str,
    new_signing_key: &str,
) -> Result<RecoveryAttestation> {
    let trustee_iid = manager.iid().await;

    let mut attestation = RecoveryAttestation {
        target_iid: target_iid.to_string(),
        trustee_iid,
        new_signing_key: new_signing_key.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        signature: String::new(),
    };

    // Sign with our signing key
    let state = manager.inner.read().await;
    attestation.signature = sign_recovery_attestation(&attestation, &state.signing_key)?;

    Ok(attestation)
}
```

### Verifying a Social Recovery

```rust
use post_urbit_core::identity::{verify_social_recovery, RecoveryAttestation};
use post_urbit_core::dht::Dht;

async fn verify_recovery(
    dht: &dyn Dht,
    target_iid: &str,
    attestations: &[RecoveryAttestation],
) -> Result<String> {
    let new_signing_key = verify_social_recovery(dht, target_iid, attestations).await?;

    println!("Recovery verified! New signing key: {}", new_signing_key);

    Ok(new_signing_key)
}
```

### Signing Arbitrary Data

```rust
async fn sign_message(manager: &IdentityManager, message: &[u8]) -> Result<String> {
    // Sign and get base64 signature
    let signature = manager.sign_data_base64(message).await;

    // Get your public key for verification
    let public_key = manager.signing_public_key_base64().await;

    println!("Message signed with key: {}", public_key);
    println!("Signature: {}", signature);

    Ok(signature)
}
```

---

## Key Files Reference

| File | Description |
|------|-------------|
| `src/identity.rs` | Core identity system implementation |
| `src/encoding.rs` | Base64 and Crockford Base32 encoding |
| `src/canonical_json.rs` | Deterministic JSON serialization |
| `src/dht.rs` | DHT key derivation and storage |

### Local Storage Files

When `IdentityManager` is initialized with a data directory:

| File | Contents |
|------|----------|
| `identity_signing.key` | Ed25519 private key (32 bytes) |
| `identity_encryption.key` | X25519 private key (32 bytes) |
| `identity.json` | Current identity document |
| `identity_meta.json` | Key validity metadata |

---

## Further Reading

- [Architecture Overview](./README.md)
- [Transport & Connections](./transport.md)
- [Building Apps Guide](./apps/building-apps.md)
