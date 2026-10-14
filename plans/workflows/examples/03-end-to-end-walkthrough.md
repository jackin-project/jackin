# End-to-end execution, failure and recovery walkthrough

## Scenario

An operator asks an existing real coding agent to implement a Rust change. The plan requires a private exact clone, read-only task references, independent tests and review, at most two repair iterations, original TUI access, final operator approval and a host-created PR. Identifiers and revisions here are fictional. This is an expected-behavior scenario, not a recorded successful run.

## Expected timeline

| Step | Host action and durable evidence | Session / revision consequence |
|---|---|---|
| 1 | Resolve source/imports/role/repository/agent profile; authorize and store plan hash P | Later source edits cannot alter P |
| 2 | Create run R and initial preservation hold; reserve resources | Failure/cancel cannot discard unique future work |
| 3 | Persist provision intent, prepare private repo at base B and adopt/create owned instance I | No shared mutable `.git` with other workers |
| 4 | Create original Capsule session S at physical epoch E; record vendor session V | S/V retained for healthy repair; operator can Observe |
| 5 | Persist initial prompt command C1, deliver once under scheduler lease | PTY receipt and vendor turn events remain distinct |
| 6 | Agent finishes a bounded turn; acquire a writer barrier and seal checkpoint K1 | K1 has immutable head/tree and candidate ordinal 1 |
| 7 | Optional authorized backup pushes K1 to a run-specific checkpoint branch | K1 is backed up, not accepted or merged |
| 8 | Independent verifier T1 and reviewer Q1 consume exact K1 in separate instances | Implementer mutable state cannot influence their input snapshot |
| 9 | T1 fails a cancellation test; Q1 emits a high-severity finding against K1 | Gate assessment is repairable failure, not accepted |
| 10 | Persist repair counter and feedback artifact F1 referencing T1/Q1/K1 | Remaining repair budget becomes 1 after the first repair is initiated |
| 11 | Wait for valid turn/input boundary, then deliver C2/F1 into original S/V | No healthy-session restart; operator takeover would pause delivery |
| 12 | Seal K2 after repair; rerun required tests/review against K2 | K1 evidence remains historical; it cannot approve K2 |
| 13 | Tests/review pass K2; enter finalization barrier and verify no newer/unsealed mutation supersedes it | Writer phase closed/fenced while acceptance and publication proceed |
| 14 | Operator approves exact K2/evidence/action; host CAS issues acceptance certificate A2 | A2 binds ordinal 2, policy/harness/results and decision |
| 15 | Host rechecks authorization and persists publication effect; push exact K2 and create PR with run marker | No worker receives publisher credential; no force/merge |
| 16 | Verify remote receipts and persist publication record | A lost response would be reconciled before retry |
| 17 | Mark required run outputs satisfied; keep/release resources only through verified holds/retention | Original history, checkpoints and unique unfinished state remain protected |

## Host daemon crashes after step 5

The new controller acquires a fresh fencing epoch and reconciles I, Capsule E and command C1. If Capsule and its receipt survive, it adopts the original session without creating another prompt. If delivery is unknown, it stops automatic input and uses the operator/vendor-proof resolution flow. It cannot infer success from a visible terminal or an agent claim that work is done.

## Operator takes control during step 11

The service fences automated input and obtains an acknowledged new lease generation before keyboard ownership is granted. It reports the current active turn. The operator may inspect/interact within the existing grant. On release, the host records intervention and reconciles the repository/session. Manual source edits invalidate prior verification eligibility and require a fresh checkpoint and checks. A disconnected terminal does not silently resume the scheduler.

## Review arrives late

Suppose Q1 finishes only after K2 is sealed. Its finding remains associated with K1. The host does not paste stale line numbers into a current repair as if they applied to K2, and does not let a clean K1 review approve K2. It either verifies/remaps the finding against K2 or presents it as historical advisory evidence while current tests/review decide the current gate.

## Code changes while approval is pending

Final acceptance needs more than “the last stored checkpoint was green.” The host closes/fences the writable phase and confirms the finalization snapshot matches K2. If a new change exists, it seals a new candidate and reruns required checks. If an operator explicitly reopens mutation, prior finalization/acceptance eligibility is revoked before writes are permitted. The original Capsule session remains observable, and control actions show that reopening work will invalidate pending approval.

## PR creation response is lost

The outbox records an unresolved publication effect. Recovery queries the exact repository and unique run/effect marker, validates head/base and adopts a single matching PR. No result or multiple conflicting results requires the configured reconciliation path. The service never blindly creates another PR or uses force push to repair uncertainty.

## Verification never passes

After the configured two repair iterations, the scope enters `failed` or `awaiting_operator` according to the frozen exhaustion policy. It retains every checkpoint, findings and original session evidence. Normal cleanup cannot delete unique unpushed/uncommitted state. An operator can inspect/export or start a new authorized plan; resetting counters, skipping tests or changing Pkl does not retroactively make this run pass.

## Parallel extension

Two implementers may use the same role but get separate instances/private clones at B. Their independent passing results are joined as typed references. A Git integration block combines them under an explicit base/order/strategy. Its output K3 is a new checkpoint; conflicts are explicit, and successful integration still requires independent tests/review of K3. Only K3’s latest valid certificate can authorize the final PR.

## Expected final evidence

A successful evidence bundle contains plan/source/import digests, effective grants, exact runtime certificates, instance/session/epoch identities, submitted prompt commands and receipts, all checkpoint OIDs/seals, trusted test reports, independent findings/dispositions, repair counters, operator input/approval events, finalization record, acceptance certificate, remote publication receipt and preservation/retention manifest. Missing or corrupted required evidence prevents a false-success report.
