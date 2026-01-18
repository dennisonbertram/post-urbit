## Iteration 61: DEEP DIVE

### Document Analysis
Documents with remaining internal inconsistencies or spec gaps:

- `spec/06-rfcs/RFC-0002-transport.md`
- `spec/01-transport-connectivity/interfaces.md`
- `spec/01-transport-connectivity/relay-protocol.md`
- `spec/04-app-runtime/abi.md`
- `spec/04-app-runtime/api-surface.md`
- `spec/04-app-runtime/capability-system.md`
- `spec/03-messaging-sync/group-messaging.md`
- `spec/03-messaging-sync/double-ratchet.md`
- `spec/00-shared/layer-integration.md`
- `spec/02-identity-trust/key-rotation.md`
- `spec/04-app-runtime/manifest-schema.md`
- `spec/02-identity-trust/identity-document-schema.md`
- `spec/06-rfcs/RFC-0001-identity-document.md` (minor TTL language mismatch)

---

### Blocking Issues (B1, B2, etc.)

**B1 — PURL payload size is internally contradictory (65535 vs 1200), leading to interoperability ambiguity**  
- **Where:**
  - RFC-0002 §7.4: “Max payload: 65535 bytes (limited by u16)”
  - RFC-0002 §4.2 + Transport constants: `max_udp_payload_size = 1200`, `MAX_UDP_PAYLOAD = 1200`, and `RELAY_PACKET_MAX_SIZE = 1244 (=44+1200)`
- **Why blocking:** A sender could be “spec-conformant” under §7.4 sending >1200, but the QUIC config (and other docs) normatively constrain UDP payload to 1200. That creates non-deterministic acceptance/rejection behavior across implementations.
- **Fix:** Make a single normative rule:
  - Either (recommended for v1) **MUST set and enforce PURL Payload Length ≤ 1200** (or ≤ negotiated QUIC `max_udp_payload_size`), and update RFC-0002 §7.4 to state the u16 field is *capacity*, not the v1 allowed size; OR
  - Define explicit PMTU discovery/allowance rules and corresponding acceptance requirements (more complex; must propagate to constants and test vectors).
  - Add an explicit receiver error behavior for oversize PURL payloads (drop + optional ERROR packet).

**B2 — WASM ABI has an unresolved memory ownership contract for guest→host return buffers (handle/get_error), contradicting “host never frees guest memory”**  
- **Where:** `spec/04-app-runtime/abi.md`
  - Host/guest allocation rules only cover host writing into guest memory via `alloc/dealloc`.
  - `handle` returns `(ptr<<32)|len` for a result located in guest memory, but **no rule defines buffer lifetime or deallocation responsibilities**.
  - Stated rule: “Host never frees guest memory directly.”
- **Why blocking:** Implementations cannot safely implement the ABI without either:
  - leaking memory across invocations, or
  - inventing non-specified conventions (host calls `dealloc`, or guest uses static buffers, etc.).  
  This affects correctness and compatibility of third-party apps.
- **Fix (normative):** Specify one of:
  1. **Host-copies + guest-frees model:** Host MUST copy result bytes immediately; guest MUST keep buffer valid until `handle` returns; guest MAY reuse/free after return. No host `dealloc`. (Also define max result size enforcement and what happens on OOB pointers.)
  2. **Host-frees model:** Host MUST call exported `dealloc(ptr,len)` after copying result. This directly contradicts current “host never frees” text—so you must change that rule (or scope it to “host never frees *host-provided* buffers”).
- **Also missing error cases:** Define host behavior when `(ptr,len)` is 0, out of bounds, overlaps, or exceeds max (1MB). Currently undefined.

**B3 — Sender-key KDF input encoding is inconsistent/underspecified (raw bytes vs string), risking incompatible group encryption**  
- **Where:**
  - `spec/03-messaging-sync/group-messaging.md` defines `SenderKey.senderIid: string` and `keyId: string` but the KDF in `double-ratchet.md` expects `bytes`.
  - `spec/03-messaging-sync/double-ratchet.md` defines `kdf_sender_key(chain_key: bytes, group_id: bytes, sender_iid: bytes, key_id: bytes)` and concatenates `group_id + b":" + sender_iid + b":" + key_id`.
  - `spec/00-shared/layer-integration.md` domain separator registry says sender-key KDF prefix is followed by binding data `group_id:sender_iid:key_id` but does **not** specify the encoding (raw vs Base32/Base64).
- **Why blocking:** If any implementation uses Base32 strings (32 bytes) vs raw 20-byte IDs, derived message keys differ → group messages become undecryptable across implementations.
- **Fix (normative, single source of truth):**
  - Specify exact encodings for KDF binding fields (recommended):
    - `group_id_raw` = 20 bytes
    - `sender_iid_raw` = 20 bytes
    - `key_id_raw` = 16 bytes
    - Remove `:` separators (not needed with fixed-length fields), **or** keep them but state explicitly they are literal byte `0x3a` and inputs are fixed-length raw bytes.
  - Update `group-messaging.md` SenderKey interface/types and all pseudocode to match RFC-0003’s byte-oriented definition.

**B4 — Host API method registry is inconsistent with its own documented request/response schemas (many methods listed but not specified)**  
- **Where:**
  - `abi.md` method string list includes (among others): `storage.shared.get/set`, `messaging.unsubscribe`, `messaging.list_groups`, `contacts.get`, `sync.get_document`, `sync.subscribe`, `sync.share`, `notifications.cancel`, `app.invoke`, `app.share`.
  - `capability-system.md` includes capability requirements for many of these methods.
  - `api-surface.md` only defines detailed CBOR schemas for a subset (e.g., `storage.get/set/delete/list`, `messaging.send/subscribe/create_group`, `contacts.list/list_app_users`, `sync.create_document/apply_operation`, `notifications.show/set_badge`, `system.get_time/get_random/get_deterministic_random/get_identity/get_app_info`, `app.invoke/app.share` partially).
- **Why blocking:** Apps cannot interoperably call methods if CBOR argument/result shapes and error codes are undefined. Implementers will diverge.
- **Fix:** Either:
  - **Fully specify** request/response CBOR schemas + error codes for every method listed in `abi.md` and mapped in `capability-system.md`, **or**
  - **Remove**/deprecate methods from the authoritative registry until specified, and ensure CAPABILITY_MAP doesn’t reference nonexistent methods.

---

### Minor Issues (M1, M2, etc.)

**M1 — Key rotation example uses numeric `sequence` instead of required decimal string**  
- **Where:** `spec/02-identity-trust/key-rotation.md` step 2 example: `"sequence": N + 1` (number-like) vs schema requiring string.
- **Fix:** Quote sequence and keep it a decimal string everywhere (`"sequence": "N+1"` or show computed literal).

**M2 — App manifest/package signing phrasing conflict (“manifest with signature”)**  
- **Where:** `spec/04-app-runtime/manifest-schema.md` directory structure line: `manifest.json # Required: App manifest with signature` contradicts “SIGNATURE file only” approach repeated elsewhere.
- **Fix:** Change to “manifest.json (unsigned; signature in SIGNATURE file)”.

**M3 — Capability system references undefined capability `app:delegate`**  
- **Where:** `spec/04-app-runtime/capability-system.md` delegation rules mention `app:delegate`, but it’s not defined in capability categories nor mapped.
- **Fix:** Define `app:delegate` (and any related host API) or mark delegation as out-of-scope and remove the capability reference.

**M4 — Some identity document example signatures/keys are “example-like” and may violate Base64 length constraints**  
- **Where:** `spec/02-identity-trust/identity-document-schema.md` Example Documents contain placeholder Base64 strings not guaranteed to decode to required sizes.
- **Fix:** Mark them explicitly as placeholders (non-conformant) or replace with valid-length Base64 (or refer readers to `00-shared/test-vectors.md` only).

**M5 — “TTL: Forever” wording conflicts with TTL-enforced DHT requirements**  
- **Where:** `spec/06-rfcs/RFC-0001-identity-document.md` §12.3 suggests genesis TTL “Forever”, while `layer-integration.md` requires TTL enforcement/expiry.
- **Fix:** Replace “Forever” with a concrete large TTL (e.g., 365 days) and keep TTL semantics consistent (“expire after TTL seconds”).

**M6 — Terminology drift: X3DH vs 2DH naming across layer docs**  
- **Where:** `secure-envelope.md` and `double-ratchet.md` still use “X3DH” wording in places, while RFC-0003 renamed the protocol to 2DH (while keeping domain separator `post-urbit-x3dh-v1`).
- **Fix:** Align terminology (“2DH (domain label remains post-urbit-x3dh-v1 for compatibility)”).

**M7 — Relay allocation request example should explicitly say HTTPS**  
- **Where:** `spec/01-transport-connectivity/relay-protocol.md` shows `POST /allocate HTTP/1.1` without explicitly stating HTTPS, while RFC-0002 §7.8 says HTTPS.
- **Fix:** Make relay allocation endpoint explicitly HTTPS everywhere.

---

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — N blocking issues require fixes
