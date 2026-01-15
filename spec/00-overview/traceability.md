# Traceability Framework

## Purpose

Ensure every requirement and success criterion is explicitly linked to verification evidence. This is how we prove "complete" means both code complete and intended behavior complete.

Traceability provides:

- A map from spec requirements -> tests -> evidence
- A reliable definition of "done"
- Fast impact analysis when specs or implementations change

## Objects and IDs

### 1. Success Criteria (SC)

Source: `spec/00-overview/success-criteria.md`

Format:

- `SC-NODE-01`, `SC-ID-02`, `SC-CONN-01`, `SC-MSG-02`, `SC-APP-01`, `SC-SYNC-01`, `SC-OPS-01`, `SC-JOURNEY-01`, etc.

Each bullet in the success criteria doc MUST have a stable ID.

### 2. Requirements (REQ)

Source: component specs and RFCs.

Format:

- `REQ-TRANS-PEER-HANDSHAKE-07`
- `REQ-IDOC-SIGNATURE-03`
- `REQ-MSG-PUSE-12`
- `REQ-APP-MANIFEST-05`

Each normative statement ("MUST/SHOULD") MUST have a stable ID. If a requirement is implied by a state machine or data schema, it still needs an ID.

### 3. Scenarios (SCEN)

Source: verification harness scenarios.

Format:

- `SCEN-JOURNEY-01`, `SCEN-CONF-07`, `SCEN-FAIL-03`

### 4. Tests (TEST)

Source: harness tests (atomic checks, conformance vectors, invariants).

Format:

- `TEST-VEC-001`, `TEST-PROTO-012`, `TEST-STATE-004`

### 5. Evidence (EVID)

Source: harness run artifacts.

Format:

- `EVID-RUN-2025-01-15-A1/summary.md`
- `EVID-RUN-2025-01-15-A1/artifacts/receipts/...`

## Traceability Matrix

The canonical matrix is a table (or machine-readable YAML/CSV) with at least:

| Requirement ID | Description | Spec Source | Scenario IDs | Test IDs | Evidence Path | Status | Notes |
|---------------|-------------|-------------|--------------|----------|---------------|--------|-------|

### Status Values

- `tested`: Evidence exists and passed
- `failed`: Evidence exists and failed
- `analyzed`: Verified by static analysis or review only
- `manual`: Requires explicit human check
- `blocked`: Not testable yet due to missing component
- `n/a`: Not applicable in the current scope

## Coverage Rules

1. Every SC-* MUST map to at least one SCEN-* and one TEST-*.
2. Every REQ-* MUST map to at least one TEST-* (or be explicitly `manual` with a reason).
3. Every SCEN-* MUST produce at least one evidence artifact.
4. Every TEST-* MUST map back to one or more SC-* or REQ-*.

## Completeness Gate (Definition of Done)

The system is "complete" when:

- 100% of SC-* are `tested` and green.
- 100% of REQ-* are `tested` or explicitly `manual` with a written rationale.
- 0 `blocked` items remain in MVP scope.
- Every harness suite produces a full evidence bundle for the release candidate.

## Change Management

When spec changes:

- Add or update REQ-* IDs in the modified spec sections.
- Update traceability matrix entries to include new or changed requirements.
- Add or update tests/scenarios accordingly.

When implementation changes:

- Run impacted scenarios/tests.
- Regenerate evidence bundles and update `Evidence Path`.

## Example Entry

| Requirement ID | Description | Spec Source | Scenario IDs | Test IDs | Evidence Path | Status | Notes |
|---------------|-------------|-------------|--------------|----------|---------------|--------|-------|
| SC-MSG-01 | E2E encrypted 1:1 | spec/00-overview/success-criteria.md | SCEN-JOURNEY-04 | TEST-PROTO-012 | runs/2025-01-15-A1/summary.md | tested | Relay path validated |

## Relationship to Harness

All scenario and test IDs referenced here must exist in the harness definition in `spec/00-overview/verification-harness.md`. The harness is the producer of evidence; this document is the index.

