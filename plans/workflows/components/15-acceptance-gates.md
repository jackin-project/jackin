# C15 — Acceptance and operator approval gates

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Determine whether exact evidence satisfies the frozen acceptance policy, and obtain explicit operator decisions where required. Only the host gate engine can issue an acceptance certificate. A gate is not an agent prompt asking whether the work looks finished.

## 2. Existing foundation and gap

Existing research correctly assigns transition authority to evidence and operator decisions. The new work is the executable, typed, latest-revision gate contract and race-safe certificate. [J04](#evidence-and-references).

## 3. API and data contracts

```text
assess(candidate, evidence_set, AcceptancePolicy) -> GateAssessment
request_approval(target, action, evidence_digest, ttl) -> ApprovalRequest
approve(request_id, actor, expected_run_version, expected_candidate, reason) -> Decision
reject(request_id, actor, expected_version, reason) -> Decision
issue_acceptance(assessment, decisions, finalization_record, expected_candidate) -> AcceptanceCertificate
validate_certificate(certificate, current_candidate, current_policy, action) -> Permit
```

`GateAssessment` is `satisfied`, `repairable_failure`, `hard_failure` or `operator_required`, with exact unmet criteria and evidence refs. `AcceptancePolicy` enumerates required tests/reviews, severity thresholds, coverage/skip/flaky rules, prohibited changed paths, mandatory operator decisions and permitted waivers. `ApprovalRequest` binds plan, checkpoint/OIDs, evidence-set digest, action, expected candidate ordinal, expiry and intended principal/role.

## 4. Required functionality

Resolve all required evidence for the latest candidate. Verify checkpoint identity, trusted producer/collector, harness/rubric policy digests, artifact integrity and result completeness. A missing/invalid/cancelled result fails its requirement. A condition/join cannot make a mandatory gate disappear. Acceptance of a node’s output differs from whole implementation acceptance.

Issue the acceptance certificate in a single CAS transaction against the candidate ordinal, run state and decision versions. A new checkpoint or operator code edit invalidates old eligibility; it does not erase historical results. The certificate records every required result and waiver used. A host-side MAC/signature or equally strong protected-record authenticity prevents worker-created certificates being admitted.

Operator approval is scoped to one action and exact revision. A checkpoint-backup grant does not authorize final PR publication; a PR approval does not authorize a merge; a review waiver does not grant network access. Recheck current host authorization at the moment of action. Expired or stale approvals require a new decision, even when their UI button remains visible.

An operator may waive only policy-defined soft criteria, with reason and expiry. The result is `accepted_with_waiver`, not “all checks passed.” Hard isolation failure, corrupted evidence, unknown revision and denied host publication rights cannot be waived within the run. An agent cannot approve itself by writing a decision artifact or taking over its own session token.

## 5. Race and timeout handling

Approval requests use expected state versions. Concurrent approve/reject yields one committed decision; the loser gets `STATE_CONFLICT` and refreshes. An evidence-set change during approval invalidates the request. A policy revocation before publication stops dispatch even after earlier approval. An already dispatched external effect is reconciled and reported rather than pretending revocation reversed it.

An approval timeout uses the plan’s explicit action: fail, remain awaiting operator until the absolute run deadline, or cancel. There is no timeout-to-approve default. Gate evaluation is pure and bounded; operator prose is retained privately, not evaluated as executable policy.

## 6. Build instructions

Implement a pure acceptance evaluator with fixtures for missing, stale, invalid and waived results. Add canonical evidence-set hashing and certificate issuance through C10 CAS. Add approval requests and actor authorization. Integrate final pre-publication certificate validation with C16. Expose the same assessment and reason codes to every UI.

## 7. Dependencies and integration

C03 authorizes actors/actions; C08 identifies candidates; C12/C13 produce evidence; C14 routes repairable failures; C10 persists decisions; C16 consumes certificates. Gate schemas live in C01/domain contracts, not UI code.

## 8. Acceptance tests

`GATE-01`: agent self-report cannot accept implementation. `GATE-02`: old checkpoint/evidence cannot satisfy latest candidate. `GATE-03`: approval raced by a new checkpoint fails CAS. `GATE-04`: hard denials cannot be waived. `GATE-05`: approved-with-waiver remains distinguishable. `GATE-06`: expired/incorrect actor/action approval fails. `GATE-07`: no implicit timeout approval. `GATE-08`: mandatory gates cannot be bypassed by skipped graph branches.

## Evidence and references

- **J04 — [Existing orchestration design constraints](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/workflow-systems/agent-workflow-orchestration/02-design-constraints.mdx).** Full document. Proposed deterministic ledger, evidence, operator intervention and Capsule ownership.

## Finalization is required before final acceptance

A passing latest *stored* checkpoint is insufficient if newer unsealed source changes exist. Ask C08/C04 to close the writable phase, obtain a finalization barrier and verify that the current source manifest matches the candidate. New changes create a new candidate and require the relevant checks again. Bind the acceptance certificate to the finalization record and writer-fence generation, and keep that boundary effective through publication/reconciliation. Reopening mutation invalidates eligibility before writes resume.

Add `GATE-09`: unsealed changes cannot be ignored during acceptance. Add `GATE-10`: reopening mutation atomically invalidates finalization and pending approval before releasing the writer fence. These supplement latest-checkpoint CAS tests rather than replacing them.
