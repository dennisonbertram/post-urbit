## BLOCKING (must fix before implementation interop)

### 1) Base32 alphabet is inconsistent (RFC4648 vs Crockford) across core identity + all derived IDs
**Locations**
- `spec/02-identity-trust/identity-document-schema.md` → **Identity Identifier (IID)** / **Encoding Specification** (RFC4648 `a-z2-7`)
- `spec/00-shared/test-vectors.md` → **Notation** (RFC4648 base32), **Test Vector 1** (RFC4648 encoding implementation)
- `spec/06-rfcs/RFC-0001-identity-document.md` → §3, §3.2 (RFC4648)
- `spec/06-rfcs/RFC-0002-transport.md` → §2.1 (**Crockford Base32**)
- `spec/03-messaging-sync/group-messaging.md` → **group_id** derivation uses `Base32Lower(...)` (unspecified alphabet)
- `spec/02-identity-trust/name-resolution.md` (and examples) implicitly assume RFC4648-style validity constraints
- Many example IIDs contain `u` (Crockford excludes `u`), e.g. `identity-document-schema.md` examples.

**Problem**
- The repo simultaneously specifies **RFC4648 Base32** and **Crockford Base32** for IIDs/DIDs/GroupIds. This is not just “formatting”: it changes the canonical string form, validation rules, decoding, DHT keys (which hash the IID string), and handshake `decode_base32()` behavior.
- Your requirement says: **Crockford variant everywhere**.

**Recommended fix**
- Make a single normative definition in one place (suggest: `spec/00-shared/layer-integration.md` “Global Conventions” or a new `spec/00-shared/encoding.md`):
  - **Crockford Base32**, lowercase only.
  - Specify normalization rules (reject non-alphabet chars; optionally accept upper then normalize? but RFC-0002 currently says “accept lowercase only”).
- Update these documents to match Crockford:
  - `identity-document-schema.md`, `RFC-0001`, `test-vectors.md`, `name-resolution.md`, all ID examples, all “invalid character” examples.
- Update all algorithms using `base64.b32encode` (RFC4648) to a Crockford encoder/decoder.
- Consider switching DHT key derivation inputs from `prefix || iid_string` to `prefix || iid_raw_20B` to decouple DHT keys from string alphabet (optional, but reduces future churn).

---

### 2) QUIC stream framing & payload typing contradicts Messaging + Sync specs (JSON vs binary vs CBOR; double-framing)
**Locations**
- `spec/00-shared/layer-integration.md` → **QUIC Stream Framing (Normative, All Stream Types):** mandates JSON payload for all stream types.
- `spec/06-rfcs/RFC-0002-transport.md` → §6.3 says streams 0x01–0x04 are **UTF-8 JSON**, and payload has JSON `type`.
- `spec/03-messaging-sync/secure-envelope.md` → Message stream 0x03 is intended to carry **raw PUSE bytes**.
- `spec/03-messaging-sync/sync-protocol.md` → defines its **own per-message framing**: `MessageType (1B) + Length (4B) + CBOR`, which conflicts with the “transport-level 4B length per frame” approach.

**Problem**
- At least three incompatible interpretations exist:
  1) “All streams are length-prefixed JSON” (layer-integration)
  2) “Control/Identity/Message/Sync are JSON” (RFC-0002)
  3) “Message is binary PUSE; Sync is CBOR with its own sub-framing” (Messaging+Sync docs)
- Implementations built to RFC-0002 will not interoperate with implementations built to `secure-envelope.md`/`sync-protocol.md`.

**Recommended fix**
- Pick **one** cross-layer contract, then make all layers conform. The cleanest is:
  - **Transport framing:** `stream_type (1B once) + repeated { len_u32_be + payload_bytes }`
  - **Payload by stream_type:**
    - `0x01 Control`: UTF-8 JSON objects (has `type`)
    - `0x02 Identity`: UTF-8 JSON objects (has `type`) *or* raw IDOC envelopes; choose one and stick to it
    - `0x03 Message`: **binary** payload = exactly one PUSE envelope per frame
    - `0x04 Sync`: **binary** payload = CBOR message per frame (remove the extra inner `Length` field from `sync-protocol.md`)
    - `0x05 Bulk`: binary; define separately
- Update:
  - `layer-integration.md` framing section (remove “JSON for all streams” claim).
  - `RFC-0002` §6.3 payload typing table.
  - `sync-protocol.md` to avoid redundant length fields.

---

### 3) Device Document schema is not canonical across Identity ↔ Transport ↔ DHT (fields + endpoints + transport key)
**Locations**
- `spec/00-shared/layer-integration.md` → **Device Document Structure (canonical)**:
  - includes `endpoints`
  - uses `device_signing_key`
  - explicitly says **`device_transport_key removed in v1`**
  - uses snake_case names.
- `spec/02-identity-trust/identity-document-schema.md` → **Device Document**:
  - includes `device_transport_key`
  - does **not** include `endpoints`
- `spec/01-transport-connectivity/interfaces.md` → `DeviceDocument` TypeScript:
  - includes `deviceTransportKey`
  - has `deviceName?` and no `endpoints`
- `spec/06-rfcs/RFC-0001-identity-document.md` → §13.2 includes `device_transport_key` and no `endpoints`.

**Problem**
- Implementations can’t agree on what a Device Document is, or what is signed/verified.
- Device discovery in `layer-integration.md` requires device endpoints to connect “to a device”, but Identity’s device doc schema doesn’t carry them.

**Recommended fix**
- Establish **one canonical on-wire Device Document** (snake_case) in `identity-document-schema.md` and `RFC-0001`:
  - Include `endpoints` (needed for per-device transport connections).
  - Decide on `device_transport_key`:
    - Either **remove** it everywhere (and remove KeyStorage “device transport key” claims in `node-daemon.md`), or
    - Keep it but mark **optional** + “unused in v1 handshake”, and keep it consistent everywhere.
- Update Transport TS types as a mapping layer (camelCase view), not as a competing schema.

---

### 4) DHT authentication model contradicts itself (internal IDOC sig only vs extra DHT-record signature)
**Locations**
- `spec/00-shared/layer-integration.md` → **DHT Record Format** states: “**No separate DHT signature required**”
- Same file → **Transport API Bridge** `publishIdentity()` signs `idocBytes` and passes `{ signature: recordSig }` into DHT.
- `spec/02-identity-trust/caching-policy.md` → `IdentityTransport.dhtPut(... signature ...)` and notes DHT nodes verify signature before storing to prevent spam.
- `spec/01-transport-connectivity/overview.md` → says DHT record has “Signature: Ed25519 by document’s signing key”
- `spec/01-transport-connectivity/interfaces.md` → `DiscoveryService.registerIdentity()` / `lookupPeer()` don’t match the “fetch full IDOC” claim.

**Problem**
- There are two competing models:
  - Model A: IDOC is self-authenticating; DHT stores after verifying internal signatures.
  - Model B: DHT requires an additional record signature.
- The APIs and examples mix both.

**Recommended fix**
- Choose Model A or B and make it consistent:
  - If **Model A** (recommended): remove `signature` from DHT put interfaces and examples; specify DHT admission rules are based on parsing/verifying the stored object (IDOC/device doc/index).
  - If **Model B**: define **exact signature input** (domain sep, what bytes, who signs), and require it uniformly for identity/device/index records.
- Also align `DiscoveryService` with the resolved behavior:
  - Either add `fetchIdentity(iid) -> IdentityDocument` to transport discovery, or state Identity layer talks to a lower-level `DhtClient` directly.

---

### 5) Domain separator strings are inconsistent for the same operation, and RFC-0002 hardcodes wrong byte lengths
**Locations**
- `spec/01-transport-connectivity/peer-handshake.md` uses:
  - `post-urbit-handshake-v1` (ok)
  - `post-urbit-device-handshake-v1`
- `spec/06-rfcs/RFC-0002-transport.md` uses:
  - device domain `post-urbit-device-v1` and claims “exactly 19 bytes” (string is 20 bytes)
  - relay alloc domain `post-urbit-relay-alloc-v1` and claims “exactly 24 bytes” (string is 25 bytes)
  - rebind domain `post-urbit-rebind-v1` and claims “exactly 19 bytes” (string is 20 bytes)
- `spec/01-transport-connectivity/relay-protocol.md` uses `post-urbit-relay-allocate-v1` (different string than RFC)

**Problem**
- Two implementations that follow different docs will sign different transcripts and fail verification.
- The RFC’s explicit byte-length claims are incorrect for multiple separators, causing additional implementation traps.

**Recommended fix**
- Create an explicit **Domain Separator Registry** (suggest: `spec/00-shared/domain-separators.md`) listing:
  - canonical string
  - exact bytes (ASCII/UTF-8)
  - whether SHA256-prehash is applied before Ed25519
- Update all references to match the registry.
- In RFC-0002: either fix the byte counts or remove the “exactly N bytes” assertions and instead specify “ASCII bytes of the literal string”.

---

### 6) RFC-0002 relay test vector uses the wrong magic bytes (“PURT” not “PURL”)
**Location**
- `spec/06-rfcs/RFC-0002-transport.md` → §11.3 PURL Packet test vector:
  - `50555254` is ASCII `"PURT"`; `"PURL"` is `5055524c`.

**Problem**
- Anyone implementing test vectors will bake in the wrong constant and fail interop.

**Recommended fix**
- Correct the test vector hex and add an explicit check:
  - Magic MUST equal `0x50 0x55 0x52 0x4C` (“PURL”).
- Cross-check `spec/01-transport-connectivity/relay-protocol.md` to ensure it remains consistent.

---

### 7) App manifest signing is internally contradictory (SIGNATURE-file-only vs embedded manifest.signature)
**Locations**
- `spec/04-app-runtime/manifest-schema.md`:
  - multiple examples include `"signature": {...}` inside `manifest.json`
  - includes `verifyManifestSignature()` that assumes embedded signature
  - elsewhere in same file: “manifest.json does NOT contain a signature field. SIGNATURE file only”
- `spec/05-ux-packaging/app-distribution.md` and `spec/04-app-runtime/manifest-schema.md` (earlier sections) require SIGNATURE file.

**Problem**
- This is a security-critical packaging ambiguity: different tooling will sign/verify different things.

**Recommended fix**
- Remove all embedded `manifest.signature` examples and the embedded signature verification pseudocode.
- Make the manifest schema explicitly forbid `signature` at top-level.
- Keep **only** the SIGNATURE file method, and ensure all examples reflect that.

---

### 8) Error code registries are not aligned (names/codes present in one place but missing elsewhere)
**Locations**
- `spec/00-shared/layer-integration.md` → Error code ranges
- `spec/01-transport-connectivity/quic-integration.md` → Application error codes only up to `0x104`
- `spec/01-transport-connectivity/interfaces.md` → glare close code `0x105`
- `spec/06-rfcs/RFC-0002-transport.md` → registry includes `0x105..0x107`
- `spec/01-transport-connectivity/peer-handshake.md` → error `TLS_BINDING_MISMATCH`, `NONCE_REUSE`, etc (string codes) without numeric mapping.

**Problem**
- Implementers won’t know which QUIC application error code to emit on which failure, and registries will drift.

**Recommended fix**
- Make **RFC-0002 §9.2** the authoritative numeric registry for transport and have all other docs reference it.
- Update `quic-integration.md` to include `0x105..0x107`.
- Add a table mapping handshake failure reasons → QUIC application close codes.

---

## HIGH (major interop/security issues; fix before shipping MVP)

### 9) Mailbox bearer token violates your global encoding conventions and lacks explicit domain separation
**Locations**
- `spec/00-shared/layer-integration.md` → **Mailbox Auth Token Format**
  - says “Base64-encoded JSON” (not Base64url) for a bearer token
  - signature input is “JCS canonical JSON (without signature field)” but no domain separator
- Same file → **Global Conventions** says “Tokens (relay, auth) → Base64url”.

**Problem**
- Token encoding conflicts with conventions; signature is vulnerable to cross-context replay if the same canonical JSON is used elsewhere.

**Recommended fix**
- Make mailbox bearer token:
  - **Base64url (no padding)** encoded JCS JSON
  - signature = Ed25519 over `domain_sep || JCS(token_without_signature)`
  - Add domain separator entry (e.g., `post-urbit:mailbox-token:v1:`) to the registry.

---

### 10) PUSE envelope signature has no explicit domain separator despite “all signatures use domain separation”
**Locations**
- `spec/03-messaging-sync/secure-envelope.md` → **Signature Scheme**
- `spec/02-identity-trust/identity-document-schema.md` and others assert domain separation as a system-wide rule.
- `spec/00-shared/layer-integration.md` says “All signatures … use a domain separator prefix.”

**Problem**
- Either the “all signatures” statement is false, or PUSE signature is missing an explicit prefix. This is both a spec contradiction and a potential cross-protocol replay surface (even if practically mitigated by `"PUSE"` magic).

**Recommended fix**
- Either:
  1) Add a domain separator for PUSE signature (e.g., sign `post-urbit:puse:v1:` + `envelope_without_sig`), **or**
  2) Explicitly carve out a rule: “Binary formats with fixed magic+version are self-domain-separated” and remove/soften the universal claim.

---

### 11) Sync operation signatures lack explicit domain separation and may collide with other identity-key signatures
**Location**
- `spec/03-messaging-sync/sync-protocol.md` → **Operation Signature**

**Problem**
- Uses identity signing keys; signature input is ad-hoc concatenation without a protocol label, contradicting your domain-sep doctrine.

**Recommended fix**
- Define signature input as:
  - `post-urbit:sync-op:v1:` + canonical encoding of the operation fields (CBOR canonical or fixed byte layout), then sign.
- Add to domain separator registry.

---

### 12) Anonymous handshake is allowed in `peer-handshake.md` but forbidden/out-of-scope in RFC-0002
**Locations**
- `spec/01-transport-connectivity/peer-handshake.md` → **Anonymous Connections** section (defines `client_iid: null`)
- `spec/06-rfcs/RFC-0002-transport.md` → §5.14 says anonymous connections are **NOT defined** and “all peer-to-peer MUST be mutually authenticated.”

**Problem**
- This is a direct behavior contradiction.

**Recommended fix**
- Decide v1 stance (RFC currently says “no anonymous”):
  - Remove anonymous mode from `peer-handshake.md`, or
  - Add it to RFC-0002 with a fully defined transcript and verification rules.

---

### 13) Relay allocation request encoding differs between relay-protocol.md and RFC-0002 (nonce base64 vs base64url; seq number type)
**Locations**
- `spec/01-transport-connectivity/relay-protocol.md` → Allocation Request uses `"nonce": "<16-bytes-base64-random>"`, `identity_doc_sequence: 42` (number)
- `spec/06-rfcs/RFC-0002-transport.md` → Allocation Request uses base64url nonce and `identity_doc_sequence: "42"` (string)

**Problem**
- These mismatches break verification and/or JSON parsing in strict implementations.

**Recommended fix**
- Make relay-protocol.md match RFC-0002 (or vice versa, but pick one):
  - `nonce`: base64url (no padding)
  - `identity_doc_sequence`: decimal string
  - align domain separator string (see BLOCKING #5).

---

## MEDIUM (important cleanups; may block some features but not base interop)

### 14) NAT traversal candidate example uses non-existent relay allocation fields
**Location**
- `spec/01-transport-connectivity/nat-traversal.md` → `collectCandidates()` pseudocode uses `allocation.address` / `allocation.port`

**Problem**
- `RelayAllocation` in `spec/01-transport-connectivity/interfaces.md` defines `boundAddress/boundPort` and relay server address/port separately. Using `allocation.address` is undefined and conceptually wrong for “relay candidate”.

**Recommended fix**
- Change relay candidate to use `relay.address/relay.port` plus `token` metadata, not the client’s `boundAddress/boundPort`.

---

### 15) DHT “genesis record” exists in RFC-0001 but is not integrated into caching/resolution flow
**Locations**
- `spec/06-rfcs/RFC-0001-identity-document.md` → §12.3 `post-urbit:genesis:` record
- `spec/02-identity-trust/caching-policy.md` and `spec/00-shared/layer-integration.md` do not define fetching/storing it.

**Problem**
- Chain verification guidance references genesis availability but the operational system doesn’t actually define who publishes/refreshes it or how resolvers use it.

**Recommended fix**
- Add to `caching-policy.md` resolution sources:
  - attempt genesis fetch from `post-urbit:genesis:` when first anchoring.
- Add publication rules to node lifecycle (`node-daemon.md` identity publish task).

---

### 16) Revocation (identity/device) has no defined transport-layer acquisition/checkpointing path
**Locations**
- `spec/02-identity-trust/revocation.md` defines revocation docs and “DHT/directory update”
- `spec/06-rfcs/RFC-0002-transport.md` defines close codes `REVOKED_IDENTITY` / `REVOKED_KEY`
- No concrete mechanism in `layer-integration.md` or `transport` specs for how a transport handshake checks revocation state.

**Problem**
- Transport cannot actually enforce the revocation close codes without a defined query path and caching semantics.

**Recommended fix**
- Define: revocation records DHT keys + TTL + signature scheme, and add to IdentityResolver API (`isRevoked(iid)`).
- Transport handshake verification procedure should call into Identity layer for revocation status before accepting.

---

### 17) Transport resumption fields don’t line up across APIs/specs
**Locations**
- `spec/01-transport-connectivity/interfaces.md` → `ConnectOptions.expectedSequence`, `sessionTicket`
- `spec/01-transport-connectivity/peer-handshake.md` → `resume.session_id`, `last_seen_sequence`
- `spec/06-rfcs/RFC-0002-transport.md` → abbreviated handshake defines `resume.session_id` too.

**Problem**
- Implementation-facing APIs and wire protocol don’t align on identifiers and semantics.

**Recommended fix**
- Define one resumption object:
  - `resume: { session_ticket?, last_seen_sequence }` (or similar)
  - ensure naming matches in TS interfaces and RFC.

---

## LOW (polish / drift / documentation correctness)

### 18) Admin UI client example contradicts cookie-based auth model
**Locations**
- `spec/05-ux-packaging/admin-ui.md` → `ApiClient` uses `Authorization: Bearer ${token}`
- `spec/05-ux-packaging/interfaces.md` and `node-daemon.md` → browser uses HttpOnly cookies + CSRF token, not bearer.

**Recommended fix**
- Update `admin-ui.md` client example:
  - remove bearer usage for browser flows
  - demonstrate CSRF header usage and cookie-based session.

---

### 19) Terminology drift: “post-urbit” vs “postnode” is inconsistent in protocol identifiers and docs
**Locations**
- Mixed across packaging (`postnode-*`), identity (`post-urbit:*`), transport (`post-urbit/1` ALPN), repository signatures (`postnode-repo-v1`).

**Recommended fix**
- Decide branding prefix strategy:
  - Keep wire/protocol identifiers stable (`post-urbit:*`, `post-urbit/1`) and allow product name “postnode” in UX only, **or**
  - Rename consistently (large churn). Document the rule explicitly.

---

### 20) Test vectors include “regenerate” placeholders and outdated notes that conflict with “no TBDs”
**Location**
- `spec/00-shared/test-vectors.md` → Identity doc signature section still contains “regenerate” notes and obsolete signature commentary.

**Recommended fix**
- Either provide final values for all vectors, or mark the entire file as “Draft vectors” and remove it from “Specification Completeness Criteria”.

---

## Net: top 5 fixes to do first (in order)
1) **Unify Base32 to Crockford** everywhere + update all examples/vectors.  
2) **Fix stream framing/payload contracts** (Message=binary PUSE, Sync=binary CBOR, avoid double length).  
3) **Canonicalize Device Document schema** (endpoints + key fields) across Identity/RFC/Transport.  
4) **Resolve DHT auth model** (internal-only vs extra signature) and align APIs.  
5) **Create domain separator registry** + fix RFC-0002 domain strings/lengths + relay magic test vector.

If you want, I can propose concrete patch-style edits (exact replacement text) for the framing section and the Base32/Domain-sep registries, since those two changes ripple through most files.
