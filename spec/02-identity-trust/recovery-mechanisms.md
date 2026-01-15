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

### V1 Conformance Requirements

For Post-Urbit v1 implementations:

| Method | Status | Notes |
|--------|--------|-------|
| `none` | REQUIRED | All implementations MUST support `none` |
| `social` | REQUIRED | All implementations MUST support `social` recovery |
| `device-escrow` | OPTIONAL | MAY be implemented; signature scheme identical to `social` but with single "trustee" |
| `threshold` | OPTIONAL | MAY be implemented; Shamir's Secret Sharing library required |
| `provider` | OPTIONAL | MAY be implemented; requires trust in third-party |

**Minimum v1 Conformance:** Implementations MUST support `none` and `social` recovery methods. Users SHOULD configure at least `social` recovery for any identity they care about retaining.

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
        "iid": "f9j0k1m2...",
        "label": "Carol (colleague)"
      },
      {
        "iid": "m3n405p6...",
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
     "sequence": "<previous + 1>",
     "timestamp": "<now>",
     "keys": {
       "signing": {
         "genesis": "<base64-genesis-key-PRESERVED>",
         "current": "<new-key>",
         "previous": null,
         "history": []
       },
       "encryption": {
         "current": "<new-key>",
         "previous": []
       }
     },
     "endpoints": [],
     "recovery": {"method": "social", "config": {"threshold": 3, "trustees": [...], "cooldown_hours": 72}},
     "claims": {},
     "extensions": {},
     "recovery_proof": {
       "method": "social",
       "initiated_at": "<timestamp>",
       "cooldown_expires_at": "<timestamp + cooldown_hours>",
       "status": "pending",
       "proof_data": {
         "attestations": [ "<array of trustee attestations>" ]
       }
     },
     "signatures": {
       "current": "<sig-by-new-key>",
       "previous": null
     }
   }
   ```

   **Note:** `keys.signing.genesis` is ALWAYS preserved (never changes). This allows verifiers to confirm the IID derivation even after recovery. The `encryption.previous` array starts empty after recovery since old encryption keys are typically unrecoverable.
5. **Cooldown period**: Document is published with `status: "pending"`
6. **Activation**: After `cooldown_expires_at`, verifiers MUST treat the document as active

**Recovery Status Semantics (Normative):**

The `recovery_proof.status` field is **informational only**. Verifiers MUST NOT rely on the `status` value to determine validity. Instead:

- Verifiers MUST check: `now() >= cooldown_expires_at`
- If true: document is active (regardless of `status` field value)
- If false: document is pending (reject or queue)

**Rationale:** Identity documents are immutable once published (sequence is the ordering primitive). Requiring republication to flip `status` from "pending" to "active" would:
- Require a sequence bump for no meaningful change
- Create race conditions if the owner publishes other updates during cooldown
- Complicate verification unnecessarily

The `status` field exists for informational purposes (e.g., UI display, debugging) but has no normative effect on verification.

### Verification

```
function verify_social_recovery(old_doc, new_doc):
    assert old_doc.recovery.method == "social"
    assert new_doc.recovery_proof != null
    assert new_doc.recovery_proof.method == "social"

    config = old_doc.recovery.config
    proof_data = new_doc.recovery_proof.proof_data
    attestations = proof_data.attestations

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

    # Verify cooldown (ALWAYS check timestamp, ignore status field)
    # CRITICAL: Do NOT trust the status field - it is informational only
    cooldown_expires = parse_time(new_doc.recovery_proof.cooldown_expires_at)
    if now() < cooldown_expires:
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
  "sequence": "<N+1>",
  "timestamp": "<now>",
  "keys": {
    "signing": {
      "genesis": "<base64-genesis-key-PRESERVED>",
      "current": "<new-key>",
      "previous": null,
      "history": []
    },
    "encryption": {
      "current": "<new-key>",
      "previous": []
    }
  },
  "endpoints": [ "<preserved-or-updated-endpoints>" ],
  "recovery": { "<new-recovery-config-for-future>" },
  "claims": {},
  "extensions": {},
  "recovery_proof": {
    "method": "social|device-escrow|threshold|provider",
    "initiated_at": "<timestamp>",
    "cooldown_expires_at": "<timestamp>",
    "status": "pending|active|contested",
    "proof_data": { "<method-specific-proof>" }
  },
  "signatures": {
    "current": "<sig-by-new-key>",
    "previous": null
  }
}
```

**Notes:**
- `signatures.previous` is null because old key is unavailable. The `recovery_proof` substitutes for it.
- `keys.signing.genesis` MUST be preserved unchanged (IID is derived from it).
- `keys.encryption.previous` is an array (empty after recovery; old keys unrecoverable).

## Cooldown and Contestation

### Cooldown Purpose

- Gives legitimate owner time to notice unauthorized recovery attempt
- Allows contestation before recovery finalizes

### Contestation

**Normative (RFC-0001 §9.6):** Contestation is performed by publishing a higher-sequence IDOC update signed with the original key during the cooldown period. If valid, this supersedes the recovery attempt.

**Experimental (Non-Normative for v1):** The contest document mechanism below is a proposed extension for explicit contestation signaling. Implementations MUST NOT treat contest documents as affecting identity validity in v1. The authoritative contestation method is the RFC-0001 §9.6 approach (higher-sequence IDOC update).

---

**[EXPERIMENTAL] Contest Document Format:**

During cooldown, if the legitimate owner still has their keys they can optionally publish a contest document (in addition to the required higher-sequence IDOC):

**Contest Document Format:**
```json
{
  "type": "recovery_contest",
  "iid": "<identity-identifier>",
  "contested_sequence": "<N+1>",
  "reason": "I still have access to my keys",
  "timestamp": "<RFC3339-UTC>",
  "signature": "<base64-sig-by-current-valid-key>"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | MUST be `"recovery_contest"` |
| `iid` | string | IID of identity being contested |
| `contested_sequence` | string | Decimal string of recovery document's sequence number |
| `reason` | string | Human-readable reason (max 256 chars) |
| `timestamp` | string | RFC3339 UTC timestamp |
| `signature` | string | Base64 Ed25519 signature |

**Contest Signature Scheme:**
```
signature_input = concat(
  "post-urbit:recovery-contest:v1:",  // domain separator (31 bytes)
  JCS(contest_doc_without_signature)
)
signature = Ed25519_Sign(current_signing_key, signature_input)
```

**DHT Publication:**
- DHT Key: `SHA256("post-urbit:contest:" || iid)` (19-byte prefix + IID)
- DHT Value: JCS-canonicalized contest document (UTF-8 JSON)
- TTL: 168 hours (matches typical cooldown period)

**Contest Verification:**
1. Fetch the current identity document (before recovery)
2. Verify `signature` using the current document's `keys.signing.current`
3. Verify `contested_sequence` matches the pending recovery document's sequence
4. Verify `timestamp` is after the recovery's `initiated_at`

If valid contest received:
- Recovery is cancelled
- Recovery configuration should be updated (trustees may be compromised)

## Interfaces

**Note:** The authoritative interface definitions are in `spec/02-identity-trust/interfaces.md`. The summary below is provided for convenience but MUST match the canonical definitions.

```typescript
// See interfaces.md for full type definitions including:
// - RecoveryConfig, SocialRecoveryConfig, DeviceEscrowConfig, ThresholdConfig, ProviderConfig
// - RecoveryService interface with all recovery operations
// - PendingRecovery, RecoveryProof, RecoveryAttestation, RecoveryResult, ContestResult

interface RecoveryConfig {
  method: 'none' | 'social' | 'device-escrow' | 'threshold' | 'provider';
  config: SocialRecoveryConfig | DeviceEscrowConfig | ThresholdConfig | ProviderConfig | {};
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
