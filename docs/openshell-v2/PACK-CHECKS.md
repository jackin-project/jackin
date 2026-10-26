# Planning-pack checks

Date: 2026-10-26.
Result: Static pack validation and per-task assignment fixtures pass.

## Scope

This report validates planning artifacts and validator behavior only.
It does not validate Jackin code, an OpenShell deployment, or the proposed upstream contracts.
No product runtime test or task implementation has been performed.
The validator does not inspect task artifact contents or review decisions.
These release-gate checks validate static planning structure only. They do not run a release preflight, test a candidate, or issue approval.
T39 records impact and fresh LIVE reruns after final legacy deletions. Later runtime-affecting changes, including T40 version and package changes, require fresh results for affected cases; unjustified impact requires the full applicable LIVE matrix.
Original case owners or operators execute reruns while T40 coordinates. Append records, retain early, failed, blocked, and independent-repeat evidence, and bind fresh results to final source, inputs, artifacts, target, run ID, and independent review.
Preflight uses completed prior evidence only. Finalization refuses missing, stale, failed, or blocked required LIVE results after the latest runtime-affecting change.
Tasks without cases still require their task-card artifacts and independent review.
The original specification remains unchanged.
Task, case, graph, and traceability authoring fields are pinned independently of progress state.
The full authored PLAN prefix before its unique task-index boundary is pinned separately from the canonical index and task-card checks.
The graph root `PLAN_ONLY` value classifies this artifact; it is fixed while task, case, assignment, and aggregate progress remains mutable.
Task, case, assignment, and aggregate statuses remain mutable during ordinary execution.
Complete CONTRACTS and SUBAGENTS files are pinned after their reviewed policy decisions.
The report template is pinned outside its named result and input-derived count slots.

## Reproduce

Run these commands from the repository root:

```sh
python3 docs/openshell-v2/validate_pack.py --initial
python3 docs/openshell-v2/validate_pack.py
python3 docs/openshell-v2/validate_assignment_fixtures.py > /tmp/openshell-assignment-results-receipts.json
```

The first command requires all task statuses to remain `NOT_STARTED` and all case and assignment statuses to remain `NOT_RUN`.
Omit `--initial` after execution starts.
The ordinary validator accepts documented progress states.
Both execution markers must be JSON Booleans in ordinary and initial validation. A LIVE assignment recorded as `PASS` or `FAIL` requires `runtime_tests_executed_for_this_pack` to be `true`; `BLOCKED`, `NOT_RUN`, SOURCE evidence, and BUILD evidence do not assert runtime execution. Serial work remains valid when `subagents_used_for_this_pack` is `false`.
It requires one `assignment_results` row for every task owner on every case.
Rows contain exactly `task_id` and `status`; duplicate, missing, and extra owners fail validation.
Each `PASS` task needs `PASS` for its own case assignments.
Both `NOT_STARTED` and `READY` tasks must keep every owned assignment at `NOT_RUN`.
The case-level `status` remains for existing readers and is derived from all assignments.
It is `PASS` only when all assignments pass.
It is `FAIL` if any assignment fails.
It is `BLOCKED` when no assignment fails and one or more assignments are blocked.
It is `NOT_RUN` otherwise.
The manifest root `status` is a documented status value derived from all detailed case statuses.
It uses the same precedence: `FAIL`, then `BLOCKED`, then `PASS` only when every case passes, and `NOT_RUN` otherwise.
The acceptance manifest keeps schema version `1` and adds `assignment_results`.
The upgraded validator requires that field; old manifests without it fail validation.
Markdown assignment labels record initial `NOT_RUN` status and stay fixed during ordinary progress.

`validate_assignment_fixtures.py` runs the validator on temporary pack copies.
Each receipt records the actual argument vector, stdout, stderr, exit code, executed validator path and hash, and fixture-source hashes.
Each receipt hashes every tested pack input after mutation. It records changed-input hashes before and after mutation, with explicit `present` or `missing` states.
The fixture runner keeps its validator-check inventory and 250 scenario names independent from the validator and fixture calls under test. It also compares 18 named runner controls with a separate fixed inventory that rejects missing, duplicate, and unexpected controls.
Each fixture, original-board, and probe subprocess runs in its own process group with a finite timeout. On timeout the runner terminates that group, preserves partial stdout and stderr, and bounds pipe draining and direct-child reaping. It records observed exit information, and a timeout remains a failure even after a passing report.
The runner snapshots pack inputs before importing the validator and immediately after import. It captures validator-import stdout and stderr, and reports import output, exceptions, and `SystemExit` codes as structured failures before running fixtures. It emits one JSON document. A subprocess fixture proves it detects an import-time README mutation on a temporary copy. `KeyboardInterrupt` remains an uncaught cancellation.
The runner anchors each selected pack root at its canonical path, so an operating-system alias above the root remains usable. An external regular validator executable is allowed to inspect a selected pack that omits its validator file. Both tools reject linked entry points, pack-local entries escaping the root, and linked pack inputs before reads or imports. They record link metadata without following or removing links, and refuse copies that would follow links. The supported self-contained pack copy has matching source and destination hashes.
The top-level source map hashes the validator, fixture runner, and every source-pack file.
The runner does not modify the pack. It validates the original board in ordinary mode and records its before and after hashes. Redirect stdout outside the repository to retain raw receipts.

## Counts

| Item | Count |
| --- | ---: |
| Detailed task cards | 46 |
| Acceptance cases | 125 |
| Mapped specification headings | 30 |
| Topological readiness groups | 20 |
| Checked procedural sentences | 1663 |

The ordinary validator report contains 86 ordered checks; `--initial` contains 88, including two initial-only checks and the final inventory check.
The C02 digest checks fixed contract text only. It does not show that source code follows this plan.
Readiness groups indicate dependency order, not permission to ignore file ownership.
Keep no more than eight actual subagents active at once.

## Check results

| Check | Result |
| --- | --- |
| Required pack files, including fixture tool | PASS |
| Manifest schema versions are supported integers | PASS |
| Both execution markers are exact Booleans in ordinary and initial modes | PASS |
| LIVE `PASS` and `FAIL` assignments require the runtime-execution marker | PASS |
| Fixture scenario names match the independent 244-name inventory with no missing, duplicate, or unexpected entries | PASS |
| The 18 runner controls match an independent fixed inventory with no missing, duplicate, or unexpected entries | PASS |
| Fixture, original-board, and probe subprocesses use owned process groups and bounded timeout cleanup | PASS |
| Validator import output, exceptions, and `SystemExit` become structured failures; cancellation remains uncaught | PASS |
| Canonical root aliases work; external file and in-pack ancestor symlinks fail before reads or copying | PASS |
| C01 source table matches all four exact fixed references | PASS |
| C02 crate graph and dependency-boundary section matches its fixed bytes | PASS |
| Machine source pins match the three fixed graph references | PASS |
| One authoritative concurrency declaration matches the graph and fixed limit | PASS |
| Task IDs and case IDs are unique | PASS |
| Declared and expected task and case counts match | PASS |
| Original specification matches its fixed checksum and both manifests | PASS |
| GOAL matches its fixed checksum | PASS |
| Dependencies reference known tasks | PASS |
| Dependency lists contain unique string task IDs before DAG checks | PASS |
| Task and case statuses use documented values | PASS |
| Acceptance evidence levels use documented values | PASS |
| All 125 case IDs retain their fixed evidence levels | PASS |
| Acceptance manifest status uses documented values and matches the case summary | PASS |
| A86 separates T26 backend refusal checks from T29 finished CLI, Console, and desktop presentation | PASS |
| A86 mutation controls reject either owner or phase removal, weakened threats, and a finished-UI prerequisite for T26 | PASS |
| Task case references are unique string IDs; completion rules match the fixed contract | PASS |
| Each task uses the different-subagent review and post-merge integration rerun rule | PASS |
| Release protocol includes conditional patched-tuple approval, runtime requalification, T40 review authority, and Integration finalization writes | PASS |
| Preflight has no future T40-output prerequisite; finalization requires fresh post-change LIVE results | PASS |
| Finalization lists fresh bound LIVE PASS evidence as a required input | PASS |
| T40 version and package changes trigger fresh LIVE requalification | PASS |
| A123 and A125 match the fixed freshness-refusal and patched-tuple authorization contracts | PASS |
| T39 records final-deletion impact and fresh LIVE reruns; T40 coordinates later changes without changing case owners | PASS |
| T40 finalization write scope routes shared records through Integration | PASS |
| T40 routes target and compatibility changes through ordered Integration requests | PASS |
| Subagent role-task assignments exactly match graph owners | PASS |
| Review packets bind candidate diffs before merge and require merged checks before completion | PASS |
| P02 early LIVE records, P04 imports, and separate T12 reruns match the fixed contract | PASS |
| P04 command-dispatch patch and P05 build-registration scopes match | PASS |
| Acceptance cases have unique string task owners | PASS |
| Dependency graph has no cycle and stored groups match | PASS |
| Assignment rows have exact unique owners and valid statuses | PASS |
| Case status matches its assignment summary | PASS |
| `NOT_STARTED` and `READY` tasks have `NOT_RUN` assignments | PASS |
| `BLOCKED` tasks with incomplete dependencies have `NOT_RUN` assignments | PASS |
| Every `REVIEW` task has a recorded result for each of its own assignments | PASS |
| Task progress waits for completed dependencies | PASS |
| `PASS` tasks have passing results for their own assignments | PASS |
| Task test references, case coverage, and ownership maps agree | PASS |
| A13 keeps P05's current graph run separate from T39's final post-deletion rerun | PASS |
| A15 keeps U00/T13 layer checks and T23 integrated StartSession qualification | PASS |
| A16 keeps U00/T13 accepted-revision checks and T29 product presentation | PASS |
| A17 keeps U00/T13 provider observations and T23 integrated StartSession/re-admission | PASS |
| PLAN's authored prefix matches its unique task-index boundary and fixed digest | PASS |
| Target-matrix requests route through Integration; required and unavailable rows remain | PASS |
| Task required artifacts match the canonical six receipts and report path | PASS |
| Task-card and test-write paths match each task ID exactly; recursive Markdown packets match the manifest set | PASS |
| Task graph matches indexes and task cards, including report paths | PASS |
| Individual task packets match their ID and complete canonical instruction preamble | PASS |
| Acceptance detailed-case anchor is unique | PASS |
| Acceptance policy before detailed cases matches fixed checksum | PASS |
| Acceptance case blocks match the manifest | PASS |
| Task SPEC citations have valid syntax and in-range lines | PASS |
| Every cited SPEC section lists its task owner in traceability | PASS |
| Traceability headings, ranges, owners, and representations match | PASS |
| Checked procedural sentences stay within the word limit | PASS |
| Authored task and case fields match fixed projections, excluding progress state | PASS |
| Graph declarations, including the fixed `PLAN_ONLY` root classification, and complete traceability records match fixed projections | PASS |
| Complete [CONTRACTS.md](CONTRACTS.md) and [SUBAGENTS.md](SUBAGENTS.md) match their fixed checksums | PASS |
| PACK-CHECKS template is fixed outside mutable count and result slots | PASS |
| Validator emits the canonical ordered ordinary and initial-only check inventories | PASS |
| Initial states claim no task or case execution (`--initial` only) | PASS |
| Pack records no runtime tests or delegated task reviews (`--initial` only) | PASS |
| Validator inventory matches the fixed ordered check sequence | PASS |

## Assignment-result causal fixtures

All 250 scenarios passed their exact report, input-derived count, and exit-code expectations.
All 18 runner controls passed their independent inventory and behavior checks.
The original 18 scenario intentions remain unchanged; 232 targeted scenarios cover assignment ownership and progress, execution-marker types and LIVE status requirements, readiness-group input shapes, authored task and case fields, source citations, canonical task paths and exact recursive packet sets, artifacts and packets, evidence levels, early LIVE ownership, command registration, fixed acceptance and PLAN prefixes, strict JSON parsing and exact short-form input details, release-gate protocol structure, post-deletion and post-change LIVE freshness, T40 version/package requalification, finalization evidence requirements, Integration write ownership, patched-tuple authorization, GOAL and concurrency pins, C01 pins, C02 integrity, complete contract and subagent policy pins, task review rules, validator report templates and result-cell grammar, independent check-inventory comparison, import-time input mutation detection, the fixed root planning classification, owner-specific A13/A15/A16/A17/A86 qualification, serialized target and compatibility-matrix updates, malformed record refusal, and review-assignment completion.
The separate A86 runner control applies eight independent mutations for either owner, either phase, hostile input, operator intent, trusted labels or approval state, and T26's premature finished-presentation prerequisite.
Each fixture copy records source hashes, permitted normalization changes, normalized hashes, intentional mutations, and input and executable hashes before and after validation. Initial-mode copies reset only existing valid progress statuses and true Boolean execution markers; missing or invalid markers remain unchanged and fail validation. The original board is never normalized or repaired.
The existing missing-P00 fixture retains its packet-preamble failure and exit code 1; exact-path validation adds a second expected failure for the missing canonical path.
A22 is owned by U02 and T32. Its mixed-result fixtures complete T32 dependencies before recording results.
A117 is owned by sibling tasks T35, T36, and T37, plus downstream T38. It checks FAIL precedence without bypassing dependencies.

| Temporary-copy fixture | Expected validator result | Observed |
| --- | --- | --- |
| All assignments pass and the shared case summary is `PASS` | Accept, exit 0 | PASS, exit 0 |
| Task `PASS` with its own case assignments `PASS` | Accept, exit 0 | PASS, exit 0 |
| U02 `PASS`, T32 `NOT_RUN`, summary `NOT_RUN`, with completed prerequisites | Accept, exit 0 | PASS, exit 0 |
| U02 `PASS`, T32 `BLOCKED`, summary `BLOCKED`, with completed prerequisites | Accept, exit 0 | PASS, exit 0 |
| A117 has sibling `FAIL` and `BLOCKED` results; summary uses `FAIL` precedence | Accept, exit 0 | PASS, exit 0 |
| T32 task `PASS` while T32/A22 remains `NOT_RUN` | Reject, exit 1 | PASS, exit 1 |
| Legacy summary `PASS` while T32/A22 remains `NOT_RUN` | Reject, exit 1 | PASS, exit 1 |
| Legacy manifest without `assignment_results` | Reject, exit 1 | PASS, exit 1 |
| Malformed or invalid-status, duplicate, missing, or extra assignment row | Reject, exit 1 | PASS, exit 1 |
| Assignment status changes under `--initial` | Reject, exit 1 | PASS, exit 1 |
| Missing or non-Boolean execution markers in ordinary and initial modes | Reject, exit 1 | PASS, exit 1 |
| LIVE `PASS` or `FAIL` without a true runtime marker; matching `BLOCKED`, SOURCE, and BUILD controls | Reject or accept according to evidence state | PASS, exit 0 |
| Non-list readiness groups, non-list groups, and non-string task IDs | Structured input error, exit 2 | PASS, exit 2 |
| Malformed assignment row under `--initial` | Reject cleanly, exit 1 | PASS, exit 1 |
| T33 runs before T27 checkpoint task passes | Reject, exit 1 | PASS, exit 1 |
| `BLOCKED` task and its blocked assignments | Accept as blocked, exit 0 | PASS, exit 0 |
| `BLOCKED` task retains prior `PASS`, `FAIL`, and `BLOCKED` results after its dependencies pass | Accept, exit 0 | PASS, exit 0 |
| U06 `PASS` with no assigned acceptance cases | Accept under existing no-case completion rule, exit 0 | PASS, exit 0 |
| Duplicate or non-string task case references | Reject before set-based ownership checks, exit 1 | PASS, exit 1 |
| Root acceptance status is stale or undocumented | Reject stale summary or unknown status, exit 1 | PASS, exit 1 |
| Root summary mixes `FAIL` and `BLOCKED` | Accept with `FAIL` precedence, exit 0 | PASS, exit 0 |
| A task completion rule differs from the fixed contract | Reject, exit 1 | PASS, exit 1 |
| Release protocol weakens prior-task/final-case/review/merged-check requirements | Reject, exit 1 | PASS, exit 1 |
| Runtime protocol weakens post-T39 scope, full-matrix fallback, original owners, evidence retention/bindings, freshness refusals, or future-output-free preflight | Reject, exit 1 | PASS, exit 1 |
| T39/T40 task mirrors or A123 omit post-deletion LIVE freshness | Reject, exit 1 | PASS, exit 1 |
| T40 removes the mandatory version/package requalification trigger | Reject, exit 1 | PASS, exit 1 |
| Finalization can omit fresh bound LIVE PASS evidence | Reject, exit 1 | PASS, exit 1 |
| Release preflight can approve, or a P04/T40/A123 procedure or acceptance mirror is weakened | Reject, exit 1 | PASS, exit 1 |
| P04 qualification ownership or review-before-merge requirements are missing | Reject, exit 1 | PASS, exit 1 |
| Pack report omits the static-only release-gate scope statement | Reject, exit 1 | PASS, exit 1 |
| `NOT_STARTED` owner has `PASS`, `FAIL`, or `BLOCKED` assignment | Reject, exit 1 | PASS, exit 1 |
| `FAIL` task has an incomplete dependency | Reject, exit 1 | PASS, exit 1 |
| `FAIL` task follows completed dependencies with `NOT_RUN` and `PASS` cases | Accept, exit 0 | PASS, exit 0 |
| `BLOCKED` task waits on an incomplete dependency with `NOT_RUN` cases | Accept, exit 0 | PASS, exit 0 |
| `READY` owner has `PASS`, `FAIL`, or `BLOCKED` assignment | Reject, exit 1 | PASS, exit 1 |
| `READY` owner retains `NOT_RUN` assignment | Accept, exit 0 | PASS, exit 0 |
| `BLOCKED` U02 retains its prior `PASS` while T32 remains `NOT_RUN` | Accept, exit 0 | PASS, exit 0 |
| `BLOCKED` owner with incomplete dependencies has a `PASS`, `FAIL`, or `BLOCKED` assignment | Reject, exit 1 | PASS, exit 1 |
| `BLOCKED` owner with incomplete dependencies retains only `NOT_RUN` assignments | Accept, exit 0 | PASS, exit 0 |
| Duplicate case owner with synchronized acceptance block | Reject, exit 1 | PASS, exit 1 |
| Copied pack lacks validator; real external validator runs | Reject missing required file, exit 1 | PASS, exit 1 |
| Missing validator short form names only `validate_pack.py` | Reject with exact missing-file detail | PASS |
| Missing `tasks` manifest key and missing P00 packet identify their input | Reject with exact task detail | PASS, exit 2 / exit 1 |
| Temporary-copy status normalization records only existing status-field changes | Accept pristine `--initial` baseline, exit 0 | PASS, exit 0 |
| Consistent progressed P00/A01 assignment | Accept ordinary board, exit 0 | PASS, exit 0 |
| Invalid temporary source board with a missing A01 assignment is not repaired by normalization | Reject, exit 1 | PASS, exit 1 |
| Report counts match task, case, SPEC, dependency, and sentence counts derived from tested files | Accept matching counts; reject mismatches | PASS |
| Validator input or executable changes during execution | Reject fixture integrity violation | PASS |
| Original source board passes separate ordinary validation without file changes | Accept unchanged board, exit 0 | PASS, exit 0 |
| Acceptance policy prefix is mutated or a line is removed | Reject checksum mismatch, exit 1 | PASS, exit 1 |
| T14 is missing from one cited traceability section while mapped elsewhere, with both tables synchronized | Reject, exit 1 | PASS, exit 1 |
| Task SPEC citation syntax is malformed or its line is out of range | Reject, exit 1 | PASS, exit 1 |
| Malformed task manifest emits the structured input-error report | Reject input, exit 2 | PASS, exit 2 |
| Empty, invalid, non-object, incomplete, or mistyped validator report | Reject malformed report | PASS |
| Inconsistent counts, checks, failures, scope, or exit code, including an input-error report with exit 1 | Reject malformed report | PASS |
| Duplicate or malformed check records | Reject malformed report | PASS |
| Unexpected extra failure name | Reject report against exact expected failures | PASS |
| Ordinary baseline report contains every check in canonical order | Accept, exit 0 | PASS, exit 0 |
| Initial baseline report contains ordinary checks and initial-only checks in canonical order | Accept, exit 0 | PASS, exit 0 |
| Truncated passing or failing report | Reject malformed report | PASS |
| Report with a missing middle check, reordered checks, or an extra check | Reject malformed report | PASS |
| Initial report without the required initial-only tail | Reject malformed report | PASS |
| Report initial flag differs from the validator invocation | Reject malformed report | PASS |
| Validator implementation and its own check inventory both omit the expected task-count check | Reject the shortened report using the fixture runner's independent inventory | PASS |
| Graph root claims release approval instead of the fixed `PLAN_ONLY` classification | Reject, exit 1 | PASS, exit 1 |
| Validator import mutates README in an isolated subprocess | Detect changed pre-import and post-import hashes | PASS |
| Canonical review receipt removed with synchronized task graph, PLAN, and card | Reject, exit 1 | PASS, exit 1 |
| Report path relocated with synchronized task graph, PLAN, and card | Reject, exit 1 | PASS, exit 1 |
| U00 upstream-change receipt removed with synchronized task graph, PLAN, and card | Reject, exit 1 | PASS, exit 1 |
| P02 early LIVE receipt removed with synchronized task graph, PLAN, and card | Reject, exit 1 | PASS, exit 1 |
| P02 early LIVE execution requirement removed | Reject, exit 1 | PASS, exit 1 |
| T12 separate fresh LIVE rerun requirement removed | Reject, exit 1 | PASS, exit 1 |
| P04 import-without-T12-credit requirement removed | Reject, exit 1 | PASS, exit 1 |
| P04 command-dispatch scope widened beyond the Integration patch | Reject, exit 1 | PASS, exit 1 |
| P05 command-dispatch ownership deferred from P04 | Reject, exit 1 | PASS, exit 1 |
| P05 workspace and build graph registration step removed | Reject, exit 1 | PASS, exit 1 |
| Integration assignment row omits a graph-owned task | Reject, exit 1 | PASS, exit 1 |
| Graph owner changes while task cards and indexes agree | Reject, exit 1 | PASS, exit 1 |
| T40 claims direct writes for shared finalization records | Reject, exit 1 | PASS, exit 1 |
| T40 claims direct writes for `tests/v2/targets.toml` or `compatibility.lock.toml` | Reject, exit 1 | PASS, exit 1 |
| Protocol assigns finalization writes to T40 | Reject, exit 1 | PASS, exit 1 |
| Conditional patched-tuple approval is removed from the protocol | Reject, exit 1 | PASS, exit 1 |
| Unpatched tuples are made to require approval records | Reject, exit 1 | PASS, exit 1 |
| A125 omits its patched-tuple approval check | Reject, exit 1 | PASS, exit 1 |
| C01 source table omits, duplicates, or adds a fixed reference | Reject, exit 1 | PASS, exit 1 |
| C01 source table alters the release version or either commit | Reject, exit 1 | PASS, exit 1 |
| Individual task packet omits its preamble or uses the wrong task ID | Reject, exit 1 | PASS, exit 1 |
| Individual task packet weakens lease, dependency, or authorization instructions | Reject, exit 1 | PASS, exit 1 |
| Individual task packet adds a conflicting instruction | Reject, exit 1 | PASS, exit 1 |
| C02 crate row is removed or its responsibility changes | Reject, exit 1 | PASS, exit 1 |
| C02 dependency-boundary prose is weakened | Reject, exit 1 | PASS, exit 1 |
| C02 or C03 boundary heading is duplicated or missing | Reject, exit 1 | PASS, exit 1 |
| Synchronized task packet changes the review rule to allow self-review | Reject, exit 1 | PASS, exit 1 |
| Synchronized task packet removes the post-merge integration rerun | Reject, exit 1 | PASS, exit 1 |
| Task-card and test-write paths are relocated, traversing, absolute, or colliding | Reject, exit 1 | PASS, exit 1 |
| Exact task packet set matches every manifest ID | Accept, exit 0 | PASS, exit 0 |
| Nested or archived Markdown is added under `tasks/` | Reject, exit 1 | PASS, exit 1 |
| A canonical Markdown task packet is missing | Reject, exit 1 | PASS, exit 1 |
| Empty roots remain valid; malformed lists fail before DAG checks; unknown, self, and cycle diagnostics remain | Accept root; reject invalid dependency, exit 1 | PASS |
| A documented evidence level is changed for a case ID | Reject, exit 1 | PASS, exit 1 |
| GOAL is truncated, removed, or given a conflicting execution-control clause | Reject, exit 1 | PASS |
| Authoritative concurrency is changed, missing, or duplicated | Reject, exit 1 | PASS, exit 1 |
| Each of the three manifests rejects duplicate top-level and nested JSON keys | Reject input, exit 2 | PASS, exit 2 |
| Fixture report parsing rejects duplicate top-level and nested JSON keys | Reject malformed report | PASS |
| P05 cannot defer A13 to the final graph; T39 must rerun after deletion | Reject, exit 1 | PASS, exit 1 |
| A15 cannot omit T23 or weaken its integrated StartSession barrier | Reject, exit 1 | PASS, exit 1 |
| U00 and T13 retain A15/A16 LIVE layer checks, accepted revisions, replacement source, and no-intersection claims | Reject, exit 1 | PASS, exit 1 |
| A16 cannot omit T29 or weaken accepted-revision product presentation | Reject, exit 1 | PASS, exit 1 |
| A17 cannot omit T23 integrated activation or re-admission ownership | Reject, exit 1 | PASS, exit 1 |
| A86 cannot remove T29 integrated presentation or weaken hostile-input assertions | Reject, exit 1 | PASS, exit 1 |
| P04 initial and T35–T38 qualified target rows use ordered Integration requests | Reject, exit 1 | PASS, exit 1 |
| P04 and qualification tasks cannot write the central target matrix directly | Reject, exit 1 | PASS, exit 1 |
| Required target rows remain and unavailable rows stay `BLOCKED` | Reject, exit 1 | PASS, exit 1 |
| Synchronized A22 prose cannot weaken its owner-specific assertions | Reject, exit 1 | PASS, exit 1 |
| C06 credential-authority fail-open cannot alter the complete contracts | Reject, exit 1 | PASS, exit 1 |
| T23 dependency removal or addition fails after all graph mirrors are recomputed | Reject, exit 1 | PASS, exit 1 |
| Worker root-Cargo claims and contradictory task scopes fail the authored projection | Reject, exit 1 | PASS, exit 1 |
| T23 citation rebasing fails with synchronized traceability | Reject, exit 1 | PASS, exit 1 |
| SUBAGENTS shared-file authority cannot be weakened | Reject, exit 1 | PASS, exit 1 |
| PACK-CHECKS rejects conflicting prose outside result slots | Reject, exit 1 | PASS, exit 1 |
| PACK-CHECKS keeps count and result slots mutable | Accept, exit 0 | PASS, exit 0 |
| PACK-CHECKS check results accept only documented outcome tokens | Reject release-approval prose, exit 1 | PASS, exit 1 |
| PACK-CHECKS observations accept only outcome tokens and optional exit codes | Reject release-approval prose, exit 1 | PASS, exit 1 |
| PLAN prefix mutation or synchronized concurrency weakening | Reject fixed-prefix change, exit 1 | PASS, exit 1 |
| PLAN task-index boundary is missing or duplicated | Reject, exit 1 | PASS, exit 1 |
| Task, case, and traceability manifests reject null records | Reject structured input, exit 2 | PASS, exit 2 |
| All three manifests reject NaN, Infinity, and -Infinity | Reject structured input, exit 2 | PASS, exit 2 |
| Fixture report parser rejects NaN, Infinity, and -Infinity | Reject malformed report | PASS |
| `REVIEW` rejects any own `NOT_RUN` assignment despite another owner's result | Reject, exit 1 | PASS, exit 1 |
| `REVIEW` accepts own `PASS`, `FAIL`, and `BLOCKED` results, plus no-case review | Accept, exit 0 | PASS, exit 0 |
| A13 cannot omit Cargo metadata for P05's current graph | Reject, exit 1 | PASS, exit 1 |
| A13 cannot omit fresh Cargo metadata for T39's final post-deletion graph | Reject, exit 1 | PASS, exit 1 |
| A16 loads the global policy before querying revisions and displaying its source | Reject, exit 1 | PASS, exit 1 |
| Canonical reports reject wrong nonempty counts and nonempty all-zero counts | Reject malformed report | PASS |
| Missing-validator short form rejects fabricated [README.md](README.md) detail | Reject against expected `validate_pack.py` detail | PASS |
| Missing-`tasks` input-error short form rejects fabricated `cases` detail | Reject against expected `tasks` detail | PASS |

The upstream `PASS` fixture shows U02 cannot close T32's assignment.
Each raw receipt binds every mutated file actually tested. The traceability-owner receipt hashes both `traceability.json` and `TRACEABILITY.md`; the missing-validator receipt records `validate_pack.py` as `missing`.
Receipts also hash task cards, `PLAN.md`, `SUBAGENTS.md`, `PACK-CHECKS.md`, and every other copied pack input. They preserve the actual validator executable and fixture source hashes.
The runner does not write receipts inside the repository.
Release-gate fixtures mutate only temporary copies of the protocol, its mirrors, and the task-assignment table. They test static refusal and consistency rules, not a release candidate or runtime finalizer.

## Language and evidence limits

The sentence check covers task steps, pass conditions, removal instructions, stop conditions, test procedures, and the goal prompt.
It uses whitespace-separated words and a simple sentence splitter.
It does not check every dictionary entry or every ASD-STE100 writing rule.
Do not use this result as a full ASD-STE100 conformance claim.

All 46 tasks remain `NOT_STARTED`.
The graph root remains `PLAN_ONLY`; this classifies the planning artifact and does not freeze task progress.
All 125 cases and their per-task assignment results remain `NOT_RUN`.
The initial status text in `ACCEPTANCE.md` remains fixed during execution. Its additive policy prefix digest is `7609a5f84a1694b1441219ac62e289191d90f367c33a400853ec64728063d36d`.
Implementation completion still requires the task, review, and runtime evidence in the pack.
