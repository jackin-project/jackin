# C14 — Same-session feedback and bounded repair

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Turn failed verification into actionable follow-up input to the original implementer session, within a finite host-owned repair scope. The repair controller belongs to `jackin-workflow`; agents cannot create iterations or bypass the next checks.

## 2. Existing foundation and gap

Capsule has targeted input, and existing research describes fixing/review states. The missing contract is durable iteration ownership, revision-correlated aggregation, safe same-session continuation and explicit exhaustion. [J08](#evidence-and-references), [J04](#evidence-and-references).

## 3. API and data contracts

```text
begin_scope(run, RepairScope) -> ScopeState
record_iteration_evidence(scope, checkpoint, results) -> IterationAssessment
prepare_feedback(scope, assessment, current_session) -> FeedbackEnvelope
dispatch_feedback(envelope, expected_lease, expected_turn) -> CommandId
finish_scope(scope, accepted_checkpoint_or_failure) -> ScopeResult
```

`RepairScope` declares initial Agent node, checkpoint template, test/review templates, acceptance criteria, Feedback template, `maxRepairs`, infrastructure/formatting retry policies, per-iteration and overall deadlines, and exhaustion action (`fail` or `request_operator`). `FeedbackEnvelope` includes exact target session, checkpoint/ordinal, finding/test IDs, normalized failures, evidence references, attempt number, remaining budget and explicit requested action. Its content is an immutable artifact with a digest.

## 4. Precise loop lowering

The main workflow graph is acyclic. Compile a repair scope into a finite expansion: `Agent0 → Checkpoint0 → Tests0/Reviews0 → Assess0`, followed only on repairable failure by `Feedback1 → Checkpoint1 → Tests1/Reviews1 → Assess1`, and so on through `maxRepairs`. All Feedback nodes target the logical session created by Agent0. There is no ordinary backward dependency.

Each expansion address includes its iteration. An accepted assessment emits the scope’s selected accepted checkpoint and marks unused later iterations skipped. Exhaustion emits a failure/operator-required result. Only the scope output connects to downstream final approval/publication. Raw internal outputs cannot bypass its assessment. The compiler can materialize this finite expansion or store a bounded template with equivalent verified semantics; the immutable plan records which representation/version is used.

`maxRepairs = 2` means one initial implementation plus at most two repair turns/verification cycles. Infrastructure retries do not silently consume or reset that semantic counter; their own bounded counter is separate. Every initiated repair increments the durable counter before feedback dispatch so a daemon crash cannot obtain an extra iteration.

## 5. Required functionality

Aggregate verification failures for one checkpoint into one actionable envelope. Include exact errors, affected files where valid, expected behavior and evidence references; do not flood the session with one uncontrolled prompt per reviewer. Store raw findings separately from a bounded normalized summary. The payload tells the agent what failed, never grants authority to skip checks.

Default feedback waits for the current turn’s safe input boundary and an available scheduler lease. Use active-turn steering only if explicitly planned and certified, with an expected vendor turn ID. An operator lease pauses automated feedback. Unknown input delivery triggers C17 reconciliation and does not allocate a second prompt automatically.

Before dispatch, compare feedback target ordinal with the current candidate/session observation. If newer work exists, retain historical findings but first verify/remap against the current candidate or explicitly obtain an operator decision. Do not submit obsolete line-level findings as if they describe the latest revision. The next checkpoint always runs the required tests/reviews again according to the frozen policy.

A healthy implementer session remains alive throughout repair; do not restart for convenience. If the process died, a certified vendor resume may bind a new physical session to the existing logical one with a recovery event. If resume is unsupported and the plan requires same-session continuation, request operator action or fail; do not invent continuity. An explicitly planned replacement-session fallback records that discontinuity and its fresh context, and does not claim uninterrupted session identity.

## 6. Build instructions

Implement finite scope expansion and reducer tests first. Add evidence aggregation and stale-target validation, then immutable feedback artifacts. Route dispatch exclusively through C06/C07 and persist iteration advancement atomically with the effect intent. Add exhaustion/operator-required flows and retain all failed checkpoints. Only then enable real adapter tests.

## 7. Dependencies and integration

Requires C08 checkpoints, C09 scheduling, C10 ledger, C12/C13 evidence, C15 gate assessment and C17 recovery. The Feedback block is a reusable primitive but cannot create an uncontrolled graph loop by itself.

## 8. Acceptance tests

`REPAIR-01`: failed review reaches original healthy session without restarting. `REPAIR-02`: repair count stays bounded across crashes/retries. `REPAIR-03`: busy turn and operator takeover prevent conflicting prompts. `REPAIR-04`: old findings are not blindly applied to a new candidate. `REPAIR-05`: every repaired checkpoint reruns required checks. `REPAIR-06`: exhaustion is explicit failure/operator state. `REPAIR-07`: unknown delivery creates no silent duplicate feedback. `REPAIR-08`: downstream publication cannot consume an unassessed internal checkpoint.

## Evidence and references

- **J04 — [Existing orchestration design constraints](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/workflow-systems/agent-workflow-orchestration/02-design-constraints.mdx).** Full document. Proposed deterministic ledger, evidence, operator intervention and Capsule ownership.
- **J08 — [Capsule control protocol](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/control.rs).** Lines 1–280. SessionSend writes literal bytes; SessionSent acknowledges PTY writer delivery; Events is a subscription. Not an exactly-once prompt protocol.
