# Identity Caching & Resolution Policy

## Overview

This document specifies how nodes discover, cache, and validate identity documents. Identity resolution is a critical dependency for all other layers.

## Resolution Sources

Identity documents can be obtained from multiple sources, in priority order:

| Priority | Source | Trust Level | Latency |
|----------|--------|-------------|---------|
| 1 | Local cache | Trusted (signature-verified) | Instant |
| 2 | Direct peer | High (authenticated connection) | Low |
| 3 | DHT | Low (must verify signature) | Medium |
| 4 | Directory service | Low (must verify signature) | Medium |

### Source Behavior

**Local Cache**: Always check first. Cache stores verified documents only.

**Direct Peer**: If connected to the identity owner, request their current document directly over authenticated channel.

**DHT**: Query distributed hash table keyed by IID. Multiple responses may exist; verify all and accept highest valid sequence.

**Directory Service**: Optional centralized directory for convenience. Treat as untrusted hint; always verify signatures.

## Caching Rules

### Cache Structure

```typescript
interface CachedIdentity {
  document: IdentityDocument;
  firstSeenAt: Timestamp;          // TOFU anchor
  lastVerifiedAt: Timestamp;       // When signature was last checked
  lastRefreshedAt: Timestamp;      // When we fetched from network
  source: 'peer' | 'dht' | 'directory' | 'local';
  pinned: boolean;                 // Manual pin prevents auto-refresh
  conflictState?: {
    alternateDocuments: IdentityDocument[];
    detectedAt: Timestamp;
  };
}
```

### TTL and Refresh Policy

| Scenario | TTL | Behavior |
|----------|-----|----------|
| Active peer (connected) | ∞ | Receive updates via authenticated channel |
| Recent contact (<24h) | 24 hours | Refresh on next interaction |
| Stale contact (>24h) | 7 days | Refresh before sending messages |
| Unknown identity | 0 | Must fetch before any operation |
| Pinned identity | ∞ | No auto-refresh, manual only |

### Refresh Triggers

1. **Pre-send**: Before sending a message to a peer, check if cache is stale
2. **Connection**: When establishing authenticated connection, exchange current documents
3. **Background**: Periodic refresh of recently-contacted identities
4. **Explicit**: User or app requests refresh

```typescript
interface CachePolicy {
  // Refresh thresholds
  STALE_THRESHOLD_HOURS: 24;       // Consider stale after this
  EXPIRE_THRESHOLD_DAYS: 7;        // Must refresh before operations after this
  BACKGROUND_REFRESH_HOURS: 6;     // Refresh active contacts this often

  // Negative caching
  NOT_FOUND_CACHE_MINUTES: 15;     // Cache "not found" results briefly
  FAILED_FETCH_RETRY_MINUTES: 5;   // Retry failed fetches after this

  // Limits
  MAX_CACHED_IDENTITIES: 10000;    // LRU eviction after this
  MAX_HISTORY_PER_IDENTITY: 10;    // Keep N previous versions for verification
}
```

## Trust-on-First-Use (TOFU)

### Genesis Binding

When first encountering an IID:

1. Fetch identity document from any source
2. Verify `iid == Base32Lower(SHA256(keys.signing.genesis))`
3. Verify `signatures.current` with `keys.signing.current`
4. Record `firstSeenAt` and the genesis public key
5. **TOFU anchor**: This genesis key is now associated with this IID forever

### Subsequent Updates

For updates to a TOFU-anchored identity:

1. Verify new document's `keys.signing.genesis` matches stored genesis key
2. Verify sequence has increased
3. Verify authorization (key continuity or recovery proof)
4. Update cache

### TOFU Violations

If an incoming document has:
- Same IID but different genesis key: **REJECT** (possible attack)
- Same genesis key but different IID: **REJECT** (invalid document)

```typescript
function checkTofu(cached: CachedIdentity, incoming: IdentityDocument): TofuResult {
  if (!cached) {
    // First time seeing this IID
    return { status: 'new', action: 'store_as_anchor' };
  }

  if (cached.document.keys.signing.genesis !== incoming.keys.signing.genesis) {
    // Genesis key mismatch - this is NEVER valid
    return {
      status: 'violation',
      action: 'reject',
      reason: 'Genesis key changed - possible attack'
    };
  }

  // Genesis matches, proceed with normal verification
  return { status: 'trusted', action: 'verify_update' };
}
```

## Offline and Partition Handling

### Offline Peers

When attempting to reach an offline peer:

1. Use cached identity document (even if stale)
2. Queue messages for later delivery (mailbox/relay)
3. Mark identity as "unverified-current" in UI
4. When peer comes online, refresh and re-verify

### Network Partitions

If identity fetch fails:

1. **Direct peer unavailable**: Fall back to DHT/directory
2. **DHT unavailable**: Use cache if available
3. **Complete network failure**: Use cache with warning, or block operation

```typescript
interface OfflinePolicy {
  // How long to trust cached identity when offline
  OFFLINE_GRACE_PERIOD_DAYS: 30;

  // After this, require re-verification before sensitive operations
  REVERIFY_THRESHOLD_DAYS: 7;

  // Operations allowed with stale cache
  ALLOWED_WITH_STALE: ['display', 'queue_message'];

  // Operations requiring fresh verification
  REQUIRE_FRESH: ['send_sensitive', 'key_agreement', 'grant_permission'];
}
```

## Identity Resolution API

```typescript
interface IdentityResolver {
  /**
   * Get identity, using cache or fetching as needed.
   * @param options.maxAge Maximum cache age to accept
   * @param options.forceRefresh Bypass cache entirely
   * @param options.timeout Fetch timeout in ms
   */
  resolve(
    iid: IdentityIdentifier,
    options?: ResolveOptions
  ): Promise<ResolveResult>;

  /**
   * Prefetch identities in background.
   */
  prefetch(iids: IdentityIdentifier[]): Promise<void>;

  /**
   * Pin an identity to prevent auto-refresh (for high-security contacts).
   */
  pin(iid: IdentityIdentifier): Promise<void>;

  /**
   * Unpin an identity.
   */
  unpin(iid: IdentityIdentifier): Promise<void>;

  /**
   * Get cache status for an identity.
   */
  getCacheStatus(iid: IdentityIdentifier): Promise<CacheStatus>;

  /**
   * Clear cache for an identity (forces re-fetch on next access).
   */
  evict(iid: IdentityIdentifier): Promise<void>;

  /**
   * Subscribe to identity updates.
   */
  subscribe(iid: IdentityIdentifier, callback: (doc: IdentityDocument) => void): Unsubscribe;
}

interface ResolveOptions {
  maxAge?: number;         // Max cache age in ms (0 = always fetch)
  forceRefresh?: boolean;  // Bypass cache
  timeout?: number;        // Fetch timeout in ms
  sources?: ('cache' | 'peer' | 'dht' | 'directory')[];  // Limit sources
}

interface ResolveResult {
  document: IdentityDocument;
  source: 'cache' | 'peer' | 'dht' | 'directory';
  age: number;             // Cache age in ms
  verified: boolean;       // Signature verified
  warnings?: string[];     // E.g., "using stale cache"
}

interface CacheStatus {
  cached: boolean;
  document?: IdentityDocument;
  firstSeenAt?: Timestamp;
  lastRefreshedAt?: Timestamp;
  age?: number;
  stale: boolean;
  pinned: boolean;
  hasConflict: boolean;
}
```

## Transport Integration Contract

The Identity layer requires Transport to provide:

1. **Authenticated channels**: Bidirectional messaging where peers prove identity ownership
2. **Update propagation**: Broadcast mechanism for identity updates
3. **DHT operations**: Put/get keyed by IID

**Minimal Transport Interface for Identity**:

```typescript
interface IdentityTransport {
  // Request current identity document from a connected peer
  requestIdentityFromPeer(peerId: string): Promise<IdentityDocument | null>;

  // Broadcast identity update to known peers
  broadcastIdentityUpdate(doc: IdentityDocument): Promise<PropagationResult>;

  // DHT operations
  dhtPut(key: string, value: Uint8Array): Promise<void>;
  dhtGet(key: string): Promise<Uint8Array[]>;  // May return multiple values

  // Directory operations (optional)
  directoryLookup(iid: IdentityIdentifier): Promise<IdentityDocument | null>;
  directoryRegister(doc: IdentityDocument): Promise<void>;
}
```

## Security Considerations

1. **Cache Poisoning**: Only cache documents with valid signatures
2. **TOFU Attacks**: First-contact is vulnerable; encourage out-of-band verification for important contacts
3. **Stale Cache Attacks**: Time-limited cache validity prevents long-term use of compromised keys
4. **Eclipse Attacks**: Multiple sources reduce single-point-of-failure risk
5. **Privacy**: Cache contents reveal social graph; encrypt at rest

## Test Scenarios

1. **Fresh fetch**: IID not in cache, fetch from DHT, verify, cache
2. **Cache hit**: IID in cache, not stale, return immediately
3. **Stale refresh**: IID in cache but stale, fetch and update
4. **TOFU violation**: Incoming doc has different genesis key, reject
5. **Offline operation**: Network unavailable, use cached doc with warning
6. **Conflict detection**: Receive same-sequence doc with different content, flag for manual resolution
