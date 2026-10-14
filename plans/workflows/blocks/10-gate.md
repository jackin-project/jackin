# B10 — Gate block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Require structured acceptance criteria or an explicit operator decision before a workflow can advance. Gates run under host authority, never under an agent’s self-assessment.

## API: configuration and ports

`kind: gate`. Configuration: `gateType` (`evidence`, `operator`, or `combined`), exact candidate binding, required tests/reviews, thresholds, permitted soft waivers, operator action scope, expiry and timeout outcome.

Inputs: candidate checkpoint, test/review results, finding dispositions and optional operator decision. Outputs: `assessment: decision/v1` and, only for final acceptance gates that satisfy all criteria, `acceptance: acceptance-certificate/v1`. An operator approval alone does not fabricate missing test evidence.

## Functionality

Run the pure gate evaluator, name unmet criteria and either satisfy, trigger repair, fail or wait for an authorized operator. Bind approvals to exact plan/checkpoint/evidence/action. Issue acceptance through a CAS on the latest candidate and current decision versions.

No evidence returned by an implementation agent can act as an operator decision. A gate cannot approve a hard security failure. A waiver produces an explicitly waived result, not a normal all-pass certificate. New candidate code invalidates prior acceptance eligibility.

## Permissions, errors and limits

Host-local evaluation; operator identity and action permit verified through C03. Timeout never means approval. Stale version, expired approval, wrong actor, invalid evidence and unreachable required gates fail closed. Limit expression/criterion size and retained reason text.

## Build instructions and acceptance

Use C15’s shared evaluator and decision records. Add graph reachability checks and UI/API exact-target confirmation. Test IDs: `GATE-01` through `GATE-08`. Include the race where a new checkpoint arrives while the operator presses Approve.

## Implementation ownership

See [C15 — Acceptance and operator approval gates](../components/15-acceptance-gates.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.

## Final-candidate guard

Final acceptance requires an active finalization record proving that the tested/reviewed candidate matches current source under a writer fence. Unsealed newer edits create a new candidate; the gate cannot ignore them. Test `GATE-09` and `GATE-10` in addition to the earlier gate suite.
