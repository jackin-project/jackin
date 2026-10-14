# B08 — Join block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Synchronize a declared group and combine typed results without implicitly merging mutable code or filesystems.

## API: configuration and ports

`kind: join`. Configuration: group reference, expected child IDs/output ports, join policy (`all_required`, `all_selected`, or an explicitly authorized success threshold), skipped-input handling, cancellation policy for unneeded children and result schema.

Inputs: `group: parallel-group/v1` and declared child outputs/outcomes. Output: `result: join-result/v1`, including each child attempt/outcome/output reference and selected aggregate decision. Output ordering follows declared child order, not wall-clock completion order.

## Functionality

Wait for the policy’s required terminal outcomes and validate all required typed outputs. `all_required` rejects missing/skipped required children; `all_selected` accepts skips only when the preceding Condition explicitly excluded those members. A threshold join may be used for optional strategies/evidence only when it cannot bypass mandatory acceptance checks.

Failures remain visible in the aggregate. A join does not turn cancelled/error children into successful artifacts. Reconciliation must finish or expose uncertainty for effectful children, even if their output is no longer needed.

Joining independently changed code requires a following explicit Git integration block. That operation creates a new checkpoint that must be retested/reviewed. Join never overlays one worker’s files onto another or silently chooses the latest branch.

## Permissions, errors and limits

Host-local pure aggregation, zero implicit network/host file writes. Bound fan-in and aggregate output size. Missing required output, wrong schema, stale attempt or uncertain side effect prevents a successful join unless the frozen policy explicitly models that outcome.

## Build instructions and acceptance

Implement deterministic outcome reduction in the scheduler and schema compiler. Test completion order permutations, failed/skipped children, nested conditions, threshold misuse and code-merge separation. Test IDs: `SCH-01`, `SCH-04`, `GIT-07`, `GATE-08`.

## Implementation ownership

See [C09 — Scheduler, dependencies and resource budgets](../components/09-scheduler-budgeting.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
