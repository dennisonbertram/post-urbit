## Holistic Cross-Layer Consistency Findings

### 1) IID examples use an invalid Base32 alphabet (contains `0/1/8/9`)
- **Severity:** **BLOCKING**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`, `spec/02-identity-trust/name-resolution.md`, any place reusing the example IID
- **Inconsistency:** IID is specified as RFC4648 Base32 (`A-Z2-7`, lowercase), but examples like `...x0y1z2a` include digits not in that alphabet.
- **Recommended fix:** Replace all IID examples with valid base32-lowercase strings (`a-z2-7` only). Add a validator note: reject any IID containing characters outside `[a-z2-7]`.

---

### 2) IdentityDocument examples/encoding contradict “raw 32-byte Ed25519 key bytes”
- **Severity:** **BLOCKING**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`
- **Inconsistency:** The schema mandates **raw 32-byte** Ed25519/X25519 public keys Base64-encoded (no DER/SPKI), but examples use `MCowBQYDK2VwAyEA...` which is characteristic of SPKI/DER encodings.
- **Recommended fix:** Replace examples with Base64 of *raw* 32-byte keys (and specify expected encoded length without padding). Add explicit test vectors using raw-byte keys to prevent implementers from using SPKI.

---

### 3) Genesis document example omits `keys.signing.genesis` even though it’s required
- **Severity:** **BLOCKING**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`
- **Inconsistency:** The schema requires `keys.signing.genesis`, and IID derivation depends on it; the “Genesis Document” example does not include it.
- **Recommended fix:** Update example documents to include:
  - `keys.signing.genesis`
  - `keys.signing.current == keys.signing.genesis` for `sequence=0`

---

### 4) IdentityDocument JSON field naming is inconsistent (snake_case vs camelCase)
- **Severity:** **BLOCKING**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`, `spec/02-identity-trust/interfaces.md`, `spec/02-identity-trust/recovery-mechanisms.md`
- **Inconsistency:** The schema uses snake_case on-wire (`recovery_proof`, `initiated_at`, `cooldown_expires_at`, `proof_data`), while the TypeScript `IdentityDocument` interface uses camelCase (`recoveryProof`, `initiatedAt`, `cooldownExpiresAt`, `proofData`).
- **Recommended fix:** Decide one on-wire convention (the rest of the protocol JSON already leans snake_case, e.g. `client_iid`). Then:
  - Make `interfaces.md` match the **on-wire** field names exactly, **or**
  - Explicitly define: “TypeScript interfaces are internal model; wire JSON uses snake_case,” and provide a normative mapping + canonicalization input (JCS must see the wire form).

---

### 5) Identity document timestamp validity (±24h) conflicts with caching/offline requirements
- **Severity:** **BLOCKING**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`, `spec/02-identity-trust/caching-policy.md`
- **Inconsistency:** Identity document verification says reject docs whose `timestamp` is not within ±24h of “now”. That makes cached identities unusable after a day, contradicting caching TTLs (7–30 days) and offline operation.
- **Recommended fix:** Change identity-doc timestamp rules to something like:
  - MUST NOT be more than `FUTURE_SKEW` (e.g., +24h) ahead of verifier clock
  - MUST be ≥ previous document timestamp (monotonic-ish)
  - MAY be arbitrarily old (sequence number prevents replay)
  - UI MAY warn on “very old timestamp”, but MUST NOT reject purely due to age

---

### 6) `sequence` is specified as uint64 but modeled as JS `number`
- **Severity:** **HIGH**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`, `spec/02-identity-trust/interfaces.md`
- **Inconsistency:** JSON `number` and TS `number` cannot safely represent uint64 beyond 2^53-1.
- **Recommended fix:** Make `sequence` on-wire a **string** containing an unsigned decimal integer (common practice), or constrain it to 53-bit explicitly. Update all interfaces and verification language accordingly.

---

### 7) Encryption key history structure conflicts across identity schema vs interfaces vs rotation doc
- **Severity:** **HIGH**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`, `spec/02-identity-trust/interfaces.md`, `spec/02-identity-trust/key-rotation.md`
- **Inconsistency:**
  - Schema: `keys.encryption.previous: <single key>|null`
  - Interfaces: `keys.encryption.previous: EncryptionKeyHistory[]` with validity windows
  - Rotation doc: implies a single “old encryption key” retained
- **Recommended fix:** Pick one:
  - **Preferred:** adopt the interfaces’ history array (supports long-offline peers), and update schema + rotation rules + examples accordingly.
  - Define pruning/retention rules (e.g., keep last N or last 30 days).

---

### 8) Endpoint schema is not aligned between Identity and Transport
- **Severity:** **HIGH**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`, `spec/01-transport-connectivity/interfaces.md`
- **Inconsistency:** Identity endpoints use `address: "<host:port>"` or URL strings; Transport `Endpoint` splits `address` and `port`, adds `transport`, `relayId`, `observedAt`, etc.
- **Recommended fix:** Define a single **normative on-wire Endpoint object** in the Identity Document that Transport can consume without guesswork. Options:
  1) Keep `address` as string but allow `port?: number` and specify parsing rules and URL forms, or  
  2) Normalize to `{ host, port, scheme/transport, relay_id }` fields.
  
  Then ensure `TransportService.connect()`’s endpoint resolution references that exact schema.

---

### 9) Handshake `tls_binding` description contradicts itself (session-id hash vs TLS exporter)
- **Severity:** **HIGH**
- **File(s):** `spec/01-transport-connectivity/peer-handshake.md`
- **Inconsistency:** Message examples label `tls_binding` as “SHA256-of-TLS-session-id”, but later the spec correctly mandates TLS Exporter (RFC 8446).
- **Recommended fix:** Replace all references/examples to session-id hashing with the exporter-derived value; keep one normative definition only.

---

### 10) Transport handshake error codes are not consistent across the layer
- **Severity:** **MEDIUM**
- **File(s):** `spec/01-transport-connectivity/interfaces.md`, `spec/01-transport-connectivity/peer-handshake.md`
- **Inconsistency:** `peer-handshake.md` defines errors like `TLS_BINDING_MISMATCH`, `DOCUMENT_INVALID`, `VERSION_UNSUPPORTED`, `NONCE_REUSE`, but `TransportErrorCode` union omits several of these.
- **Recommended fix:** Add missing codes to `TransportErrorCode` (or define a nested `HandshakeErrorCode`) and ensure `HandshakeComplete.error.code` is constrained to that set.

---

### 11) QUIC application error code registry is incomplete vs other docs
- **Severity:** **LOW**
- **File(s):** `spec/01-transport-connectivity/quic-integration.md`, `spec/01-transport-connectivity/interfaces.md`
- **Inconsistency:** `interfaces.md` defines duplicate connection close code `0x105`, but `quic-integration.md` app error table ends at `0x104`.
- **Recommended fix:** Add `0x105 DUPLICATE_CONNECTION` to the QUIC application error code table.

---

### 12) Relay header truncates IID (“first 16 bytes”) despite IID being 20 bytes of hash
- **Severity:** **HIGH**
- **File(s):** `spec/01-transport-connectivity/relay-protocol.md`, `spec/02-identity-trust/identity-document-schema.md`
- **Inconsistency:** IID is defined as 160-bit (20 bytes) truncated SHA-256; relay packet reserves only 16 bytes, which increases collision risk and contradicts IID definition.
- **Recommended fix:** Make relay packet carry the full 20-byte IID (decoded from Base32), or carry the full 32-char Base32 IID with length-prefixing. If you *must* truncate for size, you need a collision-handling rule (strongly discouraged).

---

### 13) Relay “Allocation Token 16 bytes” conflicts with API type `token: string` (encoding unspecified)
- **Severity:** **MEDIUM**
- **File(s):** `spec/01-transport-connectivity/relay-protocol.md`, `spec/01-transport-connectivity/interfaces.md`
- **Inconsistency:** Wire says fixed 16 bytes; API uses string without specifying encoding/length.
- **Recommended fix:** Specify token encoding explicitly (e.g., Base64url/no-pad of 16 raw bytes = 22 chars). Update API comments and relay header accordingly.

---

### 14) Relay allocation authentication is underspecified and internally inconsistent
- **Severity:** **HIGH**
- **File(s):** `spec/01-transport-connectivity/relay-protocol.md`
- **Inconsistency:** Allocation request uses `Authorization: Bearer <jwt-signed-by-identity>` plus also includes `signature` in JSON; later defines `AllocationAuth` as a different signature scheme. JWT profile (claims, key binding, replay protection) is not specified.
- **Recommended fix:** Choose **one** normative allocation auth mechanism. For example:
  - Signed request body using Ed25519 with `{iid, timestamp, nonce, lifetime}` and include (or reference) the identity document sequence used for verification.
  - Define replay rules (nonce cache, timestamp skew).
  - If keeping JWT/JWS, specify JWS algorithm (`EdDSA`), required claims (`aud`, `exp`, `iat`, `nonce`), and verification keys source.

---

### 15) Relay capability/token theft and injection risk not addressed (binding to source tuple / migration)
- **Severity:** **HIGH**
- **File(s):** `spec/01-transport-connectivity/relay-protocol.md`
- **Inconsistency:** The relay packet includes a bearer-like token; if observed by an on-path adversary, it may enable packet injection to the relay. The spec doesn’t state whether relay binds allocations to the originating IP:port or to a QUIC connection identity.
- **Recommended fix:** Add a normative rule, e.g.:
  - Allocation token is accepted only from the **same** source address (or validated via a QUIC-authenticated control channel).
  - Define how NAT rebinding / mobility updates that binding (explicit “rebind” message or QUIC connection migration semantics).

---

### 16) Identity “wire format” (IDOC envelope) conflicts with handshake embedding identity_document as JSON object
- **Severity:** **MEDIUM**
- **File(s):** `spec/02-identity-trust/identity-document-schema.md`, `spec/01-transport-connectivity/peer-handshake.md`
- **Inconsistency:** Identity schema declares an `IDOC` binary envelope “for network transmission”, while handshake embeds identity document inline as JSON.
- **Recommended fix:** Clarify scopes:
  - “When sent as a standalone binary payload (DHT value, identity stream), use IDOC.”
  - “When included inside another JSON structure (handshake), include the canonical JSON object (still verified via JCS for signature).”

---

### 17) Identity↔Transport “DHT contract” does not line up (Identity expects put/get; Transport provides endpoint lookup)
- **Severity:** **HIGH**
- **File(s):** `spec/02-identity-trust/caching-policy.md`, `spec/01-transport-connectivity/interfaces.md`
- **Inconsistency:** Identity layer’s `IdentityTransport` requires `dhtPut/dhtGet` (arbitrary key/value blobs) and directory operations. Transport layer defines `DiscoveryService.registerIdentity(document)` and `lookupPeer()` returning endpoints, not identity documents nor generic DHT primitives.
- **Recommended fix:** Either:
  1) Expand Transport to provide generic `dhtPut/dhtGet` and directory interfaces (as Identity expects), **or**
  2) Narrow Identity’s dependency to match Transport’s discovery APIs, and introduce a separate **Identity Publication format** (what exactly is stored in DHT: full IDOC? pointer? endpoints only?).

---

### 18) Key-rotation conflict resolution contradicts identity-doc conflict guidance
- **Severity:** **HIGH**
- **File(s):** `spec/02-identity-trust/key-rotation.md`, `spec/02-identity-trust/identity-document-schema.md`
- **Inconsistency:** `key-rotation.md` suggests resolving same-sequence conflicts via “lower timestamp (if within 1 minute)”, while identity schema says same-sequence conflicts should not be auto-resolved (TOFU/first-seen or manual), warning against gameable tiebreakers.
- **Recommended fix:** Remove timestamp tiebreaker from key-rotation.md and align with identity schema’s conflict policy (TOFU-first-seen + manual / recovery-based resolution).

---

### 19) Endianness is not consistently specified in binary wire formats
- **Severity:** **MEDIUM**
- **File(s):** `spec/01-transport-connectivity/nat-traversal.md`, `spec/01-transport-connectivity/relay-protocol.md`, `spec/02-identity-trust/identity-document-schema.md`
- **Inconsistency:** Some fields say big-endian; others omit (e.g., relay `Payload Length` 2 bytes, discovery `Observed Port`).
- **Recommended fix:** Add a global rule: “All multi-byte integers in binary wire formats are network byte order (big-endian)” and annotate any exceptions.

---

### 20) Missing glue specs needed to actually connect layers end-to-end
- **Severity:** **HIGH**
- **File(s):** multiple; highlighted by `spec/progress.md` claiming “complete”
- **Missing pieces:**
  1) **Mailbox/store-and-forward protocol** (mentioned as endpoint type, not specified)
  2) **DHT protocol + record formats** (identity doc publication, endpoint publication, signatures, TTLs)
  3) **Identity update propagation on Transport streams** (what stream type? framing? ack/retry?)
  4) **TLS certificate handling policy** for QUIC (self-signed acceptance rules, any pinning, DoS considerations)
- **Recommended fix:** Add one “Glue” RFC/spec that defines:
  - Which artifacts live in DHT (full IDOC vs endpoints vs pointers)
  - How identity updates are pushed over authenticated QUIC streams
  - Mailbox minimal viable store-and-forward envelope and retrieval semantics
  - QUIC TLS certificate validation mode (likely “accept-any-cert, rely on post-TLS identity handshake”, plus hardening guidance)

---

## Quick sanity check on the 8 requested categories

1) **Type consistency:** Not consistent (IID examples invalid; seq uint64 vs TS number; endpoints differ; naming casing mismatch).  
2) **Interface alignment:** Not aligned (Identity expects generic DHT; Transport offers endpoint discovery; Endpoint structs differ).  
3) **Error code ranges:** Numeric QUIC codes mostly fine; registries incomplete; handshake codes not unified.  
4) **Wire format coherence:** Mostly coherent (big-endian often), but relay IID truncation and unspecified endianness are problems; IDOC vs JSON embedding needs clarification.  
5) **Timestamp handling:** RFC3339 used, but identity timestamp validity rule conflicts with caching/offline.  
6) **Dependency correctness:** No direct circular dependency, but the DHT/discovery boundary is unclear (Identity depends on capabilities Transport doesn’t specify).  
7) **Security model consistency:** Generally coherent (untrusted relays, identity-bound handshake), but relay auth/token binding and key-rotation conflict policy are weak points.  
8) **Missing glue:** Mailbox, DHT record formats, identity-update transport framing, QUIC TLS cert policy.

If you want, I can propose a single consolidated “normative data model” for: `IdentityIdentifier`, `IdentityDocument` (exact on-wire JSON), and `Endpoint` that both layers import verbatim, plus a minimal DHT publication format that satisfies both `DiscoveryService.lookupPeer()` and `IdentityResolver.resolve()`.
