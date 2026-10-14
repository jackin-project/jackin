# C12 — Deterministic tests and verification evidence

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Run deterministic checks independently from the implementer and produce revision-bound, host-authenticated evidence. Implement the verification coordinator in `jackin-workflow` over existing runtime execution services.

## 2. Existing foundation and gap

Existing orchestration research correctly distinguishes evidence from agent narration. The missing workflow contract is an independently executed test bound to a sealed checkpoint, a trusted harness and structured acceptance criteria. Reuse project test/process tools; do not let an agent-created “passed.json” become the authority. [J04](#evidence-and-references), [J19](#evidence-and-references).

## 3. API and data contracts

```text
verify(checkpoint, TestSpec, verifier_grant, effect_id) -> VerificationHandle
collect(verification_handle) -> TestResult
validate_result(checkpoint, harness_policy, result) -> ValidatedEvidence
```

`TestSpec` includes a pinned harness/toolchain identity, executable/argv, cwd, environment/secret references, declared network, timeout, expected exit codes, required suites, minimum discovered/executed counts, skip/flaky policy, result parser/schema, output limits and required artifact types.

`TestResult` includes checkpoint/OIDs, verifier attempt/instance, harness and command digests, start/end times, exit/signal/timeout, counts by passed/failed/skipped/errored, suite outcomes, evidence refs and collector authenticity. `outcome` is `passed`, `failed`, `infrastructure_error`, `cancelled` or `invalid`; no missing field implies pass.

## 4. Required functionality

Create a new verifier environment that receives the exact sealed checkpoint, required reference context and only verification credentials. It cannot mount the implementer’s writable repository, home, vendor conversation or mutable build cache. Shared caches are read-only or separately keyed and validated so an implementer cannot poison verification results.

Use a trusted harness definition selected by host policy and frozen in the plan. Critical acceptance tests must not be replaceable solely by editing tests in the implementation branch. The harness may execute candidate repository tests, but reports changes to test/build configuration and runs independent baseline/hidden or otherwise host-controlled checks as policy requires. A changed test suite, zero executed tests, prohibited skips or missing reports fails the required gate.

The trusted supervisor launches the command and records exit/signal/output. Structured JUnit/JSON emitted by candidate code is evidence to validate, not inherently trusted authority. The supervisor/collector credential is inaccessible to the test process. Parse outputs under size/depth limits; store raw capture and normalized results with digests. Cross-check required suite execution against harness observations where available.

Source input is read-only when the harness permits. Tests requiring writable sources run on a separate exact clone/overlay with declared mutation rules, and the result records the source manifest before execution; they never mutate the sealed checkpoint. Build output goes to separate writable directories. Full process groups and descendants are terminated on deadline.

## 5. Acceptance semantics

A check passes only when its trusted collector, target checkpoint, harness digest, command contract, exit criteria, required suites/counts and artifact integrity all validate. The host gate decides whether the set of checks is sufficient. Test success is evidence of tested properties, not a mathematical guarantee of overall program correctness.

Flaky retry policy must be explicit and bounded. Record every attempt, including initial failures. A “passed after retry” result remains distinguishable and is accepted only if the policy allows it. Infrastructure failure may retry with a fresh verifier; test failure normally triggers bounded repair, not infrastructure retry until green.

## 6. Build instructions

Define result schemas and fake verifier fixtures. Add an independent sandbox command runner with trusted collector, then parsers for the first supported test formats. Implement policy checks for zero tests, skips, changed harness and inconsistent reports. Add checkpoint-bound acceptance and cache-isolation tests before integrating the repair loop.

## 7. Dependencies and integration

C04 executes, C08 supplies checkpoints, C10 records outcomes, C11 retains evidence, C15 evaluates acceptance and C14 routes failures. Command blocks share execution/capture infrastructure but do not automatically satisfy Test contracts.

## 8. Acceptance tests

`TEST-01`: exact checkpoint and trusted harness required. `TEST-02`: implementer cannot forge host collector result. `TEST-03`: zero tests, missing suites and forbidden skips fail. `TEST-04`: changed harness/test configuration is surfaced and independently checked. `TEST-05`: killed/timeout tests are not pass. `TEST-06`: late result cannot approve another checkpoint. `TEST-07`: verifier does not read implementer mutable cache/home. `TEST-08`: retries preserve failures and obey flaky policy.

## Evidence and references

- **J04 — [Existing orchestration design constraints](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/workflow-systems/agent-workflow-orchestration/02-design-constraints.mdx).** Full document. Proposed deterministic ledger, evidence, operator intervention and Capsule ownership.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
- **J19 — [Testing and CI rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/TESTING.md).** Lines 1–200. Pinned mise/mbx/nextest commands, e2e and conditional OrbStack lane, documentation examples and crate gates. Examples here are not evidence of execution.
