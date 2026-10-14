# C13 — Independent review and structured findings

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Run an independent real coding-agent reviewer against an exact immutable revision and normalize actionable findings. Coordinate review in `jackin-workflow`, using the same certified agent and Capsule infrastructure as implementation without sharing the implementer’s mutable state.

## 2. Existing foundation and gap

Existing research proposes independent sessions and findings tied to Git refs. Existing agent/runtime primitives can launch the reviewer, but do not by themselves enforce independence, coverage or finding validity. [J04](#evidence-and-references), [J05](#evidence-and-references), [J17](#evidence-and-references).

## 3. API and data contracts

```text
start_review(checkpoint, ReviewSpec, context_bundle, grant) -> ReviewHandle
collect_review(handle) -> ReviewResult
validate_review(result, checkpoint, output_schema) -> FindingSet
update_disposition(finding_id, evidence, authorized_actor) -> DispositionRecord
```

`ReviewSpec` includes certified agent/provider/model, role, independent-context requirement, base/checkpoint targets, scope paths, rubric digest, output schema, coverage expectations, severity threshold, deadline and formatting-retry policy. `ReviewResult` records reviewer session/attempt, checkpoint/base/head OIDs, rubric/context digests, inspected scope, findings and evidence refs. `recommendation` is advisory; the reviewer cannot issue an acceptance certificate.

## 4. Required functionality

Allocate a separate instance/private repository and new reviewer logical/vendor session. Do not ask a subagent inside the implementer’s shared Capsule instance and count it as independently isolated. A different model is optional policy; a separate context and mutable-state boundary is mandatory. A plan can request multiple independent reviewers, each with its own provenance.

Provide the exact base/head checkpoint, task requirements and approved relevant context. Do not automatically copy the implementer’s conversation or self-assessment into the reviewer’s evidence. Any supplied implementer notes are explicitly marked untrusted claims. The reviewer may read a pinned diff and tests/evidence without receiving publishing credentials or write access to the implementer.

Validate output against the finding schema. Every finding binds one checkpoint, severity, category, actionable summary and evidence; source locations must exist in that revision or explicitly describe a non-file issue. Record the review’s coverage and limitations. Empty findings from a valid complete review differs from an invalid/missing review response.

For malformed structured output, allow only the bounded formatting retry declared in the plan, preferably in the same reviewer session. Formatting retry does not change the target or grant. Exhaustion produces `review_invalid`, not “no findings.” Contradictory reviewers are presented as separate evidence; the gate applies an explicit policy or requests operator judgment.

## 5. Stale reviews and finding lifecycle

A review can finish after implementation has moved on. Store it under its original checkpoint and mark its relationship to the current candidate. Do not blindly transplant line numbers or a blocking decision to new code. To use it for repair of a newer checkpoint, either review/remap against that checkpoint with explicit evidence, or treat it as an advisory historical note and run current verification.

An implementer may claim a finding is fixed, but closure requires independent evidence bound to the new candidate. Dismissal/waiver requires an authorized reviewer/operator action with a reason, not an edit to the finding JSON. The original finding is immutable; disposition transitions are append-only.

## 6. Build instructions

Create review/rubric/finding schemas and fixtures. Provision isolated reviewer sessions through C04/C05/C06. Implement a structured-output collector and validator. Add checkpoint/coverage checks, formatting retry and stale-finding handling. Connect the normalized finding set to C14 and C15 only after provenance tests pass.

## 7. Dependencies and integration

Uses C05 real agent adapters, C08 exact repositories, C11 references and evidence, C12 test results, C14 feedback and C15 gates. Reviewer terminal sessions remain observable/attachable through the same lease system.

## 8. Acceptance tests

`REV-01`: same implementer session/instance cannot satisfy independence. `REV-02`: correct immutable base/head is mounted. `REV-03`: invalid output is not zero findings. `REV-04`: stale findings never satisfy/currently fail the wrong candidate without remapping. `REV-05`: implementer claims cannot close findings unilaterally. `REV-06`: multiple reviewer disagreement remains visible and policy-driven. `REV-07`: reviewer TUI remains the original Capsule CLI.

## Evidence and references

- **J04 — [Existing orchestration design constraints](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/workflow-systems/agent-workflow-orchestration/02-design-constraints.mdx).** Full document. Proposed deterministic ledger, evidence, operator intervention and Capsule ownership.
- **J05 — [Runtime public integration surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime/src/lib.rs).** Full file. Existing launch, inspect, spawn, attach and cleanup re-exports; reactive daemon feature boundary.
- **J17 — [Existing sealed agent runtime trait](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/runtime.rs).** Lines 1–260, including the complete trait. Installation, auth and state behavior, not workflow-turn control. Built-in adapter count in comments is stale; no count inferred from it.
