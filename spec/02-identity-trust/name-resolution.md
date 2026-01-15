# Name Resolution

## Overview

Name resolution maps human-friendly names to Identity Identifiers (IIDs). This is intentionally separated from identity to allow multiple naming systems without coupling identity to any particular namespace.

## Design Principles

1. **Names are optional**: Identities work without names (IID is the canonical identifier)
2. **Names are hints**: Names help humans; cryptographic verification uses IIDs
3. **Multiple systems**: Support DNS, local aliases, and pluggable registries
4. **No speculation**: Avoid creating valuable "name real estate"

## Naming Layers

```
┌─────────────────────────────────────┐
│     Human Interface (display)       │
├─────────────────────────────────────┤
│     Alias Resolution (local)        │
├─────────────────────────────────────┤
│     DNS Resolution (global)         │
├─────────────────────────────────────┤
│     Registry Resolution (pluggable) │
├─────────────────────────────────────┤
│     IID (cryptographic truth)       │
└─────────────────────────────────────┘
```

## Layer 1: Local Aliases

User-defined mappings stored locally on each node.

### Alias Store

```json
{
  "aliases": {
    "alice": {
      "iid": "k5xq7z8m9n2p3r4s5t6v7v8w9x0y1z2a",
      "display_name": "Alice (work)",
      "added_at": "2025-01-13T12:00:00Z",
      "verified_via": "in_person|qr_code|trusted_intro"
    },
    "bob": {
      "iid": "m3n405p6q7r8s9t0v1v2w3x4y5z6a7b8",
      "display_name": "Bob",
      "added_at": "2025-01-10T08:00:00Z",
      "verified_via": "trusted_intro"
    }
  }
}
```

### Resolution

```
resolve("alice") → "k5xq7z8m9n2p3r4s5t6v7v8w9x0y1z2a"
```

### Collision Handling

- Local aliases are per-user; no global collisions
- If user tries to add duplicate alias, warn and require confirmation
- Aliases are case-insensitive (normalized to lowercase)

## Layer 2: DNS-Based Names

Use DNS TXT records to map domain names to IIDs.

### DNS Record Format

```
_identity.alice.example.com. TXT "post-urbit=1 iid=k5xq7z8m9n2p3r4s5t6v7v8w9x0y1z2a"
```

### Record Fields

| Field | Required | Description |
|-------|----------|-------------|
| `post-urbit` | Yes | Protocol version |
| `iid` | Yes | Identity Identifier |
| `endpoint` | No | Direct endpoint hint |
| `relay` | No | Preferred relay |

### Resolution Protocol

```
function resolve_dns(name):
    # Example: "alice.example.com"
    txt_name = "_identity." + name

    records = dns_query(txt_name, "TXT")

    for record in records:
        parsed = parse_post_urbit_record(record)
        if parsed and parsed.version == 1:
            return {
                iid: parsed.iid,
                endpoint_hint: parsed.endpoint,
                relay_hint: parsed.relay,
                source: "dns",
                ttl: record.ttl
            }

    return NOT_FOUND
```

### DNSSEC Requirement

- **SHOULD** use DNSSEC for integrity
- If DNSSEC validation fails, treat result as untrusted
- Still verify identity document signature after resolution

### Subdomain Pattern

Recommended: Use subdomains rather than base domains

```
alice.users.example.com  → Alice's identity
bob.users.example.com    → Bob's identity
```

This allows organizations to manage user identities under their domain.

## Layer 3: Pluggable Registries

Support for decentralized name registries (ENS-like, but not required).

### Registry Interface

```typescript
interface NameRegistry {
  // Registry metadata
  readonly name: string;
  readonly endpoint: string;

  // Resolve name to IID
  resolve(name: string): Promise<RegistryResult | null>;

  // Reverse lookup (optional)
  reverseResolve(iid: string): Promise<string | null>;

  // Check name availability (for registration)
  isAvailable(name: string): Promise<boolean>;

  // Register name (if registry supports it)
  register(name: string, iid: string, proof: RegistrationProof): Promise<RegistrationResult>;
}

interface RegistryResult {
  iid: string;
  name: string;
  registeredAt: Date;
  expiresAt?: Date;
  metadata?: Record<string, string>;
}
```

### Registry Configuration

Users configure which registries to use:

```json
{
  "name_registries": [
    {
      "name": "local",
      "type": "local_aliases",
      "priority": 0
    },
    {
      "name": "dns",
      "type": "dns",
      "priority": 1
    },
    {
      "name": "ens",
      "type": "plugin",
      "endpoint": "https://ens-resolver.example.com",
      "priority": 2,
      "enabled": false
    }
  ]
}
```

### Resolution Order

1. Check local aliases first (instant, trusted)
2. Check DNS (fast, widely available)
3. Check enabled registries in priority order

## Display Names vs. Verified Names

### Display Name

Self-asserted name in the Identity Document `claims.name` field.

- **Source**: User sets it themselves
- **Trust**: None - anyone can claim any name
- **Display**: Show with caveat (e.g., italic, "self-reported")

### Verified Name

Name resolved through a naming system (DNS, registry).

- **Source**: External system with its own rules
- **Trust**: Depends on the system (DNS = domain owner controls)
- **Display**: Show with verification badge

### UI Guidance

```
┌────────────────────────────────────────┐
│ Alice (alice.example.com) ✓            │  ← DNS verified
│ k5xq...1z2a                            │  ← IID (always show)
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│ Bob                                    │  ← Self-reported only
│ m3n4...7b8                             │  ← IID
│ ⚠ Name not verified                    │
└────────────────────────────────────────┘
```

## Petname System

Combine layers for maximum usability:

```
┌─────────────────────────────────────────────────────────────┐
│ Layer          │ Example           │ Trust Level            │
├─────────────────────────────────────────────────────────────┤
│ My alias       │ "alice"           │ High (I assigned it)   │
│ DNS name       │ "alice.example"   │ Medium (domain owner)  │
│ Self-reported  │ "Alice Smith"     │ Low (self-asserted)    │
│ IID            │ "k5xq7z..."       │ Cryptographic          │
└─────────────────────────────────────────────────────────────┘
```

### Display Priority

1. Show user's local alias if exists
2. Else show verified name (DNS/registry)
3. Else show self-reported name with warning
4. Always show truncated IID as fallback

## Name Change Handling

### DNS Name Changes

- DNS records can change
- Cache TTL controls refresh
- On change, warn user that verified name changed

### Self-Reported Name Changes

- Identity Document updates can change `claims.name`
- Show notification: "Bob changed their name to Robert"

### Local Alias Changes

- User explicitly updates their alias
- No notification needed (user-initiated)

## Interfaces

```typescript
interface NameResolver {
  // Resolve a name through all configured systems
  resolve(
    name: string,
    options?: ResolveOptions
  ): Promise<ResolveResult>;

  // Resolve with specific system
  resolveWith(
    name: string,
    system: 'local' | 'dns' | string
  ): Promise<ResolveResult | null>;

  // Reverse lookup (IID to names)
  reverseResolve(iid: string): Promise<ReverseResult>;

  // Add local alias
  addAlias(
    alias: string,
    iid: string,
    displayName?: string,
    verifiedVia?: string
  ): Promise<void>;

  // Remove local alias
  removeAlias(alias: string): Promise<void>;

  // List all aliases
  listAliases(): Promise<AliasEntry[]>;
}

interface ResolveOptions {
  skipLocal?: boolean;      // Don't check local aliases
  skipDns?: boolean;        // Don't check DNS
  maxRegistries?: number;   // Limit registry checks
  timeout?: number;         // Overall timeout (ms)
}

interface ResolveResult {
  iid: string;
  source: 'local' | 'dns' | string;
  name: string;
  verified: boolean;
  confidence: 'high' | 'medium' | 'low';
  cachedUntil?: Date;
}

interface ReverseResult {
  localAlias?: string;
  dnsNames: string[];
  registryNames: { registry: string; name: string }[];
  selfReportedName?: string;
}
```

## Security Considerations

1. **Name Squatting**: DNS and registries may have squatting. Mitigation: Verification badges, user education.
2. **DNS Hijacking**: Compromised DNS can redirect names. Mitigation: DNSSEC, always verify IID signature.
3. **Homoglyph Attacks**: "аlice" (cyrillic 'а') vs "alice". Mitigation: Normalize names, warn on suspicious characters.
4. **Cache Poisoning**: Stale cache could map to wrong IID. Mitigation: Respect TTLs, allow manual refresh.
5. **Registry Trust**: Decentralized registries may have governance issues. Mitigation: Users choose which to enable.

## Test Scenarios

1. **Local alias**: Add alias "alice", resolve returns correct IID
2. **DNS resolution**: Query _identity.alice.example.com, get IID
3. **Resolution order**: Local alias overrides DNS for same name
4. **DNS change**: Name maps to new IID, user warned
5. **Missing name**: Resolution returns null, falls back to IID display
6. **Homoglyph detection**: "аlice" flagged as suspicious
7. **Multiple registries**: Resolve through DNS then registry, first match wins
