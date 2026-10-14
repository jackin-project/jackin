# B05 — Review block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Use a real independent coding-agent session to inspect one immutable revision and produce structured, actionable findings.

## API: configuration and ports

`kind: review`. Configuration: reviewer adapter/role/provider/model, base/checkpoint scope, rubric digest, independent-context policy, output schema, required coverage, severity vocabulary, timeout and bounded format-retry count.

Inputs: `checkpoint: checkpoint/v1`, `context: context-bundle/v1`, optional exact test evidence. Outputs: `review: review-result/v1`, `findings: finding-set/v1`, evidence artifacts and an inspectable `session-ref/v1` when interactive.

## Functionality

Provision a separate instance/private repository and fresh reviewer conversation. Supply pinned base/head, trusted task/rubric context and explicitly labeled untrusted implementation notes only when requested. Collect schema-valid findings, coverage and limitations, with each finding tied to that checkpoint.

A reviewer’s “approved” statement is an advisory recommendation. The gate evaluates severity/coverage and independent tests. Missing or malformed review output is not a finding-free review. A bounded formatting retry does not change the reviewed revision or grant.

## Permissions, errors and limits

No shared mutable implementer filesystem, conversation or publishing credentials. A vendor subagent in the same instance is not independently isolated merely because it has a different name. Stale findings stay historical until explicitly revalidated/remapped. Reviewer manipulation of finding disposition cannot replace host-authorized state.

## Build instructions and acceptance

Use C05/C06 for the actual CLI and C13 for collection/validation. Add original-TUI inspection and exact-revision fixtures. Test IDs: `REV-01` through `REV-07`; include wrong-revision, malformed output, stale findings and same-instance negative cases.

## Implementation ownership

See [C13 — Independent review and structured findings](../components/13-independent-review.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
