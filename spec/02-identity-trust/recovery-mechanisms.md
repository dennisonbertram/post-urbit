# Recovery Mechanisms

## Overview

Recovery mechanisms allow users to regain control of their identity after losing access to their signing keys (e.g., device loss, forgotten passwords, hardware failure).

**Critical principle**: Recovery is configured BEFORE it's needed. The recovery configuration is embedded in the Identity Document.

## Recovery Methods

| Method | Description | Trust Model |
|--------|-------------|-------------|
| `none` | No recovery possible | Self-sovereign, high risk |
| `social` | Trusted contacts can authorize recovery | Trust in friends |
| `device-escrow` | Backup device holds recovery key | Trust in own hardware |
| `threshold` | M-of-N key shares | Distributed trust |
| `provider` | Third-party recovery service | Trust in provider |

## Method: `none`

No recovery configured. If keys are lost, identity is lost forever.

```json
{
  "method": "none",
  "config": {}
}
```

**Use case**: Temporary or disposable identities, extreme privacy requirements.

**Warning**: Should require explicit user acknowledgment.

## Method: `social`

Trusted contacts can collectively authorize recovery of the identity.

### Configuration

```json
{
  "method": "social",
  "config": {
    "threshold": 3,
    "trustees": [
      {
        "iid": "a1b2c3d4...",
        "label": "Alice (sister)"
      },
      {
        "iid": "e5f6g7h8...",
        "label": "Bob (friend)"
      },
      {
        "iid": "i9j0k1l2...",
        "label": "Carol (colleague)"
      },
      {
        "iid": "m3n4o5p6...",
        "label": "Dave (lawyer)"
      }
    ],
    "cooldown_hours": 72
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `threshold` | uint8 | Minimum trustees required to authorize (M) |
| `trustees` | array | List of trusted identities (N, max 10) |
| `trustees[].iid` | string | Trustee's Identity Identifier |
| `trustees[].label` | string | Human-readable label (for owner's reference) |
| `cooldown_hours` | uint16 | Waiting period before recovery executes |

### Recovery Protocol

1. **Initiate**: User contacts trustees out-of-band, provides new public keys
2. **Trustee signs recovery request**:
   ```json
   {
     "type": "recovery_attestation",
     "subject_iid": "<iid-being-recovered>",
     "new_signing_key": "<base64-new-public-key>",
     "new_encryption_key": "<base64-new-public-key>",
     "trustee_iid": "<trustee's-iid>",
     "timestamp": "<RFC3339>",
     "signature": "<Ed25519-sig-by-trustee>"
   }
   ```
3. **Collect attestations**: Gather `threshold` valid attestations
4. **Publish recovery document**:
   ```json
   {
     "version": 1,
     "iid": "<unchanged>",
     "sequence": <previous + 1>,
     "timestamp": "<now>",
     "keys": {
       "signing": {
         "current": "<new-key>",
         "previous": null
       },
       "encryption": {
         "current": "<new-key>",
         "previous": null
       }
     },
     "recovery_proof": {
       "attestations": [ <array of trustee attestations> ],
       "initiated_at": "<timestamp>"
     },
     "signatures": {
       "current": "<sig-by-new-key>",
       "previous": null
     }
   }
   ```
5. **Cooldown period**: Document is "pending" for `cooldown_hours`
6. **Activation**: After cooldown, document becomes active

### Verification

```
function verify_social_recovery(old_doc, new_doc):
    assert old_doc.recovery.method == "social"
    assert new_doc.recovery_proof != null

    config = old_doc.recovery.config
    attestations = new_doc.recovery_proof.attestations

    # Verify threshold met
    valid_attestations = 0
    for att in attestations:
        # Verify trustee is in config
        assert att.trustee_iid in config.trustees[].iid

        # Verify trustee signature
        trustee_doc = fetch_identity(att.trustee_iid)
        assert Ed25519_Verify(trustee_doc.keys.signing.current, att, att.signature)

        # Verify attestation is for correct subject and keys
        assert att.subject_iid == new_doc.iid
        assert att.new_signing_key == new_doc.keys.signing.current

        valid_attestations += 1

    assert valid_attestations >= config.threshold

    # Verify cooldown (if checking during cooldown, reject activation)
    initiated = parse_time(new_doc.recovery_proof.initiated_at)
    cooldown = config.cooldown_hours * 3600
    if now() < initiated + cooldown:
        return PENDING_COOLDOWN

    return VALID
```

### Attack Mitigation

- **Collusion**: Requires out-of-band trustee coordination
- **Trustee compromise**: Need `threshold` compromised trustees
- **Cooldown**: Gives real owner time to notice and contest

## Method: `device-escrow`

A secondary device holds a recovery key that can authorize new keys.

### Configuration

```json
{
  "method": "device-escrow",
  "config": {
    "escrow_key_hash": "<SHA256-of-escrow-public-key>",
    "device_label": "Backup phone in safe"
  }
}
```

### Recovery Protocol

1. **Access escrow device** (physically retrieve backup device)
2. **Generate new keys** on new primary device
3. **Sign recovery with escrow key**:
   ```json
   {
     "type": "escrow_recovery",
     "subject_iid": "<iid>",
     "new_signing_key": "<base64>",
     "new_encryption_key": "<base64>",
     "timestamp": "<RFC3339>",
     "escrow_signature": "<sig-by-escrow-key>"
   }
   ```
4. **Publish recovery document** (no cooldown required)

### Security Note

Escrow key should be stored securely (hardware security module, secure enclave, or offline device).

## Method: `threshold`

Shamir's Secret Sharing splits a recovery key into N shares, requiring M to reconstruct.

### Configuration

```json
{
  "method": "threshold",
  "config": {
    "threshold": 3,
    "total_shares": 5,
    "share_commitments": [
      "<hash-of-share-1>",
      "<hash-of-share-2>",
      "<hash-of-share-3>",
      "<hash-of-share-4>",
      "<hash-of-share-5>"
    ],
    "recovery_key_hash": "<hash-of-reconstructed-key>"
  }
}
```

### Setup Protocol

1. **Generate recovery keypair**: `K_recovery = Ed25519_Generate()`
2. **Split private key**: `shares = Shamir_Split(K_recovery_private, threshold, total_shares)`
3. **Distribute shares**: Give shares to trustees/devices/locations
4. **Record commitments**: Hash each share for verification
5. **Embed in identity document**

### Recovery Protocol

1. **Collect shares**: Gather `threshold` shares
2. **Reconstruct**: `K_recovery_private = Shamir_Reconstruct(shares)`
3. **Verify**: Check reconstructed key matches `recovery_key_hash`
4. **Sign recovery document** with reconstructed key
5. **Destroy reconstructed key** after use

## Method: `provider`

Third-party service assists with recovery (e.g., employer, identity provider).

### Configuration

```json
{
  "method": "provider",
  "config": {
    "provider_iid": "<provider's-identity>",
    "provider_endpoint": "https://recovery.example.com/api",
    "policy": "kyc|email|phone",
    "cooldown_hours": 168
  }
}
```

### Recovery Protocol

1. **Contact provider** through their designated channel
2. **Complete verification** per provider's policy (KYC, email, phone)
3. **Provider signs recovery attestation**
4. **Cooldown period** (typically longer, e.g., 7 days)
5. **Recovery activates**

### Trust Considerations

- Provider cannot unilaterally recover (needs user to initiate)
- Provider cannot forge recovery (their attestation is one input)
- Consider combining with social recovery for defense in depth

## Recovery Document Format

When recovery is used, the new Identity Document includes proof:

```json
{
  "version": 1,
  "iid": "<unchanged>",
  "sequence": <N+1>,
  "timestamp": "<now>",
  "keys": {
    "signing": {
      "current": "<new-key>",
      "previous": null
    },
    "encryption": {
      "current": "<new-key>",
      "previous": null
    }
  },
  "recovery_proof": {
    "method": "social|device-escrow|threshold|provider",
    "initiated_at": "<timestamp>",
    "proof_data": { <method-specific-proof> }
  },
  "recovery": { <new-recovery-config-for-future> },
  "signatures": {
    "current": "<sig-by-new-key>",
    "previous": null
  }
}
```

**Note**: `signatures.previous` is null because old key is unavailable. The `recovery_proof` substitutes for it.

## Cooldown and Contestation

### Cooldown Purpose

- Gives legitimate owner time to notice unauthorized recovery attempt
- Allows contestation before recovery finalizes

### Contestation

During cooldown, if the legitimate owner still has their keys:

```json
{
  "type": "recovery_contest",
  "iid": "<identity>",
  "contested_sequence": <N+1>,
  "reason": "I still have access to my keys",
  "timestamp": "<now>",
  "signature": "<sig-by-current-valid-key>"
}
```

If valid contest received:
- Recovery is cancelled
- Recovery configuration should be updated (trustees may be compromised)

## Interfaces

```typescript
interface RecoveryConfig {
  method: 'none' | 'social' | 'device-escrow' | 'threshold' | 'provider';
  config: SocialConfig | DeviceEscrowConfig | ThresholdConfig | ProviderConfig | {};
}

interface RecoveryManager {
  // Configure recovery method
  configureRecovery(
    identity: IdentityDocument,
    config: RecoveryConfig
  ): IdentityDocument;

  // Initiate recovery (creates pending recovery)
  initiateRecovery(
    iid: string,
    newSigningKey: PublicKey,
    newEncryptionKey: PublicKey
  ): PendingRecovery;

  // Add attestation/proof to pending recovery
  addRecoveryProof(
    pending: PendingRecovery,
    proof: RecoveryAttestation | EscrowSignature | ReconstructedKey
  ): PendingRecovery;

  // Check if recovery has enough proofs
  isRecoveryReady(pending: PendingRecovery): boolean;

  // Finalize and publish recovery
  executeRecovery(
    pending: PendingRecovery,
    newSigningPrivate: PrivateKey
  ): Promise<RecoveryResult>;

  // Contest a pending recovery
  contestRecovery(
    pendingSequence: number,
    currentSigningPrivate: PrivateKey,
    reason: string
  ): Promise<ContestResult>;

  // Verify a recovery document
  verifyRecovery(
    oldDoc: IdentityDocument,
    newDoc: IdentityDocument
  ): RecoveryVerificationResult;
}
```

## Security Considerations

1. **Recovery as attack vector**: Recovery mechanisms are prime targets. Balance usability with security.
2. **Trustee selection**: For social recovery, choose trustees who are unlikely to collude and who can be reached out-of-band.
3. **Share storage**: For threshold, store shares in physically separate locations.
4. **Provider trust**: For provider method, understand their policies and failure modes.
5. **Regular testing**: Periodically verify recovery mechanism still works (e.g., trustees still responsive).
6. **Cooldown length**: Longer cooldowns are more secure but less convenient. Default 72 hours for social, 168 hours for provider.

## Test Scenarios

1. **Social recovery happy path**: 3-of-5 trustees attest, cooldown passes, recovery succeeds
2. **Social recovery contested**: Legitimate owner contests during cooldown, recovery cancelled
3. **Threshold recovery**: 3 shares collected, key reconstructed, recovery succeeds
4. **Device escrow**: Backup device signs recovery, immediate activation
5. **Provider recovery**: KYC completed, provider attests, long cooldown, recovery succeeds
6. **Collusion attack**: 2 malicious trustees attempt recovery, threshold not met, rejected
