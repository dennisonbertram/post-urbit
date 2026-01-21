# Domain Specification Review Checklist

## Purpose
This checklist ensures consistency across all domain specifications and prevents drift during the planning loop process.

---

## Pre-Work Review Checklist (Before Subagent Starts)

### Scope Clarity
- [ ] Domain scope is clearly defined
- [ ] Boundaries with adjacent domains are explicit
- [ ] Dependencies on other domains are listed
- [ ] No overlap with already-completed domains

### Approach Validation
- [ ] Proposed approach aligns with existing ADRs
- [ ] No contradictions with prior specifications
- [ ] Security implications considered upfront
- [ ] Platform differences (Win/macOS/Linux) anticipated

### Resource Allocation
- [ ] Deliverables are achievable in planning scope
- [ ] Level of detail appropriate (spec, not implementation)
- [ ] Required research identified

---

## Post-Work Review Checklist (After Subagent Completes)

### Completeness
- [ ] All required deliverables addressed
- [ ] Sequence diagrams included for key flows
- [ ] Data models defined with types and constraints
- [ ] Rust interface signatures provided
- [ ] TypeScript interface signatures provided

### Consistency
- [ ] Naming aligns with Protocol Registry
- [ ] No contradictions with other domain specs
- [ ] ADRs created for all key decisions
- [ ] References to other domains are accurate

### Security
- [ ] Security invariants explicitly stated
- [ ] "Must be enforced in Rust" items identified
- [ ] Threat vectors considered
- [ ] No new attack surfaces introduced without mitigation

### Platform Coverage
- [ ] Windows (WebView2) considerations documented
- [ ] macOS (WKWebView) considerations documented
- [ ] Linux (WebKitGTK) considerations documented
- [ ] Platform-specific code paths identified

### Testability
- [ ] Acceptance criteria are testable (pass/fail)
- [ ] Test cases provided
- [ ] Performance targets quantified
- [ ] Security tests defined

---

## System Review Checklist (Every 5 Loops)

### Cross-Document Consistency
- [ ] All specs use consistent naming (Protocol Registry)
- [ ] No contradicting requirements across specs
- [ ] All inter-domain references are valid
- [ ] ADR decisions respected throughout

### Architecture Coherence
- [ ] Security model is coherent end-to-end
- [ ] Data flow is consistent
- [ ] Error handling is uniform
- [ ] No circular dependencies

### Risk Management
- [ ] Risk Register updated with new risks
- [ ] Existing risks re-evaluated
- [ ] Mitigation plans in place
- [ ] No blocking risks unaddressed

### Implementation Readiness
- [ ] Specs are detailed enough to implement
- [ ] No ambiguous requirements
- [ ] Dependencies clearly ordered
- [ ] Vertical slice milestones achievable

---

## Review Outcome Ratings

| Rating | Meaning | Action |
|--------|---------|--------|
| **Pass** | Spec meets all criteria | Proceed to next domain |
| **Pass with Notes** | Minor issues, non-blocking | Document and proceed |
| **Revise** | Significant gaps | Subagent revises spec |
| **Escalate** | Architectural conflict | Full team discussion |
