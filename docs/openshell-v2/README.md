# Jackin v2 execution pack

Date: 2026-10-25.
Status: Plan for a breaking rewrite. No product implementation is included.

## Start

1. Put this [docs/openshell-v2](./) directory in the Jackin repository.
2. Keep [SPEC.md](./SPEC.md) unchanged.
3. Give the execution agent access to this directory and the repository.
4. Paste the complete contents of [GOAL.md](./GOAL.md) into the execution runner.
5. Let the coordinator execute the task graph in [task-graph.json](./task-graph.json).

`/goal` is the requested prompt prefix. This pack does not assume a particular runner command implementation.

## Read order

| File | Purpose |
| --- | --- |
| [SPEC.md](./SPEC.md) | The complete user-supplied architecture specification. |
| [SOURCE-AUDIT.md](./SOURCE-AUDIT.md) | Source findings, inspection limits, and source references. |
| [GOAL.md](./GOAL.md) | The execution prompt. |
| [CONTRACTS.md](./CONTRACTS.md) | Fixed implementation decisions and required boundaries. |
| [SUBAGENTS.md](./SUBAGENTS.md) | Subagent assignments, write ownership, review rules, and report formats. |
| [PLAN.md](./PLAN.md) | Detailed task cards and dependencies. |
| [ACCEPTANCE.md](./ACCEPTANCE.md) | Required test procedures and release gates. |
| [TERMS.md](./TERMS.md) | Technical terms and writing rules. |
| [task-graph.json](./task-graph.json) | The machine-readable task graph. |
| [acceptance-cases.json](./acceptance-cases.json) | The machine-readable test procedures. |
| [TASKS.md](./TASKS.md) | A compact task index. |
| [tasks/](./tasks) | One self-contained task card for each subagent assignment. |
| [TRACEABILITY.md](./TRACEABILITY.md) | Every specification section mapped to implementation tasks. |
| [traceability.json](./traceability.json) | The machine-readable section mapping. |
| [PACK-CHECKS.md](./PACK-CHECKS.md) | Static pack checks and explicit validation limits. |
| [validate_pack.py](./validate_pack.py) | Checks task links, per-task case results, dependencies, and task sentence lengths. |
| [validate_assignment_fixtures.py](./validate_assignment_fixtures.py) | Records causal fixtures for per-task acceptance results. |
| [assignment-results-receipts.json](./validation/assignment-results-receipts.json) | Raw validator fixture commands and results. |

## Source authority

The specification selects the product architecture. The task cards define its implementation sequence.
Qualified upstream source defines the actual runtime API. The plan does not create an upstream capability by naming it.
Historical experiment reports supply regression cases. They do not prove current support.
Earlier chat answers are not an implementation authority.

Keep a conflict open when these sources disagree. Do not hide the conflict through a fallback or weaker test.

## Scope

Build a fresh v2 product. Remove the old agent execution implementation.
Keep Docker and Buildx only as trusted image infrastructure inside Jackin.
Use OpenShell for every agent domain and independent verification worker.
Use an OpenShell-owned companion for the optional Docker capability.
Do not substitute an external daemon or the old DinD manager.

Complete release approval includes the companion and strict verifier contracts.
A smaller demonstration is not full completion.

## Review limits

The complete attachment was read. The source audit inspected selected files at explicit commits.
It did not inspect every repository file or execute Jackin, OpenShell, or the historical experiments.
The execution plan therefore starts with a complete source inventory and fresh qualification tests.
No subagent runner was available for this planning session. No delegated review is claimed.
The execution instructions require real subagents when the execution runner supplies them.

The task prose uses short commands and a controlled technical glossary.
An independent ASD-STE100 dictionary and conformance review has not been completed.
Do not label this pack as certified ASD-STE100 documentation.

## Delivery status

All task statuses start as `NOT_STARTED`.
All case and per-task assignment results start as `NOT_RUN`.
The case-level status is a summary of its per-task results.
The validator requires `assignment_results`; older manifests without this field fail validation.
This pack includes no passing runtime evidence.
Do not change a status to `PASS` because a source file or proposed test exists.
