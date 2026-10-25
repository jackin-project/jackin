# Planning-pack checks

Date: 2026-10-26.
Result: Static pack validation and per-task assignment fixtures pass.

## Scope

This report validates planning artifacts and validator behavior only.
It does not validate Jackin code, an OpenShell deployment, or the proposed upstream contracts.
No product runtime test or task implementation has been performed.
The validator does not inspect task artifact contents or review decisions.
Tasks without cases still require their task-card artifacts and independent review.
The original specification remains unchanged.

## Reproduce

Run these commands from the repository root:

```sh
python3 docs/openshell-v2/validate_pack.py --initial
python3 docs/openshell-v2/validate_pack.py
python3 docs/openshell-v2/validate_assignment_fixtures.py
```

The first command requires all task statuses to remain `NOT_STARTED` and all case and assignment statuses to remain `NOT_RUN`.
Omit `--initial` after execution starts.
The ordinary validator accepts documented progress states.
It requires one `assignment_results` row for every task owner on every case.
Rows contain exactly `task_id` and `status`; duplicate, missing, and extra owners fail validation.
Each `PASS` task needs `PASS` for its own case assignments.
The case-level `status` remains for existing readers and is derived from all assignments.
It is `PASS` only when all assignments pass.
It is `FAIL` if any assignment fails.
It is `BLOCKED` when no assignment fails and one or more assignments are blocked.
It is `NOT_RUN` otherwise.
The acceptance manifest keeps schema version `1` and adds `assignment_results`.
The upgraded validator requires that field; old manifests without it fail validation.
Markdown assignment labels record initial `NOT_RUN` status and stay fixed during ordinary progress.

`validate_assignment_fixtures.py` runs the validator on temporary pack copies.
Its raw receipts include each actual argument vector, stdout, stderr, exit code, and fixture source hashes.
The receipts are stored in `validation/assignment-results-receipts.json`.

## Counts

| Item | Count |
| --- | ---: |
| Detailed task cards | 46 |
| Acceptance cases | 125 |
| Mapped specification headings | 30 |
| Topological readiness groups | 20 |
| Checked procedural sentences | 1523 |

Readiness groups indicate dependency order, not permission to ignore file ownership.
Keep no more than eight actual subagents active at once.

## Check results

| Check | Result |
| --- | --- |
| Required pack files, including fixture tool and receipts | PASS |
| Manifest schema versions are supported integers | PASS |
| Machine source pins match C01 | PASS |
| Task IDs and case IDs are unique | PASS |
| Declared and expected task and case counts match | PASS |
| Original specification matches its fixed checksum and both manifests | PASS |
| Dependencies reference known tasks | PASS |
| Task and case statuses use documented values | PASS |
| Acceptance evidence levels use documented values | PASS |
| Dependency graph has no cycle and stored groups match | PASS |
| Assignment rows have exact unique owners and valid statuses | PASS |
| Case status matches its assignment summary | PASS |
| Task progress waits for completed dependencies | PASS |
| `PASS` tasks have passing results for their own assignments | PASS |
| Task test references, case coverage, and ownership maps agree | PASS |
| Task graph matches indexes and task cards | PASS |
| Acceptance case blocks match the manifest | PASS |
| Traceability headings, ranges, owners, and representations match | PASS |
| Checked procedural sentences stay within the word limit | PASS |
| Initial states claim no task or case execution (`--initial` only) | PASS |
| Pack records no runtime tests or delegated task reviews (`--initial` only) | PASS |

## Assignment-result causal fixtures

All scenarios passed with the expected validator exit code.
The downstream case below is A22, owned by U02 and T32.

| Temporary-copy fixture | Expected validator result | Observed |
| --- | --- | --- |
| All assignments pass and the shared case summary is `PASS` | Accept, exit 0 | PASS, exit 0 |
| Task `PASS` with its own case assignments `PASS` | Accept, exit 0 | PASS, exit 0 |
| U02 `PASS`, T32 `NOT_RUN`, summary `NOT_RUN` | Accept, exit 0 | PASS, exit 0 |
| U02 `PASS`, T32 `BLOCKED`, summary `BLOCKED` | Accept, exit 0 | PASS, exit 0 |
| Any `FAIL` with a `BLOCKED` assignment, summary `FAIL` | Accept, exit 0 | PASS, exit 0 |
| T32 task `PASS` while T32/A22 remains `NOT_RUN` | Reject, exit 1 | PASS, exit 1 |
| Legacy summary `PASS` while T32/A22 remains `NOT_RUN` | Reject, exit 1 | PASS, exit 1 |
| Legacy manifest without `assignment_results` | Reject, exit 1 | PASS, exit 1 |
| Malformed or invalid-status, duplicate, missing, or extra assignment row | Reject, exit 1 | PASS, exit 1 |
| Assignment status changes under `--initial` | Reject, exit 1 | PASS, exit 1 |
| Malformed assignment row under `--initial` | Reject cleanly, exit 1 | PASS, exit 1 |
| T33 runs before T27 checkpoint task passes | Reject, exit 1 | PASS, exit 1 |
| `BLOCKED` task and its blocked assignments | Accept as blocked, exit 0 | PASS, exit 0 |
| U06 `PASS` with no assigned acceptance cases | Accept under existing no-case completion rule, exit 0 | PASS, exit 0 |

The first shared-case fixture shows an upstream `PASS` cannot close T32's assignment.
The full raw output and argument vectors are in the receipts file.

## Language and evidence limits

The sentence check covers task steps, pass conditions, removal instructions, stop conditions, test procedures, and the goal prompt.
It uses whitespace-separated words and a simple sentence splitter.
It does not check every dictionary entry or every ASD-STE100 writing rule.
Do not use this result as a full ASD-STE100 conformance claim.

All 46 tasks remain `NOT_STARTED`.
All 125 cases and their per-task assignment results remain `NOT_RUN`.
The initial status text in `ACCEPTANCE.md` remains fixed during execution.
Implementation completion still requires the task, review, and runtime evidence in the pack.
