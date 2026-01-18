# Verification Harness

## Purpose

Provide a lightweight, deterministic, end-to-end harness that proves the system works as intended, not just that unit tests pass. The harness must be easy for a human (and an LLM) to interpret and must produce durable evidence that the user journeys and spec-defined behaviors are actually achieved.

This is the primary mechanism for answering: "Is the software complete, and does it behave as specified?"

## Design Goals

- Deterministic and reproducible: same inputs and seed yield identical outcomes.
- Black-box first: validate externally visible behavior and observable state, not internal implementation details.
- Minimal dependencies: run on a laptop or CI with no special services.
- Clear evidence: outputs include a human-readable summary and machine-readable artifacts.
- Low flake rate: failures should indicate real regressions or spec gaps, not timing noise.
- Layer coverage: every architecture layer is exercised in at least one scenario.

## Scope Coverage (Minimum)

The harness MUST cover: [REQ-OVR-005]

- All items in `spec/00-overview/success-criteria.md` (functional + operational + user journey).
- The spec completeness criteria (data structures, interfaces, state machines, wire formats, error handling, dependencies, test scenarios, security, no TBDs).
- Protocol conformance using `spec/00-shared/test-vectors.md` and RFC-defined canonicalization rules.
- Failure and recovery paths that are explicitly described in each component spec.
- Multi-node interaction across NAT, relay, and offline-delivery conditions.

## Harness Architecture (Conceptual)

```
┌──────────────────────────────────────────────┐
│                   Harness                    │
│                                              │
│  Scenario Runner  ──┐   Evidence Builder     │
│  Conformance Suite ─┼─> Summary + Artifacts  │
│  Fault Injector    ─┘                        │
│                                              │
│  Observability Collector (logs/events/state) │
└──────────────────────────────────────────────┘
            │           │           │
         Node A       Node B      Relay
```

### Core Components

1. Scenario Runner
   - Drives end-to-end user journeys using deterministic steps.
   - Orchestrates multiple nodes and external conditions (NAT, offline, relay).
   - Validates observable outcomes after each step.

2. Conformance Suite
   - Verifies cryptographic and wire-format correctness via test vectors.
   - Includes protocol invariants and canonicalization checks.
   - Produces explicit "pass/fail with expected vs actual" for each vector.

3. Fault Injector
   - Simulates unreliable networks, peer downtime, and message delays.
   - Supports toggles for NAT, relay-only delivery, and offline persistence.

4. Observability Collector
   - Captures logs, structured events, and state snapshots.
   - Provides cross-node correlation by run ID and scenario step ID.

5. Evidence Builder
   - Produces a compact evidence bundle per run:
     - `summary.md` (human-readable)
     - `summary.json` (machine-readable)
     - `events.ndjson` (ordered event log)
     - `artifacts/` (state snapshots, db hashes, receipts)

## Execution Model

### Topologies

The harness MUST support the following minimal topologies: [REQ-OVR-006]

- 2 nodes (Alice/Bob) direct connectivity.
- 2 nodes + relay (relay required for delivery).
- 3 nodes (Alice/Bob/Carol) for group messaging and multi-device sync.

### Determinism

- Use deterministic seeds for key generation and message contents.
- Time is controlled by the harness (virtual clock) so retries and cooldowns are predictable.
- Any randomness must be seedable and recorded in the evidence bundle.

### Isolation

- Each run uses a fresh data directory for each node.
- No shared state across runs unless explicitly testing upgrade paths.
- All network artifacts are internal to the harness environment.

## Scenario Definition Format

Scenarios are declarative and map directly to success criteria IDs and spec requirements.

Example (YAML, illustrative):

```yaml
id: SCEN-JOURNEY-04
title: "Message across NAT with relay fallback"
success_criteria:
  - SC-CONN-02  # NAT traversal and optional relays
  - SC-MSG-01   # E2E encrypted 1:1
requirements:
  - REQ-TRANS-PEER-HANDSHAKE-07
  - REQ-MSG-PUSE-12
tests:
  - TEST-JOURNEY-04
topology: [alice, bob, relay]
steps:
  - id: S1
    action: node.create_identity
    actor: alice
  - id: S2
    action: node.create_identity
    actor: bob
  - id: S3
    action: network.set_nat
    params: { bob: "symmetric_nat" }
  - id: S4
    action: node.add_contact
    actor: alice
    params: { target: bob }
  - id: S5
    action: node.send_message
    actor: alice
    params: { to: bob, body: "hello" }
  - id: S6
    action: assert.message_received
    actor: bob
    params: { from: alice, body: "hello" }
  - id: S7
    action: assert.delivery_path
    params: { via: "relay" }
```

## Action Catalog (Initial)

This catalog defines the action names used in `spec/00-overview/scenarios.yaml` and the expected observable outcome. Add new actions here before using them in scenarios.

### Action Conventions

- `actor` must be one of the `topology` entries for the scenario. For `network.*` and `vectors.*` actions, `actor` MAY be omitted (these are harness-level actions).
- `params` keys are `snake_case`. Omitted optional params use the defaults listed below.
- IDs used in evidence (`group_id`, `backup_id`, `message_id`) SHOULD be deterministic; if omitted, harness derives them from run seed + step id.
- `node_id` values in params must match the scenario topology (`alice`, `bob`, `carol`, `relay`).

### Node Actions

- `node.install`: Start a node and expose admin UI. params: `{ version?: string, data_dir?: string, admin_port?: number }`. evidence: admin health response or UI status snapshot.
- `node.create_identity`: Create a new identity. params: `{ display_name?: string }`. evidence: IDOC snapshot (canonical JSON) + derived IID.
- `node.exchange_identity`: Fetch and cache another node's identity document. params: `{ target: node_id }`. evidence: cached IDOC hash + source endpoint.
- `node.add_contact`: Create a contact entry for another IID. params: `{ target: node_id }`. evidence: contact list snapshot including target IID.
- `node.write_data`: Write a local key/value pair to node storage. params: `{ key: string, value: string }`. evidence: storage snapshot or key hash.
- `node.send_message`: Send a direct PUSE message. params: `{ to: node_id, body: string, message_id?: string }`. evidence: message ID + envelope hash + send receipt.
- `node.sync`: Initiate sync of a document or app dataset. params: `{ target: node_id, doc: string }`. evidence: sync session summary + operation IDs.
- `node.backup`: Create encrypted backup artifact. params: `{ backup_id: string, password?: string }`. evidence: backup file hash + metadata entry.
- `node.restore`: Restore from a backup artifact. params: `{ backup_id: string, password?: string }`. evidence: restore receipt + state hash after restore.
- `node.simulate_key_loss`: Remove or corrupt local key material. params: `{ kind?: "delete_keys" | "corrupt_keys" }` (default `delete_keys`). evidence: identity state shows missing keys.
- `node.recover_identity`: Execute recovery flow. params: `{ method?: "social" | "device-escrow" | "threshold" | "provider" | "none" }` (default `social`). evidence: new IDOC version + recovery proof details.
- `node.rotate_identity_keys`: Rotate signing/encryption keys. params: `{ kind?: "signing" | "encryption" | "both" }` (default `both`). evidence: new IDOC with updated keys.
- `node.install_app`: Install a `.postapp` package. params: `{ app: string, source?: "file" | "url" | "repository" }` (default `file`). evidence: installed app list entry + manifest hash.
- `node.install_version`: Install a specific node binary version. params: `{ version: string }`. evidence: node version info.
- `node.upgrade`: Upgrade node to a newer version. params: `{ version: string }`. evidence: version change + migration summary.
- `node.generate_activity`: Produce loggable activity for observability checks. params: `{ events?: string[] }` (default `["message_sent"]`). evidence: events emitted.
- `node.simulate_abuse`: Trigger abuse-control logic (rate limit, block, drop). params: `{ kind?: "oversize_relay_payload" | "rate_limit" | "spam" }` (default `oversize_relay_payload`). evidence: enforcement event.
- `node.send_tampered_message`: Send a malformed or invalid-signature message. params: `{ to: node_id, tamper?: "signature" | "header" | "ciphertext" }` (default `signature`). evidence: rejected receipt on recipient.

### Network Actions

- `network.set_nat`: Set NAT type for one or more nodes. params: `{ <node_id>: "symmetric_nat" | "cone" | "none" }`. evidence: network model state.

### App Actions

- `app.create_document`: Create or modify an app document. params: `{ doc: string, content?: object }`. evidence: doc ID + state hash.

### Group Actions

- `group.create`: Create a group with members. params: `{ group_id: string, members: node_id[] }`. evidence: group ID + membership snapshot.
- `group.send_message`: Send a group message. params: `{ group_id: string, body: string, message_id?: string }`. evidence: message ID + delivery receipts.

### Assertions

- `assert.admin_ui_reachable`: Verify admin UI or health endpoint is reachable. params: `{ url?: string }`. evidence: HTTP response snapshot.
- `assert.identity_exists`: Verify identity exists locally. params: `{ iid?: string }`. evidence: IDOC or identity metadata.
- `assert.contact_added`: Verify contact record created. params: `{ target: node_id }`. evidence: contact list snapshot.
- `assert.message_received`: Verify recipient received a message. params: `{ from: node_id, body: string, message_id?: string }`. evidence: message receipt.
- `assert.delivery_path`: Verify delivery path (direct vs relay). params: `{ via: "direct" | "relay" }`. evidence: routing metadata.
- `assert.app_installed`: Verify app is installed. params: `{ app: string }`. evidence: installed app list snapshot.
- `assert.sync_converged`: Verify document state matches across nodes. params: `{ doc: string }`. evidence: matching state hashes.
- `assert.data_restored`: Verify a local key/value exists after restore. params: `{ key: string, value: string }`. evidence: storage snapshot including key/value.
- `assert.identity_recovered`: Verify identity recovered after key loss. params: `{ iid?: string }`. evidence: new IDOC version.
- `assert.group_message_received`: Verify group message delivery. params: `{ group_id: string, from: node_id, body: string }`. evidence: receipts on members.
- `assert.data_migrated`: Verify data after upgrade. params: `{ from_version?: string, to_version?: string }`. evidence: state hash comparison.
- `assert.logs_sanitized`: Verify logs contain no user content. params: `{ redaction_rules?: string[] }`. evidence: log scrub report.
- `assert.abuse_controls_applied`: Verify abuse controls triggered. params: `{ kind?: string }`. evidence: enforcement event.
- `assert.rejected`: Verify invalid message rejected. params: `{ reason?: string }`. evidence: rejection receipt.

### Vector Suite

- `vectors.run_all`: Execute all test vectors; evidence: vector pass/fail summary and per-vector diffs.

## Test Catalog (Initial)

Each `TEST-*` is a harness test case referenced by scenarios. Journey tests MUST include at least one negative subcase (failure mode).

- `TEST-JOURNEY-01`: Install node + admin UI reachable; negative: unauthenticated admin request rejected.
- `TEST-JOURNEY-02`: Claim identity; negative: uppercase IID rejected per REQ-IDOC-003/REQ-IDOC-004.
- `TEST-JOURNEY-03`: Add contact; negative: invalid IDOC signature rejected per REQ-ID-004.
- `TEST-JOURNEY-04`: NAT + relay messaging; negative: tampered header extension causes AAD mismatch and decrypt failure (REQ-MSG-039).
- `TEST-JOURNEY-05`: Install app; negative: missing SIGNATURE file rejected per REQ-APP-034.
- `TEST-JOURNEY-06`: Sync data; negative: invalid SyncOperation signature or non-deterministic CBOR rejected per REQ-SYNC-007/REQ-SYNC-008.
- `TEST-JOURNEY-07`: Recover identity; negative: recovery before cooldown rejected per REQ-ID-030.

- `TEST-NODE-01`: Encrypted backup/restore round-trip.
- `TEST-GROUP-01`: Group messaging delivery across members.
- `TEST-KEY-ROTATION-01`: Key rotation chain preserves verification (REQ-IDOC-020/REQ-IDOC-022).
- `TEST-OPS-01`: Upgrade path; update manifest signature validation (REQ-OPS-012).
- `TEST-OPS-02`: Observability redaction; logs avoid user content (REQ-SHARED-033).
- `TEST-OPS-03`: Abuse controls; oversize relay payload dropped (REQ-TRANS-068).
- `TEST-FAIL-01`: Invalid signature rejected (REQ-MSG-051).

Vector tests (map 1:1 to `spec/00-shared/test-vectors.md`):

- `TEST-VEC-001`: Test Vector 1 (IID derivation)
- `TEST-VEC-002`: Test Vector 2 (IDOC signature)
- `TEST-VEC-003`: Test Vector 3 (Bob identity)
- `TEST-VEC-004`: Test Vector 4 (KDF chain step)
- `TEST-VEC-005`: Test Vector 5 (Root chain KDF)
- `TEST-VEC-006`: Test Vector 6 (2DH key agreement)
- `TEST-VEC-007`: Test Vector 7 (Handshake challenge)
- `TEST-VEC-008`: Test Vector 8 (DID derivation)
- `TEST-VEC-009`: Test Vector 9 (SyncOperation signature)
- `TEST-VEC-010`: Test Vector 10 (PUSE initial envelope)
- `TEST-VEC-011`: Test Vector 11 (PUSE ratchet envelope)

## Suites

### 1. Journey Suite (Primary)

Covers the user journey list in `spec/00-overview/success-criteria.md`:

- Install node -> admin UI reachable
- Claim identity -> key generation + name assignment
- Add contact -> identity exchange + verification
- Message across NAT -> direct or relay, transparent
- Install app -> signed package verification + permissions
- Sync app data -> local-first replication
- Recover identity -> configured recovery mechanism

Each journey MUST have: [REQ-OVR-007]

- A deterministic scenario
- A reproducible evidence bundle
- At least one negative test (failure mode)

### 2. Protocol Conformance Suite

- Runs all vectors in `spec/00-shared/test-vectors.md`.
- Verifies canonicalization and signature inputs (JCS, byte formats).
- Ensures envelope parsing and error handling match RFCs.

### 3. Failure and Recovery Suite

- Corrupt data handling (invalid signatures, malformed documents).
- Offline storage and delayed delivery (store-and-forward).
- Key rotation, recovery proofs, and sequence validation.

### 4. Upgrade and Compatibility Suite

- Start on version N, upgrade to N+1, ensure data remains valid.
- Verify old identities and messages can still be verified.

### 5. Security and Abuse Control Suite

- Abuse control mechanisms behave as specified without centralized moderation.
- Logs and metrics do not leak user content outside explicit test data.

## Assertions and Evidence

Assertions must be observable and externally verifiable. Examples:

- API response codes and payloads.
- Database state hash or key counts (not raw data unless explicitly needed).
- Message receipts and delivery paths (direct vs relay).
- Cryptographic verification results against vectors.
- Admin UI / CLI status outputs captured as snapshots.

Each assertion must:

- Reference a requirement ID (REQ-*) or success criterion (SC-*).
- Emit an evidence artifact (path recorded in `summary.json`).

## Evidence Bundle Layout

```
runs/<run-id>/
  summary.md
  summary.json
  config.yaml
  events.ndjson
  nodes/
    alice/
      logs/
      state_snapshot.json
      db_hash.txt
    bob/
      logs/
      state_snapshot.json
      db_hash.txt
  artifacts/
    receipts/
    wire_captures/
```

### Evidence File Schemas (Minimal)

`summary.json` (example fields; extend as needed):

```json
{
  "run_id": "2025-01-15-A1",
  "seed": "post-urbit-harness-seed",
  "started_at": "2025-01-15T12:00:00Z",
  "finished_at": "2025-01-15T12:04:12Z",
  "suites": [{ "id": "journey", "status": "pass" }],
  "scenarios": [
    {
      "id": "SCEN-JOURNEY-04",
      "status": "pass",
      "success_criteria": ["SC-JOURNEY-04", "SC-CONN-02", "SC-MSG-01"],
      "requirements": ["REQ-TRANS-007", "REQ-TRANS-066", "REQ-MSG-039"],
      "tests": ["TEST-PROTO-012"],
      "evidence": ["runs/2025-01-15-A1/artifacts/receipts/msg-001.json"]
    }
  ],
  "failures": []
}
```

`events.ndjson` (one JSON object per line):

```json
{"ts":"2025-01-15T12:00:01Z","run_id":"2025-01-15-A1","scenario_id":"SCEN-JOURNEY-04","step_id":"S5","action":"node.send_message","actor":"alice","status":"pass","evidence":["runs/2025-01-15-A1/artifacts/receipts/msg-001.json"]}
```

`state_snapshot.json` (per node; minimum fields):

```json
{
  "node_id": "alice",
  "iid": "b1anasr5h0bj3832xqexwy0f0987e1xb",
  "identity_seq": 3,
  "contacts_count": 4,
  "apps_installed": ["example.postapp"],
  "state_hash": "sha256:..."
}
```

### `summary.md` (Human-Readable)

MUST include: [REQ-OVR-008]

- Pass/fail by suite and scenario.
- Links to evidence artifacts.
- The exact command used to run the harness.
- Total run time and flake count (if any).

## Flake Policy

- A scenario is flaky if it fails and re-run passes without code changes.
- Flaky tests are treated as failures for release gates.
- The harness must support `--rerun-failed` but still record the initial failure.

## Release and Completeness Gates

The project is "complete" (code + intended behavior) when:

- All success criteria are mapped to scenarios/tests and green.
- All spec completeness criteria are satisfied per component (no TBDs).
- Conformance suite is green (vectors + canonicalization).
- Journey suite is green across required topologies.
- Evidence bundle exists for the release candidate and is reviewable.

## Relationship to Traceability

Every harness scenario and test must be referenced from the traceability matrix in `spec/00-overview/traceability.md`.
