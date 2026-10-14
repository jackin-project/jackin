# B09 — Feedback block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Deliver correlated findings or instructions to an existing real agent session, usually as part of a bounded repair scope.

## API: configuration and ports

`kind: feedback`. Configuration: target logical session input, exact checkpoint/ordinal, feedback template, finding/result bindings, `whenIdle` or explicitly certified steering policy, allowed slash/action kind and deadline. A Feedback block never silently falls back to creating a new Agent session.

Inputs: `session: session-ref/v1`, `checkpoint: checkpoint/v1`, `findings: finding-set/v1`, declared test failures/context. Outputs: `submission: submission-receipt/v1` and, when the contract waits for a turn, `turn: agent-turn-result/v1`.

## Functionality

Construct an immutable payload with relevant failures, evidence references, expected action and remaining host budget. Validate checkpoint freshness and target session/turn. Wait for the correct input boundary and scheduler lease, then use the durable Capsule submit contract. Manual takeover pauses dispatch.

Within a RepairScope, iteration advancement and feedback intent commit atomically. Outside a scope, this is one declared finite follow-up operation; it cannot introduce a backward dependency or unbounded loop. Transport acknowledgement, vendor turn completion and successful repair remain separate outcomes.

## Permissions, errors and limits

Only targeted input authority is granted, not permission expansion. Unknown delivery blocks automatic resend. Stale findings require remapping/current verification, not blind paste. Unsupported same-session continuation/steering is explicit failure or a declared operator-required path. A partial input transaction is not retried in full silently.

## Build instructions and acceptance

Implement with C14 aggregation and C06/C07 submission/lease logic. Test original-session continuity, current-turn coordination, stale revision, takeover, bounded iterations and crash ambiguity. Test IDs: `REPAIR-01` through `REPAIR-08`, `CAP-03`.

## Implementation ownership

See [C14 — Same-session feedback and bounded repair](../components/14-feedback-repair.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
