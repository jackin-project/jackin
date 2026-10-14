# B07 — Parallel block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Declare a finite concurrency group of independent child nodes. This is scheduler structure, not a manager agent spawning untracked workers.

## API: configuration and ports

`kind: parallel`. Configuration: declared child node IDs, `maxConcurrency`, resource reservation policy, failure policy (`collect_all` or `fail_fast`) and matching Join ID. Inputs are declared common immutable references. Output: `group: parallel-group/v1`, an immutable group identity and selected child set, not a merged result.

The compiler lowers child membership into dependency guards: the group opens before child execution, and each child retains its own inputs/permissions/limits. The Parallel node does not wait on a child that depends on it; the separate Join performs waiting. This avoids an accidental dependency cycle.

## Functionality

Admit children as normal scheduler nodes within group/run/host budgets. Each writable child gets independent instance/repository state by default. Common roles, images or context references are reusable; mutable filesystems are not implicitly shared.

`fail_fast` requests cancellation of still-running siblings after a qualifying failure but continues to reconcile their in-flight effects and preserve work. `collect_all` waits for declared outcomes. Actual dispatch order is recorded; deterministic control does not require serial execution.

## Permissions, errors and limits

The group cannot aggregate child grants into broader rights. Invalid duplicate membership, cyclic nesting, unbounded dynamic child creation and unsupported writable aliasing fail planning. Nested groups are allowed only within finite compiled limits.

## Build instructions and acceptance

Implement as a scheduler grouping/admission primitive, not another runtime. Add concurrency reservation, cancellation and skip fixtures. Test IDs: `SCH-02`, `SCH-03`, `SCH-04`, `SBX-02`. Verify two writers using the same role still receive isolated mutable state.

## Implementation ownership

See [C09 — Scheduler, dependencies and resource budgets](../components/09-scheduler-budgeting.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
