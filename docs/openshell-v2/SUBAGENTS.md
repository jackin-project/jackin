# Subagent execution rules

## Coordinator

The coordinator executes the plan. It does not change the product architecture.
Keep one task board and one write-lease table.
Use up to eight concurrent subagents. Keep capacity for an independent review subagent.
Start all ready tasks that have separate write scopes.
Do not run tasks in parallel only to increase the subagent count.

Use one active writer for each file.
Give source-review subagents read-only access.
Give implementation subagents only their assigned worktree and paths.
Attach the matching file from [tasks/](./tasks) to each task packet.
Do not give a subagent account secrets through its task prompt.
Use approved credential references for authorized runtime tests.

Record each actual subagent identifier and model setting.
A script, subprocess, or sequential review is not a subagent.
Record serial execution when the runner has no subagent tool.

## Subagent assignments

| Subagent | Task ownership |
| --- | --- |
| Planning review subagent | Contract conflicts, P03, upstream contract reviews, and release scope. |
| Integration subagent | Root Cargo files, lockfile, shared schemas, CI registration, and merges. |
| Source audit subagent | P01 and the final removal audit. |
| Upstream API subagent | P02, U00, U01, and OpenShell transport contracts. |
| Upstream worker subagent | U02 and U03. |
| Companion lifecycle subagent | U04 and U05. |
| Companion data subagent | U06. |
| Companion network subagent | U07. |
| State subagent | T10 and T11. |
| Runtime adapter subagent | T12 and T13. |
| Role-image subagent | T14, T15, and T16. |
| Capsule subagent | T17, T18, and T19. |
| Project subagent | T20, T27, and T28. |
| Account subagent | T21, T22, and T25. |
| Product subagent | T23, T24, T26, T29, and T30. |
| Operations subagent | T31 and T34. |
| Verifier subagent | T32. |
| Companion adapter subagent | T33. |
| Qualification subagent | P04, U08, and T35 through T38. |
| Independent review subagent | Task review, negative tests, evidence checks, and T40. |

A subagent receives a work assignment. It is not a permanently active agent.
One person or model can perform several nonconflicting assignments.
The implementation subagent must not approve the same task's independent review.

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
A different source audit subagent checks missing callers and hidden subprocess routes.

## Task packet

Give every implementation subagent this packet.
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

Do not ask the subagent to discover the overall architecture again.
The subagent can inspect source and repair code inside its assigned contract.
A missing contract requires a blocker report, not an improvised replacement.

## Write and merge procedure

1. Record the task's input commit.
2. Create an isolated worktree for that task.
3. Record the granted write paths.
4. Check that no active task owns an overlapping path.
5. Run the task's tests in that worktree.
6. Give the diff and evidence to a different review subagent.
7. Resolve the review findings in the same task.
8. Let the integration subagent merge the reviewed result.
9. Run the affected tests again on the merged commit.
10. Mark the task complete only after the merged tests pass.

Do not discard pre-existing changes to create a worktree.
Do not force-reset another subagent's branch.
Do not use a clean-room result as proof for an untested merged result.

## Shared-file policy

The integration subagent alone changes these shared files:

- Root [Cargo.toml](../../Cargo.toml), [Cargo.lock](../../Cargo.lock), and Rust toolchain files.
- Shared domain schemas and the session wire schema after P03.
- Root CI definitions and generated CI task manifests.
- `compatibility.lock.toml`, task status records, and final acceptance summaries.

Subagents submit a patch request for these files.
The request includes the exact change, dependency reason, and tests.
The integration subagent applies requests in a recorded order.

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

Record each acceptance result in `assignment_results` under its `task_id`.
Run a shared case for each task that owns it.
Never use another task's passing result to close your assignment.
The case-level `status` is a derived summary, not task evidence.

## Review packet

Give the review subagent the original task card, the input commit SHA, and the candidate task diff.
Bind the candidate task diff to both the input commit SHA and candidate commit SHA.
Give the review subagent the implementation report and raw test results.
Ask the independent reviewer to check each acceptance assertion, hidden execution path, and negative case.
Resolve review findings before the integration merge.
Run merged-result checks before task completion.
Ask the review subagent to check that tests did not become weaker.

Require these review results:

| Field | Required content |
| --- | --- |
| Decision | `APPROVE`, `CHANGES_REQUIRED`, or `BLOCKED`. |
| Contract checks | One result for every task requirement. |
| Findings | File, symbol, severity, failed assertion, and required correction. |
| Executed commands | Commands that the review subagent actually ran. |
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
