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

The harness MUST cover:

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

The harness MUST support the following minimal topologies:

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

Each journey MUST have:

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

### `summary.md` (Human-Readable)

MUST include:

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

