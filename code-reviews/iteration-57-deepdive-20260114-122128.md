## Iteration 57: DEEP DIVE

### Document-by-Document Analysis

- Document: spec/00-overview/success-criteria.md
- Issues found: None

- Document: spec/00-overview/vision.md
- Issues found: None

- Document: spec/00-shared/layer-integration.md
- Issues found:
  - Minor: Calls the DHT “content-addressable storage” while also defining a fixed key-derivation scheme `SHA256(prefix||identifier)`; that’s *key-addressed* (content-hash-based), not CAS by value.

- Document: spec/00-shared/test-vectors.md
- Issues found:
  - Minor: Test Vector 7 nonces use a “decimal-looking hex” sequence (`...08091011...`) which is valid but atypical; could confuse implementers expecting `0a0b0c...`.

- Document: spec/01-transport-connectivity/interfaces.md
- Issues found:
  - Minor: `ConnectOptions.timeout?: number` is not labeled as ms vs seconds; elsewhere constants are `_MS`, creating ambiguity.

- Document: spec/01-transport-connectivity/nat-traversal.md
- Issues found:
  - Minor: NAT type detection pseudocode is not aligned with the NAT type definitions (the “FULL_CONE vs PORT_RESTRICTED” branch does not follow from the described probes).

- Document: spec/01-transport-connectivity/overview.md
- Issues found: None

- Document: spec/01-transport-connectivity/peer-handshake.md
- Issues found: None

- Document: spec/01-transport-connectivity/quic-integration.md
- Issues found:
  - Minor: TLS config table says “Signature: Ed25519 (for identity binding)” but identity binding is explicitly post-TLS (handshake), not via the TLS cert.

- Document: spec/01-transport-connectivity/relay-protocol.md
- Issues found: None

- Document: spec/02-identity-trust/caching-policy.md
- Issues found: None

- Document: spec/02-identity-trust/identity-document-schema.md
- Issues found:
  - Minor: “Invalid examples: `hello_world` (contains invalid chars `i`, `l`, `o`)” also contains `_` which is invalid; the reason list is incomplete.

- Document: spec/02-identity-trust/interfaces.md
- Issues found: None

- Document: spec/02-identity-trust/key-rotation.md
- Issues found: None

- Document: spec/02-identity-trust/name-resolution.md
- Issues found: None

- Document: spec/02-identity-trust/overview.md
- Issues found: None

- Document: spec/02-identity-trust/recovery-mechanisms.md
- Issues found: None

- Document: spec/02-identity-trust/revocation.md
- Issues found: None

- Document: spec/03-messaging-sync/double-ratchet.md
- Issues found:
  - Minor: Pseudocode uses `state.sending_chain_key` / `state.dh_sending_key` while the TypeScript state definition uses `sendingChainKey` / `dhSendingKey`; inconsistent naming within the document.

- Document: spec/03-messaging-sync/group-messaging.md
- Issues found: None

- Document: spec/03-messaging-sync/interfaces.md
- Issues found: None

- Document: spec/03-messaging-sync/overview.md
- Issues found: None

- Document: spec/03-messaging-sync/secure-envelope.md
- Issues found:
  - Minor: `verify_envelope(..., envelope_timestamp=None)` parameter is never used; text claims it helps for historical key selection.

- Document: spec/03-messaging-sync/sync-protocol.md
- Issues found:
  - **BLOCKING:** Later sections show JSON “type”: `"SYNC_SUBSCRIBE"` / `"SYNC_OPERATIONS"` examples that contradict the earlier **normative** wire format (1-byte message type + CBOR).

- Document: spec/04-app-runtime/abi.md
- Issues found:
  - **BLOCKING:** `host.get_result` return codes conflict with later “Result Envelope Format” rule that negative values are *transport-level only* and application-level errors are *always* inside the CBOR envelope.

- Document: spec/04-app-runtime/api-surface.md
- Issues found: None (but inherits ABI ambiguity via reference to `abi.md`)

- Document: spec/04-app-runtime/capability-system.md
- Issues found: None

- Document: spec/04-app-runtime/interfaces.md
- Issues found: None

- Document: spec/04-app-runtime/manifest-schema.md
- Issues found:
  - Minor: Package directory listing says `manifest.json # Required: App manifest with signature` despite earlier repeated rule that signatures are in `SIGNATURE`, not in `manifest.json`.

- Document: spec/04-app-runtime/overview.md
- Issues found: None

- Document: spec/04-app-runtime/wasm-sandbox.md
- Issues found: None

- Document: spec/05-ux-packaging/admin-ui.md
- Issues found: None

- Document: spec/05-ux-packaging/app-distribution.md
- Issues found:
  - **BLOCKING:** Verification steps reconstruct/compare `manifest_hash` inconsistently (bytes vs hex string vs `"sha256:<hex>"`), contradicting the signing process description.
  - Minor: Repository manifest example shows `"signature": "<repository-operator-signature>"` (string) while later defines a structured `signature` object.

- Document: spec/05-ux-packaging/deployment.md
- Issues found: None

- Document: spec/05-ux-packaging/interfaces.md
- Issues found: None

- Document: spec/05-ux-packaging/node-daemon.md
- Issues found: None

- Document: spec/05-ux-packaging/observability.md
- Issues found: None

- Document: spec/05-ux-packaging/overview.md
- Issues found: None

- Document: spec/06-rfcs/RFC-0001-identity-document.md
- Issues found: None

- Document: spec/06-rfcs/RFC-0002-transport.md
- Issues found: None

- Document: spec/06-rfcs/RFC-0003-messaging.md
- Issues found: None

- Document: spec/progress.md
- Issues found:
  - Minor: Claims “No contradictions between specs” but current deep-dive found confirmed internal contradictions (listed below).

---

### Blocking Issues (B1, B2, etc.)

B1: spec/04-app-runtime/abi.md — **`host.get_result` return codes contradict “errors are always in CBOR envelope”**
- What's wrong: `host.get_result` defines `-3` as “Call failed (result contains CBOR error)”, but later the ABI states that negative return values are transport-level only and application-level errors are *always* represented inside the CBOR result envelope. This is internally inconsistent and leaves apps unsure whether to parse the buffer when `-3` is returned.
- Evidence:
  - Return codes section:  
    > “`-3`: Call failed (result contains CBOR error)”
  - Result envelope section:  
    > “Application-level errors are always returned inside the CBOR envelope with `ok: false`.”  
    > “The return value of `get_result` indicates transport-level status only: … Negative: transport error …”
- Exact fix:
  - In `host.get_result` return values, **remove** the `-3` “Call failed” case and require that call failures return `> 0` bytes containing a CBOR envelope with `ok: false`.
  - Update text to make `host.get_result` negatives strictly: `-1 invalid call_id`, `-2 buffer too small` (and reserve any additional negatives for transport-only errors).
  - If you want a distinct “call failed” status, define it as a CBOR envelope error only (no negative code), and delete/replace all references to `-3`.

B2: spec/05-ux-packaging/app-distribution.md — **Manifest hash comparison/payload reconstruction inconsistent (bytes vs hex vs “sha256:” string)**
- What's wrong: The signing process uses `manifest_hash_hex` in the payload and stores `signed_manifest_hash` as `"sha256:<hex>"`, but the verification process mixes raw bytes and strings:
  - It says “Compute manifest_hash = SHA256(...)” and then “Verify: manifest_hash == signed_manifest_hash” (type mismatch).
  - It reconstructs the payload using `manifest_hash` (not hex), contradicting the signing process.
- Evidence:
  - Signing process:  
    > “Compute: `manifest_hash_hex = hex(SHA256(...))`”  
    > “payload = `postapp-signature-v1:` || `manifest_hash_hex` || `:` || `timestamp`”
  - Verification process:  
    > “Compute `manifest_hash = SHA256(canonical_manifest_bytes)`”  
    > “Verify: `manifest_hash == signed_manifest_hash`”  
    > “Reconstruct payload = `postapp-signature-v1:` || `manifest_hash` || `:` || `timestamp`”
- Exact fix:
  - Change verification to:
    1. `manifest_hash_hex = hex(SHA256(canonical_manifest_bytes))`
    2. `expected_signed_manifest_hash = "sha256:" + manifest_hash_hex`
    3. Verify `expected_signed_manifest_hash == signed_manifest_hash`
    4. Reconstruct payload as `postapp-signature-v1:` + `manifest_hash_hex` + `:` + `timestamp`
  - Ensure terminology is consistent: use `manifest_hash_hex` for the payload string component, and `"sha256:<hex>"` for `signed_manifest_hash`.

B3: spec/03-messaging-sync/sync-protocol.md — **JSON examples contradict the normative CBOR wire format**
- What's wrong: The document defines sync stream payloads as **(1-byte message type + CBOR)** with a numeric registry (0x01–0x07) and CBOR schemas, but later sections show JSON objects with `"type": "SYNC_SUBSCRIBE"` / `"type": "SYNC_OPERATIONS"` and string `"document_id": "<doc-id>"`, contradicting the earlier normative encoding.
- Evidence:
  - Normative wire format:  
    > “Sync Payload … `Message Type` 1 byte … `CBOR Data` …”  
    > “Message Types: `0x06 = SYNC_SUBSCRIBE` …”
  - Contradicting later examples:  
    > ```json
    > { "type": "SYNC_SUBSCRIBE", "document_id": "<doc-id>", "from_hlc": { ... } }
    > ```
    > ```json
    > { "type": "SYNC_OPERATIONS", "document_id": "<doc-id>", "operations": [...] }
    > ```
- Exact fix:
  - Replace those JSON examples with **wire-accurate examples**, e.g.:
    - “Message type byte = `0x06`, CBOR map = `{ "document_id": <32-byte bstr>, "from_hlc": <Timestamp map> }`”
  - If you want to keep JSON for readability, add an explicit banner: “Illustrative logical structure (NOT on-wire); on-wire encoding is 1-byte type + CBOR per §Sync Wire Protocol”, and ensure fields match CBOR schemas (binary `bstr` sizes, not string IDs).

---

### Minor Issues (M1, M2, etc.)

M1: spec/04-app-runtime/manifest-schema.md — Directory listing implies manifest contains signature
- Fix: Change `manifest.json  # Required: App manifest with signature` → `manifest.json  # Required: App manifest (unsigned); signature is in SIGNATURE file`.

M2: spec/05-ux-packaging/app-distribution.md — Repository manifest example signature field type mismatch
- Fix: Update the initial `repository.json` example to use the structured `signature` object format defined later (or clearly label the earlier snippet as outdated/non-normative).

M3: spec/01-transport-connectivity/interfaces.md — Timeout unit ambiguity
- Fix: Specify `timeout?: number` is milliseconds (or rename to `timeoutMs`), matching the constant naming.

M4: spec/03-messaging-sync/double-ratchet.md — Mixed variable naming between TS and pseudocode
- Fix: Either (a) rename pseudocode fields to match TS (`sendingChainKey`, etc.) or (b) add a note that pseudocode uses snake_case and provide a mapping.

---

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — N blocking issues require fixes
