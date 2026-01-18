## Iteration 63: DEEP DIVE

### Document Analysis
- `spec/01-transport-connectivity/relay-protocol.md`
- `spec/06-rfcs/RFC-0002-transport.md`
- `spec/02-identity-trust/identity-document-schema.md`
- `spec/00-shared/layer-integration.md`
- `spec/06-rfcs/RFC-0001-identity-document.md`

### Blocking Issues (B1, B2, etc.)
**B1 — Relay allocation/binding is internally inconsistent (HTTPS allocation vs UDP forwarding) and missing a normative UDP binding step + control-plane payload rules.**  
**Confirmed contradiction:** Allocation is specified as `POST /allocate HTTP/1.1` (HTTPS/TCP) while the relay forwarding plane (PURL) is UDP and requires the relay to know the client’s *public UDP* mapping. Current text states the relay “create[s] allocation bound to source IP:port” during HTTPS verification, but that source port is the TCP source port, not the UDP mapping needed for PURL forwarding—cannot work reliably (especially under symmetric NAT).  
**Also incomplete / non-interoperable:** PURL control packet types include `REBIND`/`REFRESH`/`RELEASE`, and RFC-0002 defines a signed **JSON** “REBIND Message”, but neither RFC-0002 nor `relay-protocol.md` normatively specifies:
- whether REBIND is carried **inside** a PURL packet payload (and if so, exact encoding and required header↔payload consistency checks), or sent over HTTPS (in which case the PURL packet-type registry is misleading), and
- payload formats for REFRESH/RELEASE/PING/PONG beyond ERROR.

**Why blocking:** Implementers cannot correctly implement relay receive-path routing (destination allocation lookup → forward to bound UDP address:port) with the current allocation step as written, and different teams will choose incompatible interpretations for REBIND/control-plane encoding.

**Minimum fix direction (normative):**
- Define a **two-step** model: HTTPS issues `(allocation_id, token)` but **does not establish UDP binding**; binding is established/updated by a **UDP control packet** (likely `PURL type=REBIND`) received from the client’s desired UDP socket, using the UDP source address:port of that datagram.
- Specify for `PURL type=REBIND`:
  - Destination IID field MUST be 20 zero bytes (control-plane).
  - Payload encoding (e.g., UTF-8 JSON, JCS-canonical JSON, or a fixed binary struct).
  - Signature input exactly (and whether it signs SHA256(signature_input) or raw input).
  - MUST check payload token == header token (or remove redundancy).
- Either define payload formats for `PING/PONG/REFRESH/RELEASE` (even if “empty payload”) or explicitly state they MUST be empty and unsigned.

---

**B2 — Device revocation is required but the discovery/publication mechanism is incomplete (DHT record format missing).**  
`identity-document-schema.md` states: “Peers MUST check for revocation before accepting connections from a device.” A `device_revocation` document and signature scheme exist (RFC-0001 §13.5), and `layer-integration.md` allocates a DHT key prefix `post-urbit:device-revocation:`—but there is **no normative DHT record format / TTL / conflict rules / verification and lookup behavior** for device revocations (unlike identity/key revocations which have DHT storage defined in `revocation.md` and `layer-integration.md`).

**Why blocking:** A transport implementer cannot meet the MUST-check requirement interoperably without a defined location and wire/value format to fetch device revocations (especially for first-contact / offline cases).

**Minimum fix direction (normative):**
- Add a “Device Revocation DHT Record” section (preferably in `00-shared/layer-integration.md` and referenced from RFC-0001 and transport handshake):
  - DHT Key: `SHA256("post-urbit:device-revocation:" || did_base32)` (confirm identifier input encoding).
  - DHT Value: JCS-canonical JSON of the `device_revocation` document (or a dedicated binary envelope if desired).
  - TTL (e.g., 365 days like other revocations).
  - DHT-node verification requirements (signature_by_identity verified against identity signing key; specify which identity key set to try—current/previous/history).
  - Client conflict resolution if multiple revocations exist (e.g., highest `revoked_at`, or first-seen-wins if equal).
  - Caching guidance for transport handshake (acceptable staleness / negative caching).

### Minor Issues (M1, M2, etc.)
**M1 — Base32 case-handling is not fully consistent across docs (reject vs normalize).**  
Some sections say “reject uppercase or normalize”; others say “accept lowercase only.” This is mostly non-fatal since on-wire is specified lowercase, but it should be made consistent (recommend: accept uppercase in user input, normalize to lowercase; reject non-canonical on-wire fields).

**M2 — Relay control-plane redundancy/clarity:**  
REBIND JSON includes `token` while PURL header already carries the token. Either normatively require equality (and define which one is authoritative) or remove one to reduce ambiguity.

**M3 — Device document verification key selection:**  
Several places say verify `signature_by_identity` using identity’s “current (or historical)” signing key, but don’t specify the exact lookup order/constraints (current→previous→history and whether to consider `expires_at`). This is implementable but under-specified compared to the PUSE signature verification guidance.

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[✓] CONTINUE — N blocking issues require fixes
