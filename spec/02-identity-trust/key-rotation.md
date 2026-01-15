# Key Rotation Protocol

## Overview

Key rotation allows users to change their cryptographic keys while maintaining identity continuity. This is essential for:

- Routine security hygiene (periodic rotation)
- Responding to suspected compromise
- Migrating to new devices
- Upgrading to stronger algorithms (future)

## Rotation Types

| Type | Keys Changed | Use Case |
|------|--------------|----------|
| **Signing rotation** | Signing key only | Suspected signing key exposure |
| **Encryption rotation** | Encryption key only | Forward secrecy improvement |
| **Full rotation** | Both keys | Device migration, full refresh |
| **Endpoint update** | No keys | Network change (new IP, relay) |

## Protocol Flow

### Pre-Rotation State

```
Identity Document v(N):
  - sequence: N
  - keys.signing.current: K_sign_old
  - keys.encryption.current: K_enc_old
  - signatures.current: Sig(K_sign_old, doc)
```

### Rotation Steps

1. **Generate new keys** (offline, on secure device)
   ```
   K_sign_new = Ed25519_Generate()
   K_enc_new = X25519_Generate()
   ```

2. **Construct new document**
   ```json
   {
     "version": 1,
     "iid": "<unchanged>",
     "sequence": "N + 1",
     "timestamp": "<now>",
     "keys": {
       "signing": {
         "genesis": "<K_sign_genesis>",
         "current": "<K_sign_new>",
         "previous": "<K_sign_old>",
         "history": []
       },
       "encryption": {
         "current": "<K_enc_new>",
         "previous": [
           {
             "key": "<K_enc_old>",
             "valid_from": "0",
             "valid_until": "1",
             "expires_at": "<RFC3339-timestamp>"
           }
         ]
       }
     },
     "endpoints": [],
     "recovery": {"method": "none", "config": {}},
     "claims": {},
     "extensions": {},
     "recovery_proof": null,
     "signatures": {
       "current": "<sig-by-K_sign_new>",
       "previous": "<sig-by-K_sign_old>"
     }
   }
   ```

   **Note:** `keys.encryption.previous` is an array of history entries (not a single key string). Each entry includes `key`, `valid_from` (sequence number when key became current), `valid_until` (sequence number when rotated out), and `expires_at` (timestamp).

3. **Sign with BOTH keys**
   ```
   canonical = JCS(doc_without_signatures)
   payload = b"post-urbit:idoc:v1:" + canonical  # Domain separation
   sig_current = Ed25519_Sign(K_sign_new_private, payload)
   sig_previous = Ed25519_Sign(K_sign_old_private, payload)
   ```

4. **Publish new document**
   - Push to all known peers
   - Update any directory/DHT registrations
   - Store locally as current version

### Post-Rotation State

```
Identity Document v(N+1):
  - sequence: N + 1
  - keys.signing.current: K_sign_new
  - keys.signing.previous: K_sign_old
  - signatures.current: Sig(K_sign_new, doc)
  - signatures.previous: Sig(K_sign_old, doc)
```

## Verification by Recipients

When a peer receives an updated Identity Document:

```
function verify_rotation(old_doc, new_doc):
    # 1. IID must match
    assert new_doc.iid == old_doc.iid

    # 2. Sequence must increase
    assert int(new_doc.sequence) > int(old_doc.sequence)

    # 3. Current signature must be valid (with domain separation)
    canonical = JCS(new_doc without signatures)
    payload = b"post-urbit:idoc:v1:" + canonical
    assert Ed25519_Verify(new_doc.keys.signing.current, payload, new_doc.signatures.current)

    # 4. If signing key changed, previous signature required
    if new_doc.keys.signing.current != old_doc.keys.signing.current:
        assert new_doc.signatures.previous != null
        assert Ed25519_Verify(old_doc.keys.signing.current, payload, new_doc.signatures.previous)

    return VALID
```

## State Machine

```
                    ┌──────────────────────┐
                    │                      │
                    ▼                      │
┌─────────┐    ┌─────────┐    ┌─────────┐  │
│ CURRENT │───►│ROTATING │───►│ CURRENT │──┘
│   (N)   │    │         │    │  (N+1)  │
└─────────┘    └─────────┘    └─────────┘
                    │
                    │ failure
                    ▼
               ┌─────────┐
               │ ROLLBACK│ (stay at N)
               └─────────┘
```

### States

| State | Description |
|-------|-------------|
| `CURRENT(N)` | Active identity at sequence N |
| `ROTATING` | New document created, being propagated |
| `CURRENT(N+1)` | Rotation complete, new keys active |
| `ROLLBACK` | Rotation failed, revert to previous |

## Propagation Strategy

After creating a rotated document:

1. **Immediate push** to all connected peers
2. **Update DHT/directory** registrations
3. **Notify relay/mailbox** services
4. **Retain old encryption key** for 30 days (decrypt old messages)

### Propagation Message

The wire format for identity updates uses the schema defined in `spec/00-shared/layer-integration.md`:

```json
{
  "type": "identity_update",
  "idoc": "<base64-standard-no-padding-of-IDOC-envelope>",
  "sequence": "<decimal-string>",
  "sent_at": "<RFC3339-UTC-canonical>"
}
```

**Note:** The `idoc` field contains the Base64-encoded IDOC binary envelope (magic + version + length + JCS JSON), NOT an inline JSON object. This ensures signature verification uses the exact bytes that were signed.

For rotation urgency hints, implementations MAY use out-of-band signaling or extended fields (future versions may standardize urgency/reason fields), but the core wire format above is sufficient for v1.

| Urgency | Meaning |
|---------|---------|
| `normal` | Routine rotation, process when convenient |
| `urgent` | Suspected compromise, verify and update immediately |

## Handling Old Keys

### Signing Key (Old)

- **Discard private key immediately** after rotation completes
- **Retain public key** in `keys.signing.previous` for one rotation cycle
- Peers use previous key to verify the rotation was authorized

### Encryption Key (Old)

- **Retain private key for 30 days** to decrypt in-flight messages
- **Retain public key** in `keys.encryption.previous` indefinitely
- Senders may encrypt to previous key if they haven't received update

## Concurrent Updates (Conflict Handling)

If two devices attempt rotation simultaneously:

1. Both create documents with `sequence = N + 1`
2. Network will see conflicting documents with same sequence number
3. **Resolution**: TOFU (trust-on-first-use) with manual fallback

**Resolution rules** (aligned with identity-document-schema.md):
- **Same canonical content**: No conflict, documents are identical
- **First-seen wins**: Each peer keeps the first valid document received
- **No timestamp tiebreaker**: Timestamp comparison is gameable by attackers
- **Manual resolution**: If conflict cannot be auto-resolved, mark identity as conflicted and notify user

### Prevention (Strongly Recommended)

- **Single-writer discipline**: Designate one device as primary for identity updates
- **Coordination**: Before rotating, fetch latest sequence and verify no concurrent updates
- **Advisory lock**: Post intent to rotate, wait 10 seconds for conflicts before publishing
- **Recovery fallback**: If conflict occurs, use recovery mechanism to create authoritative update with higher sequence

## Rotation Frequency Recommendations

| Scenario | Recommended Interval |
|----------|---------------------|
| Normal operation | Every 90 days |
| High-security use | Every 30 days |
| After device loss | Immediately |
| After suspected compromise | Immediately |

## API Interface

```typescript
interface KeyRotation {
  // Initiate rotation - returns new document for signing
  prepareRotation(
    currentDoc: IdentityDocument,
    rotationType: 'signing' | 'encryption' | 'full'
  ): {
    newDocument: UnsignedIdentityDocument;
    newSigningKey?: KeyPair;
    newEncryptionKey?: KeyPair;
  };

  // Sign the prepared document with both keys
  signRotation(
    preparedDoc: UnsignedIdentityDocument,
    currentSigningPrivate: PrivateKey,
    newSigningPrivate: PrivateKey
  ): SignedIdentityDocument;

  // Propagate to peers
  propagateRotation(
    signedDoc: SignedIdentityDocument,
    urgency: 'normal' | 'urgent'
  ): Promise<PropagationResult>;

  // Verify a received rotation
  verifyRotation(
    oldDoc: IdentityDocument,
    newDoc: IdentityDocument
  ): VerificationResult;
}

interface PropagationResult {
  peersNotified: number;
  peersAcknowledged: number;
  directoryUpdated: boolean;
  errors: PropagationError[];
}

interface VerificationResult {
  valid: boolean;
  error?: 'SEQUENCE_REGRESSION' | 'INVALID_SIGNATURE' | 'MISSING_PREVIOUS_SIG' | 'IID_MISMATCH';
}
```

## Error Handling

| Error | Cause | Recovery |
|-------|-------|----------|
| `SEQUENCE_CONFLICT` | Concurrent rotation | Fetch latest, retry with higher sequence |
| `PROPAGATION_PARTIAL` | Some peers unreachable | Retry unreachable peers, continue with reachable |
| `OLD_KEY_UNAVAILABLE` | Lost old signing key | Use recovery mechanism |
| `SIGNATURE_FAILED` | Key material corrupt | Regenerate keys, start fresh recovery |

## Security Considerations

1. **Old Key Destruction**: Securely erase old signing private key after rotation
2. **Atomic Operations**: Rotation should be all-or-nothing to prevent split-brain
3. **Timing**: Don't rotate during active sessions; complete pending messages first
4. **Backup**: Ensure recovery mechanism is configured before first rotation
5. **Verification**: Always verify both signatures when receiving rotated documents

## Test Scenarios

1. **Happy path**: Rotate keys, verify peers accept new document
2. **Stale peer**: Peer with old document receives update, verifies chain
3. **Skipped versions**: Peer at sequence 5 receives sequence 8, verifies final signatures
4. **Concurrent rotation**: Two devices rotate simultaneously, detect conflict
5. **Compromised old key**: Attacker with old key cannot forge new rotation (needs current key)
