# Planning-pack checks

Date: 2026-10-26.
Result: All static planning-pack checks passed in ordinary and initial modes.

## Scope

This report validates the planning artifacts only.
It does not validate Jackin code, an OpenShell deployment, or the proposed upstream contracts.
No product runtime test or task implementation has been performed.
The independent pack review checked task and case consistency. It did not qualify runtime behavior.
The original specification remains unchanged.

## Reproduce

Run both commands from the repository root while all tasks and cases remain untouched:

```sh
python3 docs/openshell-v2/validate_pack.py --initial
python3 docs/openshell-v2/validate_pack.py
```

The ordinary command accepts documented progress states.
The `--initial` command adds a check that all task and case statuses remain `NOT_STARTED` and `NOT_RUN`.
Omit `--initial` after execution starts.
The validator checks the plan. It is not the product acceptance harness described in P04.
Missing required files produce a structured report and exit 1.
With `--json`, manifest read or shape errors also produce a structured error report and exit 2.
That report has empty counts because validation stopped before counts were available.

## Counts

| Item | Count |
| --- | ---: |
| Detailed task cards | 46 |
| Acceptance cases | 125 |
| Mapped specification headings | 30 |
| Topological readiness groups | 19 |
| Checked procedural sentences | 1510 |

Readiness groups indicate dependency order, not permission to ignore file ownership.
Keep no more than eight actual subagents active at once.

## Check results

| Check | Result |
| --- | --- |
| Required pack files, including this report | PASS |
| Task statuses use documented values | PASS |
| Acceptance case statuses use documented values | PASS |
| Acceptance evidence levels use documented values | PASS |
| Manifest schema versions match the supported version | PASS |
| Machine source pins match C01 | PASS |
| Parallel agent limit matches the subagent contract | PASS |
| Task IDs are unique | PASS |
| Case IDs are unique | PASS |
| Declared and expected task and case counts match | PASS |
| Original specification matches its fixed checksum and both manifests | PASS |
| Dependencies reference known tasks and the graph has no cycle | PASS |
| Task progress waits for completed dependencies | PASS |
| Stored readiness groups match dependencies with no duplicate task IDs | PASS |
| Task test references exist and every case has a task owner | PASS |
| Case and task ownership maps agree | PASS |
| Task graph fields match TASKS index table | PASS |
| Task graph fields match PLAN task index table | PASS |
| PLAN task headings match the task graph | PASS |
| Task graph fields match PLAN task cards | PASS |
| Task graph fields match individual task cards | PASS |
| Acceptance case headings have no missing, duplicate, or extra blocks | PASS |
| Acceptance case fields match the acceptance-case manifest | PASS |
| Every specification heading is mapped | PASS |
| Traceability ranges match exact specification sections | PASS |
| Traceability table matches its manifest | PASS |
| Traceability uses known task owners | PASS |
| Checked procedural sentences contain at most 20 whitespace-separated words | PASS |
| Initial task and case statuses claim no execution (`--initial` only) | PASS |
| Pack records no runtime tests or delegated task reviews (`--initial` only) | PASS |

## Language limit

The sentence check covers task steps, pass conditions, removal instructions, stop conditions, test procedures, and the goal prompt.
It uses whitespace-separated words and a simple sentence splitter.
It does not check every dictionary entry or every ASD-STE100 writing rule.
Do not use this result as a full ASD-STE100 conformance claim.

## Execution status

All 46 tasks remain `NOT_STARTED`.
All 125 acceptance cases remain `NOT_RUN`.
The `Initial status: NOT_RUN` text in `ACCEPTANCE.md` records the frozen starting state.
The current JSON status fields may change during execution.
Implementation completion requires the actual task, review, and runtime evidence specified in the pack.
