# B06 — Condition block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Select a declared path using structured evidence, without invoking an agent or arbitrary code.

## API: configuration and ports

`kind: condition`. Configuration: typed expression AST, input bindings, named branch selectors and explicit missing-input policy. Allowed expressions include typed literals/references, comparisons, boolean composition and existence tests; no evaluation of source-code strings.

Inputs: schema-validated JSON/results/decisions from completed nodes. Output: `decision: decision/v1`, containing expression digest, relevant input digests, selected branch IDs and rationale codes. Both branches and the nodes they guard are declared in the immutable plan.

## Functionality

Evaluate the pure expression deterministically against recorded inputs. Mark non-selected branch nodes skipped, with `NotProduced` outputs. Downstream joins explicitly handle selected versus required branches. Unknown, missing or malformed evidence causes the configured error/operator result; it is not truthy/falsy coercion.

The compiler proves that every successful workflow path still satisfies mandatory acceptance gates. A condition may choose an implementation strategy; it cannot choose “skip verification and publish” unless those checks were genuinely optional in the authorized policy.

## Permissions, errors and limits

Host-local CPU-only operation with a bounded expression depth/size and zero network, filesystem or secret access. Reject cycles, undeclared output paths and type coercion. The agent cannot introduce new expressions by returning text that happens to look executable.

## Build instructions and acceptance

Implement the small typed evaluator and compiler guard analysis in shared domain/scheduler code. Do not use JavaScript/Python/Pkl re-evaluation over live outputs for decisions. Test deterministic branch selection, absent values, invalid types, nested conditions and mandatory-gate bypass attempts. Test IDs: `PLAN-07`, `SCH-04`, `GATE-08`.

## Implementation ownership

See [C09 — Scheduler, dependencies and resource budgets](../components/09-scheduler-budgeting.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
