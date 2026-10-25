# Subagent execution rules

## Coordinator

The coordinator executes the plan. It does not change the product architecture.
Keep one task board and one write-lease table.
Use up to eight concurrent agents. Keep capacity for an independent reviewer.
Start all ready tasks that have separate write scopes.
Do not run tasks in parallel only to increase the agent count.

Use one active writer for each file.
Give source-review agents read-only access.
Give implementation agents only their assigned worktree and paths.
Attach the matching file from `tasks/` to each task packet.
Do not give an agent account secrets through its task prompt.
Use approved credential references for authorized runtime tests.

Record each actual agent identifier and model setting.
A script, subprocess, or sequential review is not a subagent.
Record serial execution when the runner has no subagent tool.

## Roles

| Role | Task ownership |
| --- | --- |
| Planner reviewer | Contract conflicts, P03, upstream contract reviews, and release scope. |
| Integration owner | Root Cargo files, lockfile, shared schemas, CI registration, and merges. |
| Source auditor | P01 and the final removal audit. |
| Upstream API engineer | P02, U00, U01, and OpenShell transport contracts. |
| Upstream worker engineer | U02 and U03. |
| Companion lifecycle engineer | U04 and U05. |
| Companion data engineer | U06. |
| Companion network engineer | U07. |
| State engineer | T10 and T11. |
| Runtime adapter engineer | T12 and T13. |
| Role engineer | T14, T15, and T16. |
| Capsule engineer | T17, T18, and T19. |
| Project engineer | T20, T27, and T28. |
| Account engineer | T21, T22, and T25. |
| Product engineer | T23, T24, T26, T29, and T30. |
| Operations engineer | T31 and T34. |
| Verifier engineer | T32. |
| Companion adapter engineer | T33. |
| Qualification engineer | U08 and T35 through T38. |
| Independent reviewer | Task review, negative tests, evidence checks, and T40. |

A role is a work assignment, not a permanently active agent.
One person or model can perform several nonconflicting assignments.
The implementation author must not approve the same task's independent review.

## Initial parallel source review

P01 uses seven read-only assignments.

| Child ID | Read scope | Required report |
| --- | --- | --- |
| P01-A | Runtime dispatch, Docker, Apple, discovery, and cleanup. | `runtime-calls.json` |
| P01-B | Capsule, PTYs, protocol, root setup, and host actions. | `capsule-boundaries.json` |
| P01-C | Roles, image recipes, version probes, caches, and builders. | `image-calls.json` |
| P01-D | Accounts, providers, native login, usage, and credential publication. | `account-routes.json` |
| P01-E | Projects, worktrees, mounts, snapshots, retention, and workflow workers. | `data-and-workers.json` |
| P01-F | CLI, Console, configuration, packaging, scripts, and generated CI. | `product-and-build.json` |
| P01-G | Tests, fixtures, current assertions, and historical regression cases. | `test-coverage.json` |

Require actual file and symbol references in each report.
Do not accept a directory-only inventory as a call-site inventory.
The coordinator merges these reports into the P01 inventory.
A different source auditor checks missing callers and hidden subprocess routes.

## Task packet

Give every implementation agent this packet.
Replace every bracketed value before dispatch.
Do not send a task with an unresolved write scope.

```text
Task: [task ID and title]
Role: [assigned role]
Base commit: [actual Jackin commit]
Upstream tuple: [qualified tuple ID or explicit pending dependency]
Read first: [task card, contracts, source files, and test cases]
Dependencies: [completed task IDs and evidence paths]
Write scope: [exact files or exclusive directories]
Do not edit: [root files, shared files, and other task scopes]
Required result: [task outputs]
Steps: [complete numbered task steps]
Required tests: [case IDs and exact local test commands]
Stop conditions: [task blockers]
Report path: [evidence/task-ID/implementation.json]
```

Do not ask the worker to discover the overall architecture again.
The worker can inspect source and repair code inside its assigned contract.
A missing contract requires a blocker report, not an improvised replacement.

## Write and merge procedure

1. Record the task's input commit.
2. Create an isolated worktree for that task.
3. Record the granted write paths.
4. Check that no active task owns an overlapping path.
5. Run the task's tests in that worktree.
6. Give the diff and evidence to a different reviewer.
7. Resolve the review findings in the same task.
8. Let the integration owner merge the reviewed result.
9. Run the affected tests again on the merged commit.
10. Mark the task complete only after the merged tests pass.

Do not discard pre-existing changes to create a worktree.
Do not force-reset another agent's branch.
Do not use a clean-room result as proof for an untested merged result.

## Shared-file policy

The integration owner alone changes these shared files:

- Root `Cargo.toml`, `Cargo.lock`, and Rust toolchain files.
- Shared domain schemas and the session wire schema after P03.
- Root CI definitions and generated CI task manifests.
- `compatibility.lock.toml`, task status records, and final acceptance summaries.

Workers submit a patch request for these files.
The request includes the exact change, dependency reason, and tests.
The integration owner applies requests in a recorded order.

## Implementation report

Use this JSON shape. Use actual values, not placeholders.

```json
{
  "task_id": "T10",
  "status": "NOT_STARTED",
  "agent_id": null,
  "model": null,
  "effort": null,
  "base_commit": null,
  "result_commit": null,
  "changed_paths": [],
  "removed_paths": [],
  "source_evidence": [],
  "commands": [],
  "test_cases": [],
  "raw_evidence_paths": [],
  "assumptions": [],
  "blockers": [],
  "requested_shared_changes": []
}
```

Allowed task statuses are `NOT_STARTED`, `READY`, `RUNNING`, `REVIEW`, `PASS`, `FAIL`, and `BLOCKED`.
A coordinator derives `READY` from dependencies and write ownership.
Only reviewed, merged evidence permits `PASS`.

## Review packet

Give the reviewer the original task card and input commit.
Give the reviewer the merged diff, implementation report, and raw test results.
Ask the reviewer to check each acceptance assertion independently.
Ask the reviewer to inspect hidden execution paths and missing negative cases.
Ask the reviewer to check that tests did not become weaker.

Require these review results:

| Field | Required content |
| --- | --- |
| Decision | `APPROVE`, `CHANGES_REQUIRED`, or `BLOCKED`. |
| Contract checks | One result for every task requirement. |
| Findings | File, symbol, severity, failed assertion, and required correction. |
| Executed commands | Commands that the reviewer actually ran. |
| Evidence limits | Unavailable targets, accounts, tools, or source. |
| Regression check | Confirmation that old prohibited paths did not return. |

A code review is not a runtime test.
A report copied from the author is not an independent test.

## Blocker report

Record the failed requirement and its source.
Record the exact API or runtime limitation.
Include the smallest reproducer and sanitized output.
Name the required upstream owner and affected tasks.
Propose one contract-preserving change.
Keep any proposed API clearly separate from an existing API.
Continue tasks that do not depend on the blocked result.
