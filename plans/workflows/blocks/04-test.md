# B04 — Test block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Produce independent deterministic verification evidence for one immutable checkpoint. This block has a stronger trusted-harness contract than Command.

## API: configuration and ports

`kind: test`. Configuration: exact `harnessRef`, command/argv, toolchain/role identity, test suites, result format/schema, minimum executed counts, allowed skips, flaky policy, deadline, independent sandbox profile and required artifacts.

Inputs: `checkpoint: checkpoint/v1`, approved `context: context-bundle/v1`. Output: `result: test-result/v1` plus evidence artifacts. Required metadata binds checkpoint/base/head/tree, verifier attempt, harness/command digests and trusted collector identity.

## Functionality

Materialize a new exact consumer of the sealed checkpoint, with no implementer home/session/writable cache. Execute the trusted harness and record process facts independently. Validate suite presence/counts, declared skip/retry policy and report consistency. Candidate-generated result files are untrusted data; the collector and acceptance gate validate them.

Failure is a valid TestResult that downstream repair/gate logic can inspect, not an infrastructure exception that automatically retries until green. Infrastructure errors have their own bounded policy. The block records every flaky attempt and never overwrites its initial failure.

## Permissions, errors and limits

Use a verification-specific grant with no publisher credentials. A writable test overlay cannot change the sealed target. Timeout, killed process, zero tests, missing result, altered trusted harness and corrupt evidence cannot yield pass. Review/test results for a different revision are rejected.

## Build instructions and acceptance

Build on C12’s collector and schemas. Test trusted harness separation, exact revision, forged report rejection, zero/skip behavior, stale results and descendant cancellation. Test IDs: `TEST-01` through `TEST-08`. The workflow gate, not this worker, decides final implementation acceptance.

## Implementation ownership

See [C12 — Deterministic tests and verification evidence](../components/12-tests-evidence.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
