## BLOCKING (must fix before implementation/RFC-final)

1) **IDOC signature payload is inconsistent (domain separation vs none)**
- **Where:** `identity-document-schema.md` (Canonical Serialization), `key-rotation.md`, `revocation.md`, `00-shared/test-vectors.md`, **RFC-0001 §6/§7/§15.7**
- **Conflict:** Most layer docs + test vectors imply `Ed25519_Sign(JCS(doc_without_signatures))`. RFC-0001 changes this to `Ed25519_Sign("post-urbit:idoc:v1:" || JCS(...))`.
- **Impact:** Every implementation will disagree on signatures; test vectors cannot validate.
- **Fix:** Pick ONE normative rule and propagate everywhere:
  - Option A (recommended): **keep domain separation** and regenerate test vectors + all signature examples.
  - Option B: remove domain separator from RFC-0001 and keep existing docs/test vectors.

2) **DHT record authentication model contradicts itself (separate signature vs internal-only)**
- **Where:** `01-transport-connectivity/overview` (mentions separate DHT signature), `00-shared/layer-integration.md` (record format includes “Signature: 64 bytes” and bridge signs `idocBytes`), **RFC-0001 §12.2** (“No separate DHT signature required”)
- **Impact:** Transport/DHT API shape and storage validation differ across layers; implementers won’t know whether DHT `put()` requires an external signature.
- **Fix:** Normalize:
  - Make `dhtPut(..., { signature?: Uint8Array })` **optional**.
  - For IDOC values: DHT nodes MUST verify **full IDOC validity** (iid↔genesis binding + signature) and MAY ignore external signature.
  - For non-IDOC records (device index, revocation notices, etc.): require explicit external signature OR define an internal signature field normatively.

3) **Device Document schema is inconsistent across Identity schema, RFC-0001, and Glue spec**
- **Where:**  
  - `identity-document-schema.md` “Device Document” (no endpoints; fields: `device_name`, `signature_by_identity`)  
  - `00-shared/layer-integration.md` device doc includes `name`, `endpoints`, `signature` (field names differ)  
  - **RFC-0001 §13.2** device doc also lacks endpoints
- **Impact:** Multi-device connectivity can’t work as described (Transport needs per-device endpoints if main IDOC doesn’t list devices). Field name drift breaks verification and storage.
- **Fix:** Decide the normative Device Document format and use it everywhere:
  - If devices have distinct network presence: **Device Document MUST include `endpoints[]`** (canonical Endpoint schema) and standardize signature field name (`signature_by_identity` vs `signature`).
  - If identity-level endpoint only: then **remove device discovery via DHT as required for connectivity** and clarify multi-device is internal-only.

4) **Multi-device model contradicts itself regarding ratchet/session granularity**
- **Where:** `identity-document-schema.md` “Multi-Device Implications” says **ratchet session per (peer_iid, peer_did)**; `secure-envelope.md` declares **identity-level addressing** and “ratchet sessions per (sender_iid, recipient_iid)”; transport handshake supports DIDs.
- **Impact:** Messaging cannot be implemented correctly without deciding whether encryption sessions are per-device or per-identity. This affects envelope header fields, mailbox addressing, and ratchet state sync.
- **Fix (choose one, then align all docs):**
  - **Option A (identity-level sessions):** Update the multi-device implications table and explicitly specify how multiple devices share ratchet state (single “gateway device”, or state sync protocol, or deterministic conflict handling).
  - **Option B (per-device sessions):** Add DID(s) to PUSE header (sender_did and/or recipient_did), define mailbox addressing per device or fanout rules, and update all messaging interfaces accordingly.

5) **QUIC stream framing is not actually unified despite claims**
- **Where:**  
  - `peer-handshake.md`: Control stream = `stream_type(1 byte)` then **4-byte length + JSON** (no per-frame message type).  
  - `00-shared/layer-integration.md`: Identity stream frames include **message_type(1 byte) + length(4) + payload** and claims “consistent across all QUIC stream types.”
- **Impact:** Implementers will build incompatible parsers/framers; interop breaks immediately.
- **Fix:** Define one normative framing pattern per stream type (or one universal framing). If universal, adopt:  
  `stream_type(1)` then repeat `{ msg_type(1), len(4), payload(len) }`, with payload encoding (JSON/CBOR) per stream.

6) **App manifest signing is contradictory (SIGNATURE-file-only vs manifest.signature fields still present)**
- **Where:** `04-app-runtime/manifest-schema.md` contains multiple sections still referencing `manifest.signature` and even lists `signature.*` as required fields; `05-ux-packaging/app-distribution.md` mandates **SIGNATURE file** and says manifest has no signature.
- **Impact:** Packaging/install verification cannot be implemented consistently.
- **Fix:** Remove embedded `manifest.signature` requirements and all code paths that verify it, or explicitly define it as OPTIONAL compatibility metadata (and then make SIGNATURE file authoritative everywhere, including required-fields lists).

---

## HIGH (serious; can ship MVP only with explicit choices/workarounds)

7) **Recovery configuration schema drift between layers**
- **Where:** `recovery-mechanisms.md` device-escrow config uses `escrow_key_hash`; `05-ux-packaging/interfaces.md` uses `escrowDeviceDid`; `identity-document-schema.md` defines `recovery.method` and `config` but doesn’t reconcile these variants.
- **Impact:** Recovery can’t be configured/verified consistently.
- **Fix:** Pick a single canonical config per method and define (a) how it’s validated, (b) what is embedded in IDOC, (c) what Admin UI displays. If you want “device escrow” keyed by device DID, update recovery spec accordingly.

8) **Sequence number max bounds inconsistent**
- **Where:** `identity-document-schema.md` says max `"18446744073709551614"`; RFC-0001 allows `<= 2^64-1`; interfaces talk about max but not always consistent.
- **Impact:** Edge-case failures and reject/accept mismatches across nodes.
- **Fix:** Decide whether `2^64-1` is allowed or reserved and document it consistently (including regex/bounds and test vectors).

9) **Key history retention rules conflict**
- **Where:** `identity-document-schema.md` lists `keys.signing.history` “Max 3 entries” in one place, later says retain up to 10 or 2 years; `key-rotation.md` says keep old encryption public key “indefinitely” while schema says keep 5/30 days.
- **Impact:** Long-lived verification (apps, delayed mailbox) becomes unreliable if nodes prune differently.
- **Fix:** Make one retention policy normative and remove contradictory notes. If you need indefinite encryption pubkey retention, reflect that in schema (or explicitly state “previous[] is bounded; older keys require fetching historical IDOCs”).

10) **Revocation integration points are underspecified / not enforced in handshake & resolution**
- **Where:** `revocation.md` defines propagation + local revocation list; `peer-handshake.md` doesn’t mention checking revocation before accepting; caching policy doesn’t define a DHT record format/key for revocation notices.
- **Impact:** Nodes may authenticate with revoked identities/keys; compromised keys remain usable longer than intended.
- **Fix:** Add normative checks:
  - On identity resolution: consult revocation list before returning usable keys.
  - On handshake: if peer IID is revoked or signing key revoked effective_at <= now, abort with an error code.
  - Define DHT keys/value format for revocation notices (or state “revocations propagate only over gossip/peers + optional directory”).

11) **Secure Envelope plaintext examples conflict with stated format (message_id location)**
- **Where:** `secure-envelope.md` says message_id is only in header; `group-messaging.md` and some examples include `"id"` in plaintext.
- **Impact:** App/message model and storage indexing will diverge.
- **Fix:** Make it explicit: decrypted `Message.id` is derived from envelope header; plaintext MUST NOT include an `id` field (or allow it but declare it redundant/ignored).

12) **Device transport key appears unused**
- **Where:** `identity-document-schema.md` device doc includes `device_transport_key`; `peer-handshake.md` uses device *signing* key for DID proof, not transport key; QUIC uses TLS keys.
- **Impact:** Confusing/unimplemented crypto surface.
- **Fix:** Either (a) specify what `device_transport_key` is for (e.g., extra Noise binding, future E2E transport), or (b) remove it from v1 Device Document.

---

## MEDIUM (important cleanup; avoid interoperability surprises)

13) **Mixed “canonical JSON” claims for wire vs signing need one crisp rule**
- **Where:** `identity-document-schema.md` says document is signed over JCS without signatures; wire format says “Canonical JSON”; RFC-0001 adds “wire JSON MUST be JCS including signatures.”
- **Impact:** Byte-for-byte reproduction expectations differ; some nodes may transmit pretty JSON and still be “valid” but fail DHT hashing expectations if any.
- **Fix:** Define:
  - For signatures: always JCS(doc_without_signatures).
  - For wire encoding: either (a) MUST be JCS(full doc) or (b) MAY be any JSON; receivers JCS-normalize before verify. (If you need reproducible envelopes for storage hashing, choose (a).)

14) **Mailbox auth token canonicalization and signature domain separation not fully aligned with Identity rules**
- **Where:** `00-shared/layer-integration.md` mailbox token signs JCS without signature field; does not specify a domain separator (unlike some other signatures).
- **Impact:** Another signature-family divergence; replay/canonicalization bugs.
- **Fix:** Define a consistent signature-input pattern for all non-IDOC objects (relay allocate, mailbox token, revocations, contests): `domain_sep || JCS(without_signature_field)`.

15) **Admin UI / daemon TLS + cookie security inconsistencies**
- **Where:** `node-daemon.md` allows local HTTP without TLS; `admin-ui.md` says secure cookies always and “even localhost uses TLS”; session cookie config differs.
- **Impact:** Real deployments get insecure cookies or broken auth depending on which doc is followed.
- **Fix:** Decide: either “localhost may be HTTP” (then cookies must be `Secure=false` and possibly use loopback-only protections), or “always TLS” (then ship local cert workflow).

16) **Repository/update manifest signing payload encoding is ambiguous**
- **Where:** `app-distribution.md` uses payload strings like `"postapp-signature-v1:" || manifest_hash || ":" || timestamp` but doesn’t specify whether `manifest_hash` is hex string, raw bytes, or `sha256:...`.
- **Impact:** Signatures won’t verify across implementations.
- **Fix:** Make payload definition explicit and test-vector it (e.g., `payload = ASCII("postapp-signature-v1:") + HEX(hash)`).

---

## LOW (polish / editorial but worth doing)

17) **Example documents contain placeholder Base64 / invalid-looking values**
- **Where:** various example JSON snippets.
- **Impact:** Implementers copy/paste and fail validation; confusion.
- **Fix:** Mark placeholders clearly or replace with valid-length Base64 strings.

18) **Some pseudocode uses numeric comparisons on string sequences**
- **Where:** revocation/rotation verification snippets.
- **Impact:** Real implementations may accidentally do lexicographic compare.
- **Fix:** Add “parse as uint64/bigint” note everywhere sequence is compared.

19) **Conflict-handling rules appear in multiple places with slight wording differences**
- **Where:** identity-doc schema conflict resolution, rotation protocol concurrent updates, caching policy conflictState.
- **Impact:** Harder to implement consistently.
- **Fix:** Centralize conflict rules in one normative section and reference it.

---

## Net assessment vs core vision
- **Core vision alignment:** Mostly good (self-sovereign, rotatable, recoverable, portable; transport is untrusted; apps are capability sandboxed).
- **Main drift risk:** the current multi-device + messaging story is not internally consistent yet. You can still hit the vision, but only after choosing **identity-level** vs **per-device** cryptographic sessions and making the DHT/device discovery story consistent with that choice.

If you want, I can propose two coherent “v1 profiles” (Identity-level gateway vs Per-device sessions) and list exactly which paragraphs/schemas to change for each, so the whole spec becomes self-consistent quickly.
