# Jackin v2 implementation plan

## Required result

Replace the implementation, not only its backend name.
Use OpenShell for every production agent domain and independent verification worker.
Keep Docker and Buildx only as trusted image infrastructure inside Jackin.
Use an OpenShell-owned companion for the optional Docker capability.

The final product has no old execution fallback, importer, compatibility parser, or dormant legacy backend.
Preserve original project data and unrelated resources throughout the rewrite.
Use the source inventory to retain required product behavior without retaining its old implementation.

## How to execute this plan

Read the complete task card before dispatch.
Do not infer task readiness from its position in this file.
Use the dependency graph in [task-graph.json](./task-graph.json).
Use up to eight real subagents when write scopes permit this.
Reserve an independent review subagent for each implementation result.

P01 resolves the exact current Jackin paths.
P02 resolves the exact upstream paths and symbols.
The coordinator converts each task scope into an exclusive file lease before dispatch.
A scope description does not grant permission to edit every file in a repository.
Use the task-specific test directory unless an existing qualified test location is required.
Record the exact test location in the task packet.

The integration subagent alone changes shared manifests, lockfiles, schemas, and generated CI.
Implementation workers submit exact patch requests for those files.
Do not use compatibility shims to make an intermediate commit compile.
Use explicit unsupported results while a new operation is unfinished.
Do not report those results as completed features.

## Completion rules for every task

1. Complete every numbered step.
2. Produce the listed artifacts and the task-specific source outputs.
3. Pass each task check at its declared evidence level.
4. Preserve the raw sanitized commands and results.
5. Obtain review from a different agent.
6. Merge through the integration subagent.
7. Re-run affected tests on the merged commit.
8. Update the task board only after these checks.

SOURCE and UNIT evidence cannot fill a LIVE test result.
Later target qualification repeats the runtime journeys on each required tuple.
A source review does not establish a platform capability.
An isolated successful component does not establish the combined account and session journey.
Record a separate result for each `(task_id, case_id)` assignment.
The global case status cannot replace the result for the current task.

## Integration gates

| Gate | Required result | Tasks |
| --- | --- | --- |
| G0: Frozen inputs | Complete inventory, real upstream contracts, schemas, and honest test harness. | P00–P05. |
| G1: Safe native path | Fenced runtime calls, unprivileged Capsule, image delivery, and a real-agent vertical path. | U00–U01 and T10–T23. |
| G2: Complete product behavior | Account domains, shell, host actions, retention, fleet, services, diagnostics, and workflow coverage. | T24–T31 and T34. |
| G3: Required upstream capabilities | Strict verifier, normal rebuilt binaries, and managed engine with real cleanup. | U02–U08 and T32–T33. |
| G4: Qualified targets | Local, remote, VM, and Kubernetes journeys with failure and resource evidence. | T35–T38. |
| G5: Clean replacement | No old implementation in source, dependencies, packages, or active documentation. | T39. |
| G6: Full release candidate | All mandatory gates, independent repeats, fresh setup, and accurate support claims. | T40. |

The gates overlap in development. They do not permit missing release requirements.
G1 is not full replacement approval.
G3 remains mandatory even when the ordinary engine-disabled profile works.

## Upstream patch policy

Make runtime changes in an OpenShell worktree or approved upstream fork.
Do not copy runtime controllers into Jackin.
Record the base commit, patch digest, owner, tests, and upstream contribution plan.
Update the compatibility tuple after each relevant patch.
Require explicit maintainer approval before a patched tuple becomes a supported release tuple.
Do not imply that stock OpenShell includes an unmerged extension.

## Test commands

P04 implements the commands in [ACCEPTANCE.md](./ACCEPTANCE.md).
Do not pretend they already exist in the old repository.
Use the actual source toolchain and pinned dependencies.
At the final gate, run formatting, workspace lint, unit tests, documentation tests, and the complete live matrix.
Keep build-only Docker use separate from sandbox execution in command traces.

## Task index

| ID | Task | Subagent | Dependencies |
| --- | --- | --- | --- |
| P00 | Freeze the work area and source evidence | Integration subagent | None |
| P01 | Inventory every old implementation path | Source audit subagent | P00 |
| P02 | Verify the upstream API and compatibility tuple | Upstream API subagent | P00 |
| P03 | Freeze the internal contracts and schemas | Planning review subagent | P01, P02 |
| P04 | Build the evidence and acceptance harness | Qualification subagent | P03 |
| P05 | Create the clean v2 crate graph | Integration subagent | P03, P04 |
| U00 | Qualify authoritative runtime observations | Upstream API subagent | P02, P03, P04 |
| U01 | Add server-side mutation identity fences | Upstream API subagent | P02, P03, P04 |
| U02 | Add strict verification-worker isolation | Upstream worker subagent | P02, P03, P04 |
| U03 | Qualify rebuilt executable network identity | Upstream worker subagent | P02, P03, P04 |
| U04 | Freeze the upstream companion API and ownership | Companion lifecycle subagent | P02, P03, P04 |
| U05 | Implement companion VM lifecycle and cleanup | Companion lifecycle subagent | U04, U01 |
| U06 | Implement confined companion project projection | Companion data subagent | U04 |
| U07 | Implement companion network ports and budgets | Companion network subagent | U04 |
| U08 | Qualify the complete managed companion | Qualification subagent | U05, U06, U07, U00, U03 |
| T10 | Implement v2 state and operation recovery | State subagent | P04, P05 |
| T11 | Implement the per-user host control service | State subagent | T10 |
| T12 | Implement native OpenShell connection handling | Runtime adapter subagent | P02, P05 |
| T13 | Implement scoped runtime operations and observations | Runtime adapter subagent | T12, T10, U00, U01 |
| T14 | Implement reproducible role resolution | Role-image subagent | P03, P05 |
| T15 | Implement explicit Buildx and image delivery | Role-image subagent | T14, T10, P02 |
| T16 | Build and validate portable role images | Role-image subagent | T15, T13, T18 |
| T17 | Implement the bounded Capsule protocol | Capsule subagent | P04, P05 |
| T18 | Rewrite Capsule as an unprivileged resident service | Capsule subagent | T17 |
| T19 | Implement relay attachment and input ownership | Capsule subagent | T18, T13, T11 |
| T20 | Implement portable project materialization | Project subagent | T13, T10 |
| T21 | Implement account grants and native providers | Account subagent | T13, T10 |
| T22 | Implement native-login files and agent adapters | Account subagent | T21, T20, T14 |
| T23 | Complete the native launch and activation path | Product subagent | T16, T19, T20, T21, T22, T11 |
| T24 | Implement domains sessions and the default shell | Product subagent | T23 |
| T25 | Implement trusted usage and account presentation | Account subagent | T21, T22, T24 |
| T26 | Implement host actions and approved tool domains | Product subagent | T24, T21, T13 |
| T27 | Implement checkpoints and conflict-safe export | Project subagent | T20, T22, T24 |
| T28 | Implement handoff eject exile and purge | Project subagent | T27, T13, T24 |
| T29 | Rebuild CLI Console and fleet observations | Product subagent | T24, T25, T26, T28 |
| T30 | Implement private developer service forwards | Product subagent | T13, T23 |
| T31 | Implement caches prewarm diagnostics and resource views | Operations subagent | T15, T28, T29, T33 |
| T32 | Replace the independent verification worker | Verifier subagent | U02, T13, T20, T10 |
| T33 | Integrate the managed engine capability into Jackin | Companion adapter subagent | U08, T13, T23, T27 |
| T34 | Complete workflow and product behavior coverage | Operations subagent | T32, T26, T27, T29, T30, T31 |
| T35 | Qualify local containers and remote container gateways | Qualification subagent | T29, T30, T31, T34, U03 |
| T36 | Qualify isolated VM profiles and engine domains | Qualification subagent | T29, T30, T31, T33, T34, U03 |
| T37 | Qualify the Kubernetes deployment tuple | Qualification subagent | T29, T30, T31, T34, U03 |
| T38 | Run cross-area failure security and resource tests | Qualification subagent | T35, T36, T37, T32, T33 |
| T39 | Delete all obsolete implementation and rebuild packaging | Integration subagent | P01, T31, T34, T35, T36, T37, T38 |
| T40 | Approve the full v2 release candidate | Independent review subagent | T39 |

## Detailed task cards

### P00 — Freeze the work area and source evidence

**Owner:** Integration subagent.

**Dependencies:** None.

**Source basis:** SPEC L13-L24; L493-L509.

#### Write scope

- docs/openshell-v2/evidence/source-lock.json
- Local task worktrees and evidence directories

#### Required inputs

- SPEC.md and SOURCE-AUDIT.md.
- The current Jackin checkout and its existing changes.

**Task test directory:** `tests/v2/tasks/P00/`.

#### Procedure

1. Record the current commit, branch, dirty files, submodules, and tool versions.
2. Compare the checkout with the source baseline in C01.
3. Record each difference before you use the baseline inventory.
4. Create a v2 worktree without changing the original checkout.
5. Copy this execution pack into that worktree.
6. Verify the SPEC.md checksum against SOURCE-AUDIT.md.
7. Create separate work areas for Jackin and OpenShell changes.
8. Record actual subagent, model, and effort capabilities.
9. Create the task board with all tasks marked NOT_STARTED.
10. Record authorized disposable targets, registry destinations, and credential references.

#### Pass conditions

- The original checkout has the same uncommitted content after setup.
- The source lock names immutable commits and the specification checksum.
- No test target points to an unapproved production environment.
- No missing tool or unavailable subagent appears as an executed capability.

**Acceptance cases:** A01.

#### Removal and replacement

- Remove nothing from the original checkout or existing runtime resources.

#### Required evidence

- `evidence/openshell-v2/P00/implementation.json`
- `evidence/openshell-v2/P00/review.json`
- `evidence/openshell-v2/P00/test-results.json`
- `evidence/openshell-v2/P00/commands.jsonl`
- `evidence/openshell-v2/P00/source-map.json`
- `evidence/openshell-v2/P00/diff.patch`

#### Stop conditions

- Block code changes when the source baseline cannot be identified.
- Block remote effects when their environment or authorization is unknown.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### P01 — Inventory every old implementation path

**Owner:** Source audit subagent.

**Dependencies:** P00.

**Source basis:** SPEC L51-L64; L156; L375-L383; L503-L509.

#### Write scope

- docs/openshell-v2/evidence/inventory/
- docs/openshell-v2/evidence/legacy-inventory.json
- docs/openshell-v2/evidence/feature-register.json

#### Required inputs

- Current Cargo manifests and all production source.
- Current tests, scripts, role assets, documentation, packaging, and generated CI.
- The source findings in SOURCE-AUDIT.md.

**Task test directory:** `tests/v2/tasks/P01/`.

#### Procedure

1. Dispatch the seven read-only reviews in SUBAGENTS.md.
2. List every direct and indirect runtime call with its caller.
3. Include multiline commands, command builders, shell scripts, FFI, and inactive code.
4. Classify each record as DELETE, REWRITE, PORT_PURE, or BUILD_ONLY.
5. Record the resource owner and trust boundary for each effect.
6. Map each current user journey to its v2 task and acceptance cases.
7. Keep a separate list of old behaviors that the specification explicitly removes.
8. Compare the current source with the older experiment inventory.
9. Mark historical paths that moved or no longer exist.
10. Have an independent auditor trace callers and check inventory completeness.
11. Freeze the inventory before deletion work starts.

#### Pass conditions

- The inventory includes version.rs Docker probes and verify.rs Docker workers.
- The inventory includes every runtime-prefix crate and Capsule privilege path.
- Each retained behavior has a replacement task and a test case.
- Each removal has an explicit reason and final-audit rule.
- No directory-only entry replaces a call-site record.

**Acceptance cases:** A02, A03.

#### Removal and replacement

- Do not preserve obsolete behavior because an old test asserts it.
- Do not delete code during this read-only task.

#### Required evidence

- `evidence/openshell-v2/P01/implementation.json`
- `evidence/openshell-v2/P01/review.json`
- `evidence/openshell-v2/P01/test-results.json`
- `evidence/openshell-v2/P01/commands.jsonl`
- `evidence/openshell-v2/P01/source-map.json`
- `evidence/openshell-v2/P01/diff.patch`

#### Stop conditions

- Block inventory approval when an executable path has no ownership classification.
- Block a behavior removal that has no specification basis or planner decision.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### P02 — Verify the upstream API and compatibility tuple

**Owner:** Upstream API subagent.

**Dependencies:** P00.

**Source basis:** SPEC L24; L252-L278; L413-L415; L477-L491.

#### Write scope

- docs/openshell-v2/evidence/upstream-contracts.json
- compatibility.lock.toml through the integration subagent

#### Required inputs

- Pinned OpenShell SDK, protobufs, server handlers, driver code, and CLI registration code.
- C01 and the dependency register in SPEC.md.

**Task test directory:** `tests/v2/tasks/P02/`.

#### Procedure

1. Read the actual symbols for each required API and transport operation.
2. Map create, read, list, start, stop, delete, exec, transfer, policy, provider, and forward operations.
3. Record scope fields, request IDs, error codes, replay rules, and identity preconditions.
4. Verify SDK mTLS and each intended authentication mode.
5. Verify gateway registration resolution without global gateway switching.
6. Verify canonical restart behavior and the dedicated Kubernetes topology.
7. Check canonical-name syntax and the role ABI for each proposed target.
8. Identify every missing authoritative observation and mutation precondition.
9. Create U00 through U08 work inputs from actual source paths.
10. Record one coherent candidate tuple with immutable artifact references.
11. Record unavailable credentials and test environments as blockers.

#### Pass conditions

- Each API mapping includes a source path, symbol, commit, and qualification case.
- No invented RPC field or driver feature enters the adapter contract.
- The mTLS distinction between the release and candidate source is recorded.
- Every missing platform contract has an upstream task owner.

**Acceptance cases:** A04, A05, A06.

#### Removal and replacement

- Do not create a permanent custom authentication stack.
- Do not use a moving main branch as a compatibility pin.

#### Required evidence

- `evidence/openshell-v2/P02/implementation.json`
- `evidence/openshell-v2/P02/review.json`
- `evidence/openshell-v2/P02/test-results.json`
- `evidence/openshell-v2/P02/commands.jsonl`
- `evidence/openshell-v2/P02/source-map.json`
- `evidence/openshell-v2/P02/diff.patch`

#### Stop conditions

- Block a target when its tuple cannot be resolved or tested.
- Block production claims for source-only capabilities.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### P03 — Freeze the internal contracts and schemas

**Owner:** Planning review subagent.

**Dependencies:** P01, P02.

**Source basis:** SPEC L68-L111; L148-L190; L336-L369; L402-L463.

#### Write scope

- docs/openshell-v2/contracts/
- crates/core shared schema designs through the integration subagent

#### Required inputs

- The complete specification and CONTRACTS.md.
- P01 feature register and P02 upstream contract map.

**Task test directory:** `tests/v2/tasks/P03/`.

#### Procedure

1. Define every domain entity in the specification table.
2. Freeze typed IDs, receipts, error classes, and operation transitions.
3. Freeze the strict user, role, and project TOML schemas.
4. Freeze the Capsule message schema and limits in C10.
5. Freeze activation fields and their authority sources.
6. Freeze account-domain reuse rules and scope requirements.
7. Freeze materialization and checkpoint manifest formats.
8. Freeze host-action approval fields and replay rules.
9. Write a table for each public command and its JSON result.
10. Record unsupported debugging and profiling without implied capability.
11. Resolve contract conflicts through the planning review subagent before implementation.

#### Pass conditions

- Every product entity and command has one unambiguous definition.
- Different accounts cannot resolve to one shared credential domain.
- Operation unknown outcomes have explicit recovery rules.
- All technical terms use the same names across schemas and task cards.

**Acceptance cases:** None.

#### Removal and replacement

- Remove old backend variants from the new schema designs.
- Do not add a v1 parser, importer, or compatibility field.

#### Required evidence

- `evidence/openshell-v2/P03/implementation.json`
- `evidence/openshell-v2/P03/review.json`
- `evidence/openshell-v2/P03/test-results.json`
- `evidence/openshell-v2/P03/commands.jsonl`
- `evidence/openshell-v2/P03/source-map.json`
- `evidence/openshell-v2/P03/diff.patch`

#### Stop conditions

- Block dependent implementation when a shared contract is unresolved.
- Do not let an implementation worker invent a security-sensitive schema field.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### P04 — Build the evidence and acceptance harness

**Owner:** Qualification subagent.

**Dependencies:** P03.

**Source basis:** SPEC L346-L354; L471-L491; L509.

#### Write scope

- crates/tools/jackin-xtask/src/v2/
- tests/v2/
- docs/openshell-v2/evidence/test-manifest.json

#### Required inputs

- ACCEPTANCE.md and the frozen schemas.
- The current test toolchain and CI generator inventory.

**Task test directory:** `tests/v2/tasks/P04/`.

#### Procedure

1. Implement the test command contract in ACCEPTANCE.md.
2. Register every acceptance case with a concrete test entry point.
3. Record target tuples, commands, exit codes, assertions, and evidence hashes.
4. Keep PASS, FAIL, BLOCKED, and NOT_RUN distinct.
5. Fail the full gate when a required case is missing or skipped.
6. Add disposable-resource ownership labels and unique test names.
7. Add cleanup receipts that preserve uncertain failures.
8. Add secret redaction and canary-leak assertions.
9. Add source, dependency, and subprocess-boundary checks.
10. Make the harness usable before full product integration.
11. Implement the fixed release_gate_protocol in the evidence harness.

#### Pass conditions

- An omitted case makes the full gate fail.
- An unavailable target produces BLOCKED, not PASS.
- A failing assertion makes the command fail even when its subprocess exits zero.
- The harness preserves failing evidence and unrelated resources.
- Preflight cannot approve release. Finalization requires every fixed protocol prerequisite.

**Acceptance cases:** A10, A11, A12.

#### Removal and replacement

- Replace old CI assumptions that equate a command exit with complete qualification.
- Do not remove valid old regression assertions without a mapped replacement.

#### Required evidence

- `evidence/openshell-v2/P04/implementation.json`
- `evidence/openshell-v2/P04/review.json`
- `evidence/openshell-v2/P04/test-results.json`
- `evidence/openshell-v2/P04/commands.jsonl`
- `evidence/openshell-v2/P04/source-map.json`
- `evidence/openshell-v2/P04/diff.patch`

#### Stop conditions

- Block qualification when raw evidence cannot be retained safely.
- Do not record mock execution as a live target test.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### P05 — Create the clean v2 crate graph

**Owner:** Integration subagent.

**Dependencies:** P03, P04.

**Source basis:** SPEC L404-L419; L507-L509.

#### Write scope

- Root Cargo.toml and Cargo.lock
- The fourteen production crate roots in C02
- crates/tools/jackin-xtask and test-support registration

#### Required inputs

- P01 disposition map.
- The frozen dependency graph and the current Rust toolchain.

**Task test directory:** `tests/v2/tasks/P05/`.

#### Procedure

1. Create the production crates listed in C02.
2. Set the product version to 2.0.0-dev until release approval.
3. Remove old runtime crates from the production workspace graph.
4. Keep old source only as temporary deletion input outside the active graph.
5. Create application ports in the domain crate.
6. Wire concrete adapters only in application composition.
7. Add compile-time dependency-boundary tests.
8. Keep the existing qualified toolchain unless compatibility requires a reviewed change.
9. Make unfinished operations return explicit unsupported errors.
10. Do not add placeholder success paths.

#### Pass conditions

- The production graph contains no old runtime backend crate.
- Core code imports no upstream protobuf, Docker, or database types.
- Capsule imports no host credential or builder implementation.
- A clean build fails on any forbidden dependency edge.

**Acceptance cases:** A13, A14.

#### Removal and replacement

- Remove old workspace-member and feature edges.
- Schedule physically retained old source for T39 deletion.

#### Required evidence

- `evidence/openshell-v2/P05/implementation.json`
- `evidence/openshell-v2/P05/review.json`
- `evidence/openshell-v2/P05/test-results.json`
- `evidence/openshell-v2/P05/commands.jsonl`
- `evidence/openshell-v2/P05/source-map.json`
- `evidence/openshell-v2/P05/diff.patch`

#### Stop conditions

- Block an extra production crate until the planner approves its need.
- Do not restore an old dependency merely to make compilation pass.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U00 — Qualify authoritative runtime observations

**Owner:** Upstream API subagent.

**Dependencies:** P02, P03, P04.

**Source basis:** SPEC L150-L176; L246-L248; L350; L415.

#### Write scope

- A separate OpenShell worktree: observation APIs and related tests identified by P02
- Jackin evidence only; no embedded runtime implementation

#### Required inputs

- P02 API map and C06 activation fields.
- OpenShell policy, provider, image, identity, and runtime-status source.

**Task test directory:** `tests/v2/tasks/U00/`.

#### Procedure

1. Test each required activation observation through the scoped public API.
2. Identify its authority source and runtime generation.
3. Check whether ordinary authorized workspace users can obtain the required result.
4. Use an existing authoritative API when it meets the contract.
5. Add a narrow upstream observation contract when required data is absent.
6. Bind each result to the immutable sandbox ID and applicable revision.
7. Keep provider secret bytes outside observation payloads.
8. Reject helper-reported identity as a substitute.
9. Add old-server and unsupported-capability behavior.
10. Update the candidate tuple and upstream change record.

#### Pass conditions

- A submitted policy does not count as loaded policy evidence.
- A helper echo cannot satisfy image or sandbox identity checks.
- Provider readiness refers to the installed binding revision.
- Unsupported observations prevent activation with a specific error.

**Acceptance cases:** A15, A16, A17.

#### Removal and replacement

- Do not add a Jackin-side supervisor or attestation service.
- Do not require administrator credentials merely to hide an API gap.

#### Required evidence

- `evidence/openshell-v2/U00/implementation.json`
- `evidence/openshell-v2/U00/review.json`
- `evidence/openshell-v2/U00/test-results.json`
- `evidence/openshell-v2/U00/commands.jsonl`
- `evidence/openshell-v2/U00/source-map.json`
- `evidence/openshell-v2/U00/upstream-change.json`

#### Stop conditions

- Block activation qualification when an authority field remains unobservable.
- Keep unmerged patches explicit in the compatibility tuple.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U01 — Add server-side mutation identity fences

**Owner:** Upstream API subagent.

**Dependencies:** P02, P03, P04.

**Source basis:** SPEC L348-L354; L484; L491.

#### Write scope

- A separate OpenShell worktree: protobufs, admission, replay, effect handlers, and driver dispatch
- Upstream race and replay tests

#### Required inputs

- All effectful API mappings from P02.
- C05 and historical same-name replacement failures.

**Task test directory:** `tests/v2/tasks/U01/`.

#### Procedure

1. List every effectful sandbox operation, including transfer and service changes.
2. Add expected immutable sandbox identity to each relevant request.
3. Add generation preconditions wherever process incarnation changes the effect target.
4. Choose unused protobuf field numbers from the actual pinned schema.
5. Check authorization before returning resource-sensitive identity details.
6. Validate preconditions at the server effect boundary, not only during a prior read.
7. Carry the immutable binding into asynchronous driver work.
8. Bind replay records to scope, identity, method, request ID, and request digest.
9. Reject the same request ID with a different payload.
10. Make old servers fail safely when the required contract is unavailable.
11. Test delayed requests against same-name replacements for every affected operation.
12. Document creation, start-generation, cancellation, and replay semantics.

#### Pass conditions

- Delayed stop, start, delete, exec, policy, provider, transfer, and service effects cannot reach a replacement.
- Identical retries return the qualified original outcome without duplicate effects.
- A cross-scope replay cannot expose or affect another resource.
- The adapter can distinguish identity change from transport uncertainty.

**Acceptance cases:** A18, A19, A20, A21.

#### Removal and replacement

- Do not treat a delete-only patch as complete fencing.
- Do not claim atomicity from a client read followed by mutation.

#### Required evidence

- `evidence/openshell-v2/U01/implementation.json`
- `evidence/openshell-v2/U01/review.json`
- `evidence/openshell-v2/U01/test-results.json`
- `evidence/openshell-v2/U01/commands.jsonl`
- `evidence/openshell-v2/U01/source-map.json`
- `evidence/openshell-v2/U01/upstream-change.json`

#### Stop conditions

- Block all affected production mutations until the complete contract passes.
- Do not implement a Jackin lock as a substitute for upstream fencing.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U02 — Add strict verification-worker isolation

**Owner:** Upstream worker subagent.

**Dependencies:** P02, P03, P04.

**Source basis:** SPEC L326-L334; L483; L491.

#### Write scope

- A separate OpenShell worktree: workload configuration, selected drivers, and worker tests

#### Required inputs

- C15 and the historical metadata-mutation counterexample.
- Actual workload-root and scratch configuration in the pinned drivers.

**Task test directory:** `tests/v2/tasks/U02/`.

#### Procedure

1. Expose a real workload read-only-root contract in OpenShell.
2. Define bounded writable control, temporary, and scratch paths.
3. Make candidate input and trusted checks immutable through the qualified runtime boundary.
4. Enforce memory, CPU, PID, disk, output, and deadline limits.
5. Block ordinary network access for the worker profile.
6. Provide authoritative command completion and infrastructure-failure outcomes.
7. Keep supervisor write needs separate from worker write permissions.
8. Test metadata operations as well as content writes.
9. Test child and grandchild cleanup after cancellation and timeout.
10. Qualify each driver that advertises strict worker support.

#### Pass conditions

- chmod, chown, rename, unlink, hardlink, xattr, and content changes fail outside approved scratch.
- Scratch overflow and process exhaustion remain bounded.
- No account, provider, host socket, or ordinary network path reaches the worker.
- An OOM or uncertain completion cannot become a passing verdict.

**Acceptance cases:** A22, A23, A24, A25.

#### Removal and replacement

- Do not replace read-only root with image chmod commands or Landlock content rules.
- Do not retain the direct Docker verification path.

#### Required evidence

- `evidence/openshell-v2/U02/implementation.json`
- `evidence/openshell-v2/U02/review.json`
- `evidence/openshell-v2/U02/test-results.json`
- `evidence/openshell-v2/U02/commands.jsonl`
- `evidence/openshell-v2/U02/source-map.json`
- `evidence/openshell-v2/U02/upstream-change.json`

#### Stop conditions

- Block strict verification on every unqualified driver.
- Do not describe a weaker development test domain as an independent verifier.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U03 — Qualify rebuilt executable network identity

**Owner:** Upstream worker subagent.

**Dependencies:** P02, P03, P04.

**Source basis:** SPEC L485-L486; L491.

#### Write scope

- A separate OpenShell worktree: executable identity and policy lifecycle code identified by P02
- Rebuild regression fixtures

#### Required inputs

- Historical same-path rebuilt-binary failures.
- Native policy and provider executable-identity rules.

**Task test directory:** `tests/v2/tasks/U03/`.

#### Procedure

1. Reproduce ordinary Cargo and nextest network tests on the candidate tuple.
2. Rebuild the executable twice at the same path with different content.
3. Record executable identity, policy revision, and each connection decision.
4. Implement or qualify an explicit safe identity-update contract.
5. Keep destination limits and provider authority separate from mutable executable paths.
6. Prevent unauthorized replacements from acquiring a protected provider grant.
7. Test concurrent rebuild and connect operations.
8. Test legitimate development traffic and hostile replacement traffic.
9. Supply the native rebuild fixture to U08 for its Testcontainers run.
10. Keep debugging and profiling unsupported unless separately qualified.

#### Pass conditions

- Normal approved rebuild cycles succeed without manual path changes.
- A changed executable cannot inherit unrelated credential authority.
- The solution does not disable identity checks globally.
- Docker-executed tests do not substitute for the native rebuild regression.

**Acceptance cases:** A26, A27.

#### Removal and replacement

- Do not add a blanket allow-all rule for compilers or interpreters.
- Do not hide the defect with an alternate executable path.

#### Required evidence

- `evidence/openshell-v2/U03/implementation.json`
- `evidence/openshell-v2/U03/review.json`
- `evidence/openshell-v2/U03/test-results.json`
- `evidence/openshell-v2/U03/commands.jsonl`
- `evidence/openshell-v2/U03/source-map.json`
- `evidence/openshell-v2/U03/upstream-change.json`

#### Stop conditions

- Block the normal development claim until repeated native tests pass.
- Request planner review when the safe identity lifecycle needs a new contract.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U04 — Freeze the upstream companion API and ownership

**Owner:** Companion lifecycle subagent.

**Dependencies:** P02, P03, P04.

**Source basis:** SPEC L286-L324; L482.

#### Write scope

- A separate OpenShell worktree: companion API proposal and contract tests
- Upstream companion design record

#### Required inputs

- C14 and SPEC optional Docker capability section.
- The actual VM lifecycle extension points from P02.

**Task test directory:** `tests/v2/tasks/U04/`.

#### Procedure

1. Define one dedicated engine VM for each admitted execution domain.
2. Define the default-off admission field and capability revision.
3. Bind ownership to workspace, immutable sandbox ID, and runtime generation.
4. Define authenticated endpoint issuance and revocation.
5. Define project projection, ports, network limits, and aggregate budgets.
6. Define create, start, stop, recreate, delete, and orphan transitions.
7. Define partial-provision and uncertain-cleanup states.
8. Define capability discovery for unsupported gateways.
9. Assign exact upstream modules to U05, U06, and U07.
10. Obtain independent authority and lifecycle review before implementation.

#### Pass conditions

- Disabled capability creates no engine-related resource or secret.
- No companion belongs to a friendly name alone.
- The API does not expose the physical host daemon.
- The contract identifies every resource owner and deletion receipt.

**Acceptance cases:** None.

#### Removal and replacement

- Do not reuse the Jackin DinD manager as the companion owner.
- Do not introduce shared engine pools.

#### Required evidence

- `evidence/openshell-v2/U04/implementation.json`
- `evidence/openshell-v2/U04/review.json`
- `evidence/openshell-v2/U04/test-results.json`
- `evidence/openshell-v2/U04/commands.jsonl`
- `evidence/openshell-v2/U04/source-map.json`
- `evidence/openshell-v2/U04/upstream-change.json`

#### Stop conditions

- Block companion implementation until its ownership and security contract is complete.
- Do not present proposed API fields as existing OpenShell fields.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U05 — Implement companion VM lifecycle and cleanup

**Owner:** Companion lifecycle subagent.

**Dependencies:** U04, U01.

**Source basis:** SPEC L290-L324.

#### Write scope

- A separate OpenShell worktree: companion lifecycle modules assigned by U04
- Shared driver integration through the upstream integration owner

#### Required inputs

- The approved companion state machine and identity-fencing contract.
- VM lifecycle source and failure fixtures.

**Task test directory:** `tests/v2/tasks/U05/`.

#### Procedure

1. Create a disposable engine VM only after explicit capability admission.
2. Persist parent identity and child resource ownership before external effects.
3. Give the companion separate compute and private engine storage.
4. Keep the agent workload in its ordinary sandbox.
5. Implement create, start, stop, delete, and orphan reconciliation.
6. Issue new authenticated access for each runtime generation.
7. Revoke old access before stop completes.
8. Create a new engine after sandbox recreation.
9. Retain only the domain data allowed by the contract.
10. Resume interrupted cleanup from durable ownership records.
11. Keep deletion pending until destruction and revocation are confirmed.

#### Pass conditions

- A killed parent cannot leave an unmanaged live engine.
- A provisioning crash is recoverable without duplicate ownership.
- Old-generation endpoints fail after restart.
- Deleting a same-name replacement cannot remove the original domain's unrelated data.

**Acceptance cases:** A29, A30, A31.

#### Removal and replacement

- Do not run a rootful daemon beside the agent's trusted runtime.
- Do not create a Jackin companion supervisor.

#### Required evidence

- `evidence/openshell-v2/U05/implementation.json`
- `evidence/openshell-v2/U05/review.json`
- `evidence/openshell-v2/U05/test-results.json`
- `evidence/openshell-v2/U05/commands.jsonl`
- `evidence/openshell-v2/U05/source-map.json`
- `evidence/openshell-v2/U05/upstream-change.json`

#### Stop conditions

- Block engine support while cleanup can return false success.
- Do not bypass ownership records during recovery.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U06 — Implement confined companion project projection

**Owner:** Companion data subagent.

**Dependencies:** U04.

**Source basis:** SPEC L308; L318-L320.

#### Write scope

- A separate OpenShell worktree: companion data-projection modules assigned by U04

#### Required inputs

- The approved companion API and project-data contract.
- The runtime's actual VM transport and filesystem facilities.

**Task test directory:** `tests/v2/tasks/U06/`.

#### Procedure

1. Expose only the approved project at the contract's fixed daemon-side path.
2. Keep account state and OpenShell control files outside the projection.
3. Implement file create, edit, rename, delete, and watch behavior.
4. Define concurrent writer behavior and result ownership.
5. Reject escaping symlinks, hardlinks, path traversal, and unsupported special files.
6. Test local projection operations, ownership, and path rejection in the companion data module.
7. Record the absolute path and bind requirements for complete U08 qualification.
8. Propagate projection errors without a wider mount fallback.
9. Record data retention and deletion independently from engine cache state.

#### Pass conditions

- The projection exposes only approved project data to the companion data plane.
- File create, edit, rename, delete, and watch behavior matches the projection contract.
- Escaping links, hardlinks, traversal, and unsupported special files remain confined.
- Projection errors never trigger a wider host-mount fallback.

**Acceptance cases:** None.

#### Removal and replacement

- Do not use raw host virtiofs sharing as isolated projection.
- Do not broaden a host mount to solve a mount-budget limit.

#### Required evidence

- `evidence/openshell-v2/U06/implementation.json`
- `evidence/openshell-v2/U06/review.json`
- `evidence/openshell-v2/U06/test-results.json`
- `evidence/openshell-v2/U06/commands.jsonl`
- `evidence/openshell-v2/U06/source-map.json`
- `evidence/openshell-v2/U06/upstream-change.json`

#### Stop conditions

- Block Compose and Testcontainers bind claims until real projection tests pass.
- Reject unsupported link or file-watch behavior explicitly.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U07 — Implement companion network ports and budgets

**Owner:** Companion network subagent.

**Dependencies:** U04.

**Source basis:** SPEC L304-L318; L320-L322.

#### Write scope

- A separate OpenShell worktree: companion network and resource modules assigned by U04

#### Required inputs

- The approved companion API and resource budgets.
- VM networking, endpoint transport, and upstream enforcement code.

**Task test directory:** `tests/v2/tasks/U07/`.

#### Procedure

1. Provide authenticated Docker API access only to the owning domain.
2. Support build, exec, logs, events, and long-lived streaming operations.
3. Enforce egress outside the daemon and every child container.
4. Support online image pulls without a physical host socket.
5. Implement dynamic published ports and service-host discovery.
6. Implement authenticated callback routes with explicit lifetimes.
7. Enforce aggregate CPU, memory, PID, disk, and network budgets.
8. Handle port collision, exhaustion, release, and reuse.
9. Keep policy-update stream interruption visible.
10. Revoke routes and credentials during companion stop and delete.
11. Add focused API, egress, callback, budget, and revocation tests for each local component.

#### Pass conditions

- Local API tests reject credentials that belong to another domain.
- Network-policy tests cover daemon, build-step, and child-container egress paths.
- Route-manager tests cover owners, collisions, exhaustion, release, and reuse.
- Callback and credential tests cover revocation during stop and delete.
- Aggregate-budget tests account for daemon and child workloads together.
- Policy-stream interruption remains visible to the engine client.
- A guest-init script alone does not satisfy local network checks.

**Acceptance cases:** None.

#### Removal and replacement

- Do not inherit agent binary policy as an assumed child-container boundary.
- Do not add a general public port forward.

#### Required evidence

- `evidence/openshell-v2/U07/implementation.json`
- `evidence/openshell-v2/U07/review.json`
- `evidence/openshell-v2/U07/test-results.json`
- `evidence/openshell-v2/U07/commands.jsonl`
- `evidence/openshell-v2/U07/source-map.json`
- `evidence/openshell-v2/U07/upstream-change.json`

#### Stop conditions

- Block online Docker claims when the VM has no qualified network route.
- Block engine support when child resource limits remain unenforced.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### U08 — Qualify the complete managed companion

**Owner:** Qualification subagent.

**Dependencies:** U05, U06, U07, U00, U03.

**Source basis:** SPEC L298-L324; L491; L503-L509.

#### Write scope

- Upstream companion integration tests
- Jackin evidence/openshell-v2/U08/

#### Required inputs

- The complete patched OpenShell tuple.
- Real Docker, Compose, and Testcontainers fixtures.

**Task test directory:** `tests/v2/tasks/U08/`.

#### Procedure

1. Create two fresh domains with separate companions.
2. Test default-off behavior on a third domain.
3. Run online pull, build, run, exec, logs, and events.
4. Run Compose with live project binds and dynamic ports.
5. Test confined project access from the daemon and privileged Docker children.
6. Edit, rename, create, and delete files from both sides. Verify file watches.
7. Probe escaping links, traversal paths, and unsupported special files through child containers.
8. Run real Testcontainers traffic after two ordinary source edits.
9. Test callbacks, port collisions, exhaustion, release, reuse, and simultaneous domains.
10. Attempt cross-domain access and unapproved egress.
11. Exceed combined CPU, memory, PID, disk, and network budgets across daemon and child containers.
12. Kill the gateway, parent, and provisioning process at defined transition points.
13. Verify stop, restart, recreation, deletion, route revocation, credential revocation, and orphan cleanup.
14. Have a different test agent repeat the complete journey from fresh resources.
15. Publish the qualified capability and exact tuple.

#### Pass conditions

- No external fixture supplies lifecycle ownership missing from OpenShell.
- All child resources have confirmed cleanup receipts.
- The disabled profile leaves no engine endpoint, credential, or prewarm artifact.
- The independent repeat matches the required assertions.
- Compose sees live edits at the agreed absolute project path.
- Docker children cannot widen the project projection through path attacks.
- A streamed build context does not satisfy bind compatibility.
- Required API behavior works without a host socket. Stream cancellation preserves ownership.
- Daemon, build-step, and child-container egress follow the outer policy.
- Dynamic ports and callbacks stay with their owner. Revoked routes cannot be reused.
- Aggregate domain budgets remain enforced across the daemon and all child containers.

**Acceptance cases:** A28, A30, A31, A32, A33, A34, A35, A36, A37, A38, A39.

#### Removal and replacement

- Remove experimental external-engine substitutes from supported product tests.
- Keep historical failure records as evidence.

#### Required evidence

- `evidence/openshell-v2/U08/implementation.json`
- `evidence/openshell-v2/U08/review.json`
- `evidence/openshell-v2/U08/test-results.json`
- `evidence/openshell-v2/U08/commands.jsonl`
- `evidence/openshell-v2/U08/source-map.json`
- `evidence/openshell-v2/U08/upstream-change.json`

#### Stop conditions

- Block the managed-engine release gate after any missing lifecycle or isolation assertion.
- Do not reduce the contract to build-only support.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T10 — Implement v2 state and operation recovery

**Owner:** State subagent.

**Dependencies:** P04, P05.

**Source basis:** SPEC L87-L93; L336-L354; L413-L419.

#### Write scope

- crates/adapters/jackin-state/
- crates/services/jackin-control/src/operations/
- Related state tests

#### Required inputs

- C03 through C06 and the frozen entity schemas.
- The complete failure-phase table from P03.

**Task test directory:** `tests/v2/tasks/T10/`.

#### Procedure

1. Create a fresh v2 SQLite schema for all durable entities and receipts.
2. Add one write coordinator and explicit transaction boundaries.
3. Store account references separately from secret material.
4. Persist operation inputs and request IDs before each external effect.
5. Record partial receipts and OutcomeUnknown without guessed success.
6. Implement recovery for every crash point in the operation table.
7. Keep runtime and Capsule observations separate from operation state.
8. Add owner epochs for local leases and control-service recovery.
9. Keep canonical-name tombstones and immutable binding history.
10. Add v2-only database version checks and backup tests.

#### Pass conditions

- A crash before an effect does not lose its request identity.
- A crash after an effect does not cause an automatic duplicate command.
- No database write transaction waits for a network call.
- A v1 database produces a fresh-setup error, not an import.

**Acceptance cases:** A09, A40, A42.

#### Removal and replacement

- Replace old instance manifests and backend-specific resource records.
- Do not copy the old workflow database format as the v2 schema.

#### Required evidence

- `evidence/openshell-v2/T10/implementation.json`
- `evidence/openshell-v2/T10/review.json`
- `evidence/openshell-v2/T10/test-results.json`
- `evidence/openshell-v2/T10/commands.jsonl`
- `evidence/openshell-v2/T10/source-map.json`
- `evidence/openshell-v2/T10/diff.patch`

#### Stop conditions

- Block operations whose recovery disposition is undefined.
- Reject secret bytes in ordinary serialized records.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T11 — Implement the per-user host control service

**Owner:** State subagent.

**Dependencies:** T10.

**Source basis:** SPEC L66; L176; L180-L188; L346.

#### Write scope

- crates/services/jackin-control/src/ipc/
- crates/apps/jackin/src/control/
- Host IPC tests

#### Required inputs

- C05 and the per-user service contract.
- The host-platform path and peer-authentication APIs.

**Task test directory:** `tests/v2/tasks/T11/`.

#### Procedure

1. Start the control service on demand from the CLI.
2. Use a permission-protected Unix socket and operating-system peer authentication.
3. Reject unsafe socket directories and symlink substitutions.
4. Give CLI and Console one application API.
5. Expose operation IDs and inspection without duplicating workflow logic.
6. Serialize local mutations through the write coordinator.
7. Keep remote Capsule lifetime independent from the host process.
8. Recover journaled operations after a host-service restart.
9. Report host-dependent approval unavailability without ending remote sessions.

#### Pass conditions

- Another local user cannot control the service.
- A forged remote helper message does not become a local peer credential.
- Killing the host service leaves the remote Capsule and PTYs alive.
- Restart recovers operation state without replaying an uncertain effect.

**Acceptance cases:** A43.

#### Removal and replacement

- Replace old host-daemon runtime execution and process-relay ownership.
- Do not create a required team server or fleet scheduler.

#### Required evidence

- `evidence/openshell-v2/T11/implementation.json`
- `evidence/openshell-v2/T11/review.json`
- `evidence/openshell-v2/T11/test-results.json`
- `evidence/openshell-v2/T11/commands.jsonl`
- `evidence/openshell-v2/T11/source-map.json`
- `evidence/openshell-v2/T11/diff.patch`

#### Stop conditions

- Block host effects when peer identity or action correlation is absent.
- Do not use remote helper identity as local IPC authentication.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T12 — Implement native OpenShell connection handling

**Owner:** Runtime adapter subagent.

**Dependencies:** P02, P05.

**Source basis:** SPEC L55; L252; L413-L415.

#### Write scope

- crates/adapters/jackin-openshell/src/connection/
- crates/adapters/jackin-openshell/src/registration/
- Transport tests

#### Required inputs

- The qualified SDK and registration contracts from P02.
- C01 and C13.

**Task test directory:** `tests/v2/tasks/T12/`.

#### Procedure

1. Resolve existing upstream gateway registrations without copying their credential store.
2. Build scoped clients with verified server identity and supported authentication.
3. Use upstream Rust SDK transport from the qualified candidate.
4. Use raw RPC and streaming methods only where the SDK helpers are insufficient.
5. Pass gateway and workspace explicitly with every adapter call.
6. Keep local target identity separate from registration alias and TLS identity.
7. Implement bounded connection errors and supported token refresh.
8. Redact credentials from Debug, logs, and evidence.
9. Test simultaneous requests to two gateways with duplicate sandbox names.
10. Keep version negotiation inside this adapter.

#### Pass conditions

- Wrong certificates and wrong server identities fail closed.
- Concurrent calls cannot switch each other's target.
- The production adapter never sets insecure TLS verification.
- An expired credential produces the correct authentication outcome.

**Acceptance cases:** A05, A06, A46, A47.

#### Removal and replacement

- Remove old Docker and Apple connection clients from production composition.
- Do not create a parallel gateway credential database.

#### Required evidence

- `evidence/openshell-v2/T12/implementation.json`
- `evidence/openshell-v2/T12/review.json`
- `evidence/openshell-v2/T12/test-results.json`
- `evidence/openshell-v2/T12/commands.jsonl`
- `evidence/openshell-v2/T12/source-map.json`
- `evidence/openshell-v2/T12/diff.patch`

#### Stop conditions

- Block an authentication mode that the tuple does not qualify.
- Do not guess SDK behavior from stale documentation.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T13 — Implement scoped runtime operations and observations

**Owner:** Runtime adapter subagent.

**Dependencies:** T12, T10, U00, U01.

**Source basis:** SPEC L51-L64; L244-L248; L348-L354; L413-L417.

#### Write scope

- crates/adapters/jackin-openshell/src/sandboxes/
- crates/adapters/jackin-openshell/src/policies/
- crates/adapters/jackin-openshell/src/providers/
- crates/adapters/jackin-openshell/src/transfer/
- Adapter contract tests

#### Required inputs

- P02 actual API map, U00 observations, and U01 fences.
- C05, C06, and C13.

**Task test directory:** `tests/v2/tasks/T13/`.

#### Procedure

1. Implement create, inspect, list, start, stop, delete, and effectful exec through OpenShell.
2. Implement approved transfer, policy, provider, and service operations.
3. Attach stored request IDs and immutable preconditions to each effect.
4. Verify returned bindings before publishing receipts.
5. Map upstream errors to the frozen Jackin error classes.
6. Preserve authorization masking, incomplete pagination, and uncertain outcomes.
7. Collect authoritative policy, provider, image, and generation observations.
8. Prevent effect retries that lack a qualified replay contract.
9. Expose capability rejection before launch.
10. Keep infrastructure reconciliation upstream.

#### Pass conditions

- Every effect uses its explicit target, scope, identity, and required generation.
- Lost responses do not cause duplicate commands.
- A generic NOT_FOUND cannot delete retained application data.
- An outage cannot create a replacement sandbox.

**Acceptance cases:** A15, A16, A17, A18, A19, A20, A21, A48.

#### Removal and replacement

- Replace direct Docker run, exec, attach, inspect, stop, and remove consumers.
- Do not retain a second runtime state machine.

#### Required evidence

- `evidence/openshell-v2/T13/implementation.json`
- `evidence/openshell-v2/T13/review.json`
- `evidence/openshell-v2/T13/test-results.json`
- `evidence/openshell-v2/T13/commands.jsonl`
- `evidence/openshell-v2/T13/source-map.json`
- `evidence/openshell-v2/T13/diff.patch`

#### Stop conditions

- Block mutations against a server without the required fence contract.
- Block activation when authoritative observations are incomplete.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T14 — Implement reproducible role resolution

**Owner:** Role-image subagent.

**Dependencies:** P03, P05.

**Source basis:** SPEC L68-L111; L113-L127; L146; L402-L463; L475.

#### Write scope

- crates/services/jackin-roles/src/resolve/
- crates/core/jackin-config user, role, and project parsing and strict validation; shared schema-definition edits go through the integration subagent
- Role resolution tests

#### Required inputs

- Frozen strict user, role, and project schemas.
- RoleRecipe and ResolvedRole definitions.
- P01 role, agent, tool, plugin, and hook inventory.

**Task test directory:** `tests/v2/tasks/T14/`.

#### Procedure

1. Parse strict v2 user, role, and project schemas.
2. Reject unknown fields, unsupported versions, ambiguous IDs, and old-schema fallbacks.
3. Resolve every declared agent, tool, plugin, hook, and base image to an explicit input.
4. Keep the full selectable agent set in the resolved role.
5. Reject undeclared or unavailable agent selections.
6. Compute the effective build context with actual ignore rules.
7. Include every required C07 input in the cache identity.
8. Separate secret references from nonsecret build identity.
9. Define cache behavior for secret-dependent output.
10. Record provenance and an executable inventory.
11. Require explicit role trust before build-side source or secret exposure.

#### Pass conditions

- Changing any declared agent or plugin input changes the cache identity.
- An Amp selection cannot reuse an image without Amp.
- An unknown version cannot silently match a reproducible cache entry.
- Ignoring a file affects the key exactly as the build context requires.
- Unknown fields and unsupported versions in user, role, and project schemas fail.
- Ambiguous IDs fail without a first-match fallback.

**Acceptance cases:** A07, A49.

#### Removal and replacement

- Replace old selected-agent cache shortcuts and runtime-role labels.
- Do not retain implicit fallback installers.

#### Required evidence

- `evidence/openshell-v2/T14/implementation.json`
- `evidence/openshell-v2/T14/review.json`
- `evidence/openshell-v2/T14/test-results.json`
- `evidence/openshell-v2/T14/commands.jsonl`
- `evidence/openshell-v2/T14/source-map.json`
- `evidence/openshell-v2/T14/diff.patch`

#### Stop conditions

- Block the role when required inputs cannot be resolved or verified.
- Do not claim an advertised executable is installed without evidence.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T15 — Implement explicit Buildx and image delivery

**Owner:** Role-image subagent.

**Dependencies:** T14, T10, P02.

**Source basis:** SPEC L119-L127; L371; L475.

#### Write scope

- crates/adapters/jackin-buildx/
- crates/services/jackin-roles/src/build/
- crates/services/jackin-roles/src/delivery/
- Build and registry tests

#### Required inputs

- ResolvedRole build inputs and configured builders.
- Actual local-engine and registry contracts from P02.

**Task test directory:** `tests/v2/tasks/T15/`.

#### Procedure

1. Run builds through an explicit builder endpoint and context.
2. Remove ambient Docker routing variables from the build subprocess environment.
3. Pass arguments as argv without host shell interpolation.
4. Use supported secret mounts without ordinary-log disclosure.
5. Record local image IDs, OCI manifests, and OCI indexes as distinct values.
6. Use a same-engine fast path only after endpoint identity matches.
7. Push remote artifacts to the approved registry by immutable reference.
8. Record the selected platform manifest and scoped pull authorization.
9. Verify delivery without a mutable-tag assumption.
10. Keep build failure separate from gateway and management availability.

#### Pass conditions

- An ambient DOCKER_HOST cannot redirect source or build secrets.
- A remote gateway cannot receive a local image ID as its pull reference.
- Wrong architecture and missing registry authorization fail before activation.
- An unavailable separate builder does not disable status or stop operations.
- Secret bytes stay out of build logs, and secret-dependent output follows its declared cache policy.

**Acceptance cases:** A51, A52, A53, A54.

#### Removal and replacement

- Replace old image transport tied to role-container discovery.
- Keep only reviewed build, image-inspect, delivery, and cache operations.

#### Required evidence

- `evidence/openshell-v2/T15/implementation.json`
- `evidence/openshell-v2/T15/review.json`
- `evidence/openshell-v2/T15/test-results.json`
- `evidence/openshell-v2/T15/commands.jsonl`
- `evidence/openshell-v2/T15/source-map.json`
- `evidence/openshell-v2/T15/diff.patch`

#### Stop conditions

- Block a build without a resolved builder identity.
- Block activation when the executed artifact cannot match the recorded build.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T16 — Build and validate portable role images

**Owner:** Role-image subagent.

**Dependencies:** T15, T13, T18.

**Source basis:** SPEC L115-L146; L150-L156.

#### Write scope

- Role image recipes and docker/construct replacement assets
- crates/services/jackin-roles/src/validate/
- Role ABI tests

#### Required inputs

- C08, the new Capsule artifact, and P01 role journeys.
- The qualified target identity and entrypoint contracts.

**Task test directory:** `tests/v2/tasks/T16/`.

#### Procedure

1. Build ordinary OCI images with the complete declared agent set.
2. Install the new Capsule at the fixed C08 path.
3. Create the named nonroot account and explicit writable directory layout.
4. Set HOME, XDG, tool paths, and nonsecret environment explicitly.
5. Replace privileged startup with an explicit unprivileged command.
6. Validate every advertised executable through metadata or an OpenShell probe.
7. Validate actual target UID, workdir, syscall, and filesystem behavior.
8. Test agent switching without an image rebuild or missing binary.
9. Repair required installer defects instead of hiding them behind a fallback.
10. Record role and Capsule compatibility in ResolvedRole.

#### Pass conditions

- The role starts without root, sudo, chown, setuid, or firewall setup.
- Every advertised agent reports the recorded version and starts its declared route.
- OpenShell ENTRYPOINT behavior cannot skip required setup silently.
- The canonical Capsule starts with zero sessions.
- An image that lacks a declared agent executable fails validation.

**Acceptance cases:** A50, A56, A57, A58.

#### Removal and replacement

- Delete the old privileged Capsule entrypoint and runtime provisioning assets.
- Delete direct Docker version probes, including runtime-image version.rs.

#### Required evidence

- `evidence/openshell-v2/T16/implementation.json`
- `evidence/openshell-v2/T16/review.json`
- `evidence/openshell-v2/T16/test-results.json`
- `evidence/openshell-v2/T16/commands.jsonl`
- `evidence/openshell-v2/T16/source-map.json`
- `evidence/openshell-v2/T16/diff.patch`

#### Stop conditions

- Block a role that requires an unsupported identity or syscall.
- Do not replace a failed real-agent route with a version-only success claim.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T17 — Implement the bounded Capsule protocol

**Owner:** Capsule subagent.

**Dependencies:** P04, P05.

**Source basis:** SPEC L158-L176; L408.

#### Write scope

- crates/core/jackin-session-protocol/ implementation excluding shared post-P03 wire-schema definitions
- Protocol property tests and fuzz targets

#### Required inputs

- Frozen C10 schema, limits, and compatibility rules.
- Reviewed old terminal and protocol regression fixtures.

**Task test directory:** `tests/v2/tasks/T17/`.

#### Procedure

1. Implement the four-byte length prefix and bounded CBOR payload.
2. Implement Hello, Inspect, StartSession, StopSession, Attach, Detach, Resize, Input, and Snapshot messages.
3. Implement output, exit, replay-gap, and advisory-status events.
4. Carry boot, session, attachment, operation, and input-generation identifiers where applicable.
5. Reject incompatible major versions before side effects.
6. Reject oversized, truncated, deeply nested, and invalid frames before unsafe allocation.
7. Keep protocol bytes separate from diagnostic text.
8. Add stable golden frames and round-trip property tests.
9. Fuzz both decoder directions and arithmetic boundaries.
10. Do not import host authority into the protocol crate.
11. Submit post-P03 wire-schema changes to integration as patch requests with exact change, dependency reason, and tests.

#### Pass conditions

- A malformed length cannot cause unbounded allocation.
- Diagnostics cannot be parsed as control messages.
- The same technical entity uses the same field meaning in both directions.
- Unknown required capabilities fail before session creation.

**Acceptance cases:** A59, A60, A61.

#### Removal and replacement

- Replace the old Capsule wire protocol and Linux peer-PID proof fields.
- Do not retain a legacy protocol negotiation path.

#### Required evidence

- `evidence/openshell-v2/T17/implementation.json`
- `evidence/openshell-v2/T17/review.json`
- `evidence/openshell-v2/T17/test-results.json`
- `evidence/openshell-v2/T17/commands.jsonl`
- `evidence/openshell-v2/T17/source-map.json`
- `evidence/openshell-v2/T17/diff.patch`

#### Stop conditions

- Block transport integration until framing and negative tests pass.
- Do not silently accept incompatible protocol versions.
- Do not edit shared wire-schema definitions after P03; submit a patch request to integration.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T18 — Rewrite Capsule as an unprivileged resident service

**Owner:** Capsule subagent.

**Dependencies:** T17.

**Source basis:** SPEC L148-L156; L168-L190.

#### Write scope

- crates/apps/jackin-capsule/src/service/
- crates/apps/jackin-capsule/src/pty/
- crates/apps/jackin-capsule/src/main.rs
- Capsule process tests

#### Required inputs

- C08 through C10.
- P01 Capsule inventory and reviewed terminal-parser behavior.

**Task test directory:** `tests/v2/tasks/T18/`.

#### Procedure

1. Implement the serve command without root or PID1 requirements.
2. Create a fresh boot ID and an empty session inventory at startup.
3. Create inner PTYs only after an explicit StartSession request.
4. Deduplicate identical StartSession operations within the current boot.
5. Reject operation-ID reuse with different command inputs.
6. Manage process groups, terminal size, signals, and child reaping.
7. Keep Capsule alive after an agent child exits.
8. Keep bounded output replay and snapshot watermarks.
9. Drain PTYs when viewers disconnect or become slow.
10. Keep helper observations advisory and all host effects outside Capsule.
11. Start zero sessions after a crash or new boot.

#### Pass conditions

- The service runs under the qualified nonroot identity.
- Client loss and child exit do not end unrelated PTYs.
- Capsule restart loses old PTYs and never silently restarts old commands.
- Slow output consumers cannot block PTY draining.
- Process-group supervision is not represented as a hostile-agent security boundary.

**Acceptance cases:** A62, A63, A64.

#### Removal and replacement

- Delete root PID1 setup, UID switching, DAC capabilities, sudo, firewall, and Capsule Landlock enforcement.
- Port only reviewed terminal parsing and process utilities.

#### Required evidence

- `evidence/openshell-v2/T18/implementation.json`
- `evidence/openshell-v2/T18/review.json`
- `evidence/openshell-v2/T18/test-results.json`
- `evidence/openshell-v2/T18/commands.jsonl`
- `evidence/openshell-v2/T18/source-map.json`
- `evidence/openshell-v2/T18/diff.patch`

#### Stop conditions

- Block a role that needs privileged Capsule startup.
- Do not add a second workload enforcement layer.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T19 — Implement relay attachment and input ownership

**Owner:** Capsule subagent.

**Dependencies:** T18, T13, T11.

**Source basis:** SPEC L158-L188; L336-L355; L361-L364.

#### Write scope

- crates/apps/jackin-capsule/src/relay/
- crates/services/jackin-sessions/src/attach/
- Host terminal adapter and attachment tests

#### Required inputs

- The qualified OpenShell interactive exec contract.
- C09 and C10.

**Task test directory:** `tests/v2/tasks/T19/`.

#### Procedure

1. Implement relay --stdio as a short-lived local-socket client.
2. Start each relay through explicit scoped OpenShell exec.
3. Use the absolute executable path with tty=false and no_login_shell=true.
4. Create a new upstream exec request ID for each new attachment.
5. Verify expected application binding and Capsule boot before input.
6. Implement output replay, snapshot watermarks, and replay-gap recovery.
7. Implement multiple viewers and one renewable input-writer lease.
8. Discard input from stale lease generations.
9. Do not replay unacknowledged input after disconnect.
10. Restart host control during a live PTY and reconnect without replaying uncertain agent startup.
11. Close only the attachment after relay EOF or cancellation.
12. Restore the host terminal after every exit path.

#### Pass conditions

- Binary frames survive without outer-PTY transformations.
- Detach and network interruption preserve the same live inner PTY.
- A new relay does not reuse admission replay as output replay.
- Two viewers cannot both send accepted writer input.
- Host-service restart preserves remote execution and binding without replaying uncertain agent startup.
- Host terminal state is restored after errors and interrupts.

**Acceptance cases:** A44, A65, A66, A67.

#### Removal and replacement

- Delete direct Docker attach, docker exec relay, and host-mounted Capsule socket transports.
- Delete old attachment fallback chains.

#### Required evidence

- `evidence/openshell-v2/T19/implementation.json`
- `evidence/openshell-v2/T19/review.json`
- `evidence/openshell-v2/T19/test-results.json`
- `evidence/openshell-v2/T19/commands.jsonl`
- `evidence/openshell-v2/T19/source-map.json`
- `evidence/openshell-v2/T19/diff.patch`

#### Stop conditions

- Block input when binding, boot, or writer generation is stale.
- Do not convert a lost relay response into an automatic agent restart.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T20 — Implement portable project materialization

**Owner:** Project subagent.

**Dependencies:** T13, T10.

**Source basis:** SPEC L192-L222; L250-L264.

#### Write scope

- crates/services/jackin-workspaces/src/materialize/
- crates/services/jackin-workspaces/src/source/
- Project transfer and storage tests

#### Required inputs

- C11 and frozen materialization manifests.
- Qualified transfer and storage contracts for each target.

**Task test directory:** `tests/v2/tasks/T20/`.

#### Procedure

1. Implement Shared, Worktree, and Clone as explicit source choices.
2. Keep source isolation separate from transfer and storage choices.
3. Capture the Git base, selected dirty files, untracked files, deletions, and exclusions.
4. Transfer required unpushed Git objects without host worktree pointers.
5. Detect nested repositories, submodules, and large-file pointers before transfer.
6. Use guarded host Git operations without repository-controlled host command execution.
7. Stage each import under a unique epoch through OpenShell facilities.
8. Reject escaping paths, unsafe links, special files, and transfer-budget violations.
9. Verify content digests before atomic project activation.
10. Use trusted live binds only on their qualified local profile.
11. Record the actual storage owner and durability class.

#### Pass conditions

- A remote worktree import has no broken host .git link.
- Interrupted staging never becomes the active project.
- Known account stores never enter a project archive.
- A hostile Git configuration cannot execute a host command during materialization.
- Snapshot mode does not silently replace a requested live-edit mode.

**Acceptance cases:** A68, A69, A70, A71.

#### Removal and replacement

- Replace old mount planners and Docker-specific isolation materializers.
- Do not retain raw host binds in isolated VM profiles.

#### Required evidence

- `evidence/openshell-v2/T20/implementation.json`
- `evidence/openshell-v2/T20/review.json`
- `evidence/openshell-v2/T20/test-results.json`
- `evidence/openshell-v2/T20/commands.jsonl`
- `evidence/openshell-v2/T20/source-map.json`
- `evidence/openshell-v2/T20/diff.patch`

#### Stop conditions

- Block unsupported source content before altering either checkout.
- Block a storage layout whose deletion or mount behavior is unknown.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T21 — Implement account grants and native providers

**Owner:** Account subagent.

**Dependencies:** T13, T10.

**Source basis:** SPEC L95-L111; L228-L248.

#### Write scope

- crates/services/jackin-accounts/src/grants/
- crates/services/jackin-accounts/src/providers/
- Account and scope tests

#### Required inputs

- C04, C06, C12, and C13.
- P01 account routes and actual upstream provider contracts.

**Task test directory:** `tests/v2/tasks/T21/`.

#### Procedure

1. Resolve stable Account IDs separately from display labels and provider records.
2. Create explicit grants with account revision, agent, scope, lifetime, and generation.
3. Verify private scope membership before admitting private credentials.
4. Keep saved instance account selection stable across reconnect.
5. Attach only the selected provider bindings through OpenShell.
6. Observe installed provider and effective-policy revisions before activation.
7. Account for provider rules that add effective network access.
8. Re-admit after a relevant account or provider revision changes.
9. Reject delayed requests from ended grants.
10. Keep local leases distinct from mandatory upstream authorization.

#### Pass conditions

- Changing the user's default account does not change a saved instance.
- A team workspace cannot silently receive a private account grant.
- A helper cannot enumerate unrelated accounts or grant itself a provider.
- A provider placeholder is treated as authority, not as harmless public text.

**Acceptance cases:** A73.

#### Removal and replacement

- Replace old account-to-container identity coupling.
- Do not create a second generic provider or credential store.

#### Required evidence

- `evidence/openshell-v2/T21/implementation.json`
- `evidence/openshell-v2/T21/review.json`
- `evidence/openshell-v2/T21/test-results.json`
- `evidence/openshell-v2/T21/commands.jsonl`
- `evidence/openshell-v2/T21/source-map.json`
- `evidence/openshell-v2/T21/diff.patch`

#### Stop conditions

- Block a private grant when authorized scope membership cannot be established.
- Do not claim local leases prevent cross-device access.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T22 — Implement native-login files and agent adapters

**Owner:** Account subagent.

**Dependencies:** T21, T20, T14.

**Source basis:** SPEC L97-L111; L234-L238; L487.

#### Write scope

- crates/adapters/jackin-agents/
- crates/services/jackin-accounts/src/native_login/
- Native-login and agent-route tests

#### Required inputs

- P01 supported agent and authentication routes.
- C08 and C12.

**Task test directory:** `tests/v2/tasks/T22/`.

#### Procedure

1. Implement explicit agent argv, environment, native-login, usage, and resume adapters.
2. Keep executable paths fixed by the resolved role inventory.
3. Stage only the admitted account files inside the private domain.
4. Publish credential revisions atomically.
5. Use one refresh writer within the declared deployment scope.
6. Reject unsupported cross-device rotating-refresh sharing.
7. Do not copy host home, SSH trees, or all-account stores.
8. Filter account environment from generic shells and other domains.
9. Preserve selected model, endpoint, and effort options only through validated agent adapters.
10. Exercise each adapter's credential expansion, argv, and environment with approved disposable credentials.
11. Record unsupported routes without a fallback or false success.

#### Pass conditions

- Two accounts cannot read each other's native credential files.
- An interrupted publication leaves one valid credential revision.
- Adapter reconnect preserves the account binding supplied by the admitted launch.

**Acceptance cases:** A76.

#### Removal and replacement

- Replace old auth-home seeding, UID-based account isolation, and broad credential mounts.
- Do not retain a generic refresh broker workaround.

#### Required evidence

- `evidence/openshell-v2/T22/implementation.json`
- `evidence/openshell-v2/T22/review.json`
- `evidence/openshell-v2/T22/test-results.json`
- `evidence/openshell-v2/T22/commands.jsonl`
- `evidence/openshell-v2/T22/source-map.json`
- `evidence/openshell-v2/T22/diff.patch`

#### Stop conditions

- Block a live-account test when approved credentials are absent.
- Do not count a --version result as authenticated-agent support.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T23 — Complete the native launch and activation path

**Owner:** Product subagent.

**Dependencies:** T16, T19, T20, T21, T22, T11.

**Source basis:** SPEC L115; L150-L152; L228-L241; L336-L360; L497.

#### Write scope

- crates/services/jackin-control/src/launch/
- crates/apps/jackin/src/cli/load.rs
- Vertical-path tests

#### Required inputs

- C05 and C06 activation protocol.
- All build, role, account, project, and Capsule receipts.

**Task test directory:** `tests/v2/tasks/T23/`.

#### Procedure

1. Implement load through the complete recorded operation sequence.
2. Resolve the role, project, target, scope, selected agent, and account before effects.
3. Build or reuse the exact role artifact and deliver it.
4. Create one admitted OpenShell domain with canonical Capsule restart policy Never.
5. Observe the runtime, image, policy, and provider configuration.
6. Materialize the project and verify its active epoch.
7. Recheck account, provider, and policy revisions before StartSession.
8. Re-admit activation when any required revision changes.
9. Record the full activation receipt in one local transaction.
10. Start the selected agent only after every required field matches.
11. Attach through a fresh relay without duplicating agent startup.
12. Run an authenticated coding task through each advertised route with approved disposable credentials.
13. Handle cancellation and lost responses through the journal.
14. Publish the complete operation and binding to inspection.

#### Pass conditions

- A missing receipt or stale revision prevents StartSession.
- Stale account, provider, or policy revisions block StartSession until the current binding is admitted.
- SandboxReady alone does not activate an agent.
- Two identical launch retries do not create duplicate agent processes.
- A real role reaches useful agent output and reconnects to the same live session.
- Every advertised route completes a real coding task through the native launch path.

**Acceptance cases:** A74, A77, A78, A79, A80.

#### Removal and replacement

- Replace the old launch pipeline, backend selector, and restore shortcuts.
- Do not keep a Docker execution fallback for failed OpenShell startup.

#### Required evidence

- `evidence/openshell-v2/T23/implementation.json`
- `evidence/openshell-v2/T23/review.json`
- `evidence/openshell-v2/T23/test-results.json`
- `evidence/openshell-v2/T23/commands.jsonl`
- `evidence/openshell-v2/T23/source-map.json`
- `evidence/openshell-v2/T23/diff.patch`

#### Stop conditions

- Block activation after any authority or image mismatch.
- Treat this vertical path as a demonstration until all release gates pass.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T24 — Implement domains sessions and the default shell

**Owner:** Product subagent.

**Dependencies:** T23.

**Source basis:** SPEC L72-L111; L180-L190; L358-L369.

#### Write scope

- crates/services/jackin-sessions/src/domains/
- crates/services/jackin-sessions/src/lifecycle/
- Shell and multi-account tests

#### Required inputs

- C04, C09, and the complete agent inventory.
- The logical-instance command contract.

**Task test directory:** `tests/v2/tasks/T24/`.

#### Procedure

1. Group several execution domains under one logical instance.
2. Reuse a domain only when its complete admitted trust context matches.
3. Create separate domains when account, policy, trust class, or capability changes.
4. Keep saved account selection when the user's default changes.
5. Require re-admission after the saved account revision changes.
6. Use independent writable materializations by default.
7. Require explicit collaboration approval for shared writable source.
8. Create the default project shell lazily in a credential-free domain.
9. Provide an explicit in-domain shell with a trusted authority warning.
10. Implement session start, stop, interrupt, detach, and agent switching.
11. Keep session exit separate from domain and instance exit.
12. Show new-process resume explicitly after retained-state recovery.

#### Pass conditions

- Two accounts and the generic shell use distinct required domains.
- Changing the default account does not silently reassign a saved instance.
- A changed account revision requires re-admission before activation.
- An in-domain shell displays and inherits the selected domain authority.
- Interrupt affects only the selected session.
- A dead Capsule does not appear as a reconnectable old PTY.

**Acceptance cases:** A08, A72, A75, A81, A82, A83.

#### Removal and replacement

- Delete one-container instance assumptions and environment-only account separation.
- Delete automatic account reassignment on reconnect.

#### Required evidence

- `evidence/openshell-v2/T24/implementation.json`
- `evidence/openshell-v2/T24/review.json`
- `evidence/openshell-v2/T24/test-results.json`
- `evidence/openshell-v2/T24/commands.jsonl`
- `evidence/openshell-v2/T24/source-map.json`
- `evidence/openshell-v2/T24/diff.patch`

#### Stop conditions

- Block reuse when any trust-context field differs.
- Do not merge account domains to reduce resource use.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T25 — Implement trusted usage and account presentation

**Owner:** Account subagent.

**Dependencies:** T21, T22, T24.

**Source basis:** SPEC L61; L238; L375-L379.

#### Write scope

- crates/services/jackin-accounts/src/usage/
- crates/adapters/jackin-agents usage modules
- Host usage presentation tests

#### Required inputs

- P01 usage semantics and current provider parsers.
- C12 account authority and cache rules.

**Task test directory:** `tests/v2/tasks/T25/`.

#### Procedure

1. Keep usage requests and interpretation in trusted host code.
2. Port reviewed provider parsers into the new agent adapters.
3. Key each cache entry by Account ID and credential-source revision.
4. Keep unavailable, stale, error, and zero values distinct.
5. Restrict each request to the active grant and run.
6. Deny unrelated account enumeration.
7. Invalidate delayed requests after a run or grant ends.
8. Show account identity through trusted labels rather than agent text.
9. Retain useful provider-specific behavior without old runtime coupling.

#### Pass conditions

- A stale request cannot refresh or expose an ended grant.
- Two account revisions cannot share a usage cache entry.
- A provider failure cannot appear as zero usage.
- Capsule never receives the global account catalog.

**Acceptance cases:** A84, A85.

#### Removal and replacement

- Replace old per-container usage tunnels and broker wire coupling.
- Do not keep a generic credential broker for usage convenience.

#### Required evidence

- `evidence/openshell-v2/T25/implementation.json`
- `evidence/openshell-v2/T25/review.json`
- `evidence/openshell-v2/T25/test-results.json`
- `evidence/openshell-v2/T25/commands.jsonl`
- `evidence/openshell-v2/T25/source-map.json`
- `evidence/openshell-v2/T25/diff.patch`

#### Stop conditions

- Block a parser claim when its provider response format is unverified.
- Do not weaken account scope to make usage polling succeed.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T26 — Implement host actions and approved tool domains

**Owner:** Product subagent.

**Dependencies:** T24, T21, T13.

**Source basis:** SPEC L61; L176; L240; L336-L346.

#### Write scope

- crates/services/jackin-control/src/host_actions/
- crates/services/jackin-control/src/tool_actions/
- Host action and approval tests

#### Required inputs

- C12 approval fields and P01 host-affordance inventory.
- Qualified OpenShell domain and credential mechanisms.

**Task test directory:** `tests/v2/tasks/T26/`.

#### Procedure

1. Keep clipboard, URL opening, export destinations, and approval dialogs on the host.
2. Require a matching local operator action and scoped correlation record.
3. Treat terminal escape sequences and helper events as untrusted data.
4. Keep approved actions pending while host control is stopped.
5. Restore host control without killing the remote session or granting default approval.
6. Normalize each tool request into fixed image identity and exact argv.
7. Bind approval to domain, operation, arguments, input digests, destination, expiry, and replay identity.
8. Invalidate approval after any material change.
9. Run the approved consumer in a separate OpenShell domain.
10. Give that consumer only its admitted capability or secret.
11. Keep project hooks and writable configuration from selecting the consumer executable.
12. Record denial, cancellation, timeout, revocation, and uncertain completion.
13. Keep secret-consuming output separate from ordinary terminal replay.

#### Pass conditions

- Unsolicited OSC or helper messages cannot open URLs or change the clipboard.
- A wrong-domain or changed-input approval fails.
- A replayed approval cannot repeat its effect.
- Stopping host control blocks the action without killing the remote session or granting approval.
- No secret enters the coding-agent PTY or arbitrary host shell.

**Acceptance cases:** A45, A86, A87, A88.

#### Removal and replacement

- Delete Linux peer-PID and privileged Capsule authority assumptions.
- Delete host shell execution of agent-supplied strings.

#### Required evidence

- `evidence/openshell-v2/T26/implementation.json`
- `evidence/openshell-v2/T26/review.json`
- `evidence/openshell-v2/T26/test-results.json`
- `evidence/openshell-v2/T26/commands.jsonl`
- `evidence/openshell-v2/T26/source-map.json`
- `evidence/openshell-v2/T26/diff.patch`

#### Stop conditions

- Block an action without a fixed executable identity and approved scope.
- Do not claim secret non-disclosure for a worker that receives the secret.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T27 — Implement checkpoints and conflict-safe export

**Owner:** Project subagent.

**Dependencies:** T20, T22, T24.

**Source basis:** SPEC L204-L226; L234; L367-L368.

#### Write scope

- crates/services/jackin-workspaces/src/checkpoint/
- crates/services/jackin-workspaces/src/export/
- Checkpoint and apply tests

#### Required inputs

- C11 and agent-specific resume formats.
- Qualified external retention storage.

**Task test directory:** `tests/v2/tasks/T27/`.

#### Procedure

1. Capture the declared project baseline and selected dirty or untracked content.
2. Preserve required unpushed commits with verified object transfer.
3. Capture only allowlisted resumable agent state.
4. Exclude credential stores, tokens, and unrequested host files.
5. Record the quiesce method and the checkpoint consistency class.
6. Store the checkpoint outside sandbox-owned storage.
7. Verify checksums and perform a restore test before a complete receipt.
8. Return patches or artifacts with their original baseline.
9. Check host changes before applying returned work.
10. Preserve both versions on conflict and report the exact paths.
11. Report incomplete transfer, flush failure, and unknown cleanup visibly.

#### Pass conditions

- Deleting the source sandbox does not delete the accepted checkpoint.
- Dirty and untracked selected files survive restore.
- Known credential canaries do not enter checkpoint content or logs.
- A changed host checkout is never overwritten blindly.
- A checksum alone is not reported as application-level consistency.

**Acceptance cases:** A89, A90, A91, A92.

#### Removal and replacement

- Replace Docker-layer snapshots and implicit copy-back paths.
- Do not use Git alone as complete conversation and untracked-file retention.

#### Required evidence

- `evidence/openshell-v2/T27/implementation.json`
- `evidence/openshell-v2/T27/review.json`
- `evidence/openshell-v2/T27/test-results.json`
- `evidence/openshell-v2/T27/commands.jsonl`
- `evidence/openshell-v2/T27/source-map.json`
- `evidence/openshell-v2/T27/diff.patch`

#### Stop conditions

- Block destructive follow-up when the required external checkpoint is incomplete.
- Do not claim arbitrary project data is secret-free merely because a scan passed.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T28 — Implement handoff eject exile and purge

**Owner:** Project subagent.

**Dependencies:** T27, T13, T24.

**Source basis:** SPEC L214-L226; L348-L369.

#### Write scope

- crates/services/jackin-control/src/handoff/
- crates/services/jackin-control/src/cleanup/
- Lifecycle command tests

#### Required inputs

- Checkpoint receipts, immutable runtime bindings, and retention owners.
- The command semantics in SPEC.md.

**Task test directory:** `tests/v2/tasks/T28/`.

#### Procedure

1. Quiesce source writers through the qualified application procedure.
2. Admit a new destination with its own account and authority scope.
3. Materialize and verify the accepted checkpoint before destination resume.
4. Use agent-supported conversation resume, not PTY or process-memory migration.
5. Retire the source only after destination acceptance.
6. Implement eject as scoped stop with documented retained state.
7. Implement exile as bulk eject within explicit target and project scope.
8. Implement purge only for confirmed owned resources and selected retained data.
9. Use immutable fences for every runtime mutation.
10. Keep partial and uncertain cleanup results in the journal.
11. Protect original checkouts, shared providers, and externally owned volumes.

#### Pass conditions

- A failed handoff leaves the last accepted checkpoint recoverable.
- An uncertain source action cannot be described as exactly-once migration.
- A stale binding cannot stop or delete a replacement.
- Bulk cleanup cannot expand beyond the selected scope.
- Purge never deletes the original user checkout.

**Acceptance cases:** A93, A94, A95.

#### Removal and replacement

- Delete old container discovery, adoption, absent-resource cleanup, and DinD garbage collection.
- Delete destructive NOT_FOUND shortcuts.

#### Required evidence

- `evidence/openshell-v2/T28/implementation.json`
- `evidence/openshell-v2/T28/review.json`
- `evidence/openshell-v2/T28/test-results.json`
- `evidence/openshell-v2/T28/commands.jsonl`
- `evidence/openshell-v2/T28/source-map.json`
- `evidence/openshell-v2/T28/diff.patch`

#### Stop conditions

- Block deletion when ownership, authority, or required retention is uncertain.
- Do not create a replacement because observation timed out.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T29 — Rebuild CLI Console and fleet observations

**Owner:** Product subagent.

**Dependencies:** T24, T25, T26, T28.

**Source basis:** SPEC L356-L399; L413-L419.

#### Write scope

- crates/apps/jackin/src/cli/
- crates/apps/jackin/src/console/
- CLI JSON and terminal UI tests

#### Required inputs

- Frozen command and JSON contracts.
- All application APIs and the P01 feature register.

**Task test directory:** `tests/v2/tasks/T29/`.

#### Procedure

1. Implement target list, load, hardline, shell, inspect, checkpoint, handoff, eject, exile, purge, and doctor entry points.
2. Use the shared host application API for every effect.
3. Group panes by project, instance, domain, and session.
4. Show trusted target, scope, account, policy, storage, resource, and freshness labels.
5. Keep agent activity separate from runtime state and verification verdicts.
6. Aggregate only authorized scopes with bounded concurrency and pagination.
7. Preserve stale rows and last-seen times during target failure.
8. Show fleet completeness and per-target errors.
9. Keep branch, pull-request, CI, clipboard-image, and export workflows through trusted adapters.
10. Regenerate completion and man-page sources from the new command model.

#### Pass conditions

- Two reachable targets remain usable when a third target fails.
- Incomplete pagination never becomes a complete empty fleet.
- Agent-reported done cannot authorize deletion or a passing verdict.
- Ambiguous human selectors produce a selection error.
- JSON output follows only the v2 schema.

**Acceptance cases:** A96, A97, A98.

#### Removal and replacement

- Replace old Docker-based status, Console discovery, and backend CLI flags.
- Remove old JSON fields and compatibility aliases that contradict v2 semantics.

#### Required evidence

- `evidence/openshell-v2/T29/implementation.json`
- `evidence/openshell-v2/T29/review.json`
- `evidence/openshell-v2/T29/test-results.json`
- `evidence/openshell-v2/T29/commands.jsonl`
- `evidence/openshell-v2/T29/source-map.json`
- `evidence/openshell-v2/T29/diff.patch`

#### Stop conditions

- Block a UI effect that bypasses the application API.
- Do not hide unavailable rows to present a healthy fleet.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T30 — Implement private developer service forwards

**Owner:** Product subagent.

**Dependencies:** T13, T23.

**Source basis:** SPEC L280-L284.

#### Write scope

- crates/adapters/jackin-openshell/src/forwards/
- crates/services/jackin-control/src/services/
- Developer service tests

#### Required inputs

- Actual forward and service API contracts from P02.
- C13 service exposure rules.

**Task test directory:** `tests/v2/tasks/T30/`.

#### Procedure

1. Create authenticated private forwards through OpenShell.
2. Bind local listener endpoints to loopback by default.
3. Track forward lifetime, target binding, owner, and request identity.
4. Show the convenient URL through trusted host UI.
5. Require a separate capability for public exposure.
6. Show public listener TLS and application-authentication requirements explicitly.
7. Reject every attempt to expose Capsule control publicly.
8. Clean up only confirmed owned forwards.
9. Handle disconnect and policy-update interruption without broader access.

#### Pass conditions

- A private server is unreachable through an unintended public listener.
- An unauthorized user cannot use the control-plane forward.
- Public exposure never implies OIDC or CLI authentication protection.
- Capsule control remains private.

**Acceptance cases:** A99, A100.

#### Removal and replacement

- Remove direct Docker port inspection and publication paths.
- Do not introduce a Jackin relay scheduler.

#### Required evidence

- `evidence/openshell-v2/T30/implementation.json`
- `evidence/openshell-v2/T30/review.json`
- `evidence/openshell-v2/T30/test-results.json`
- `evidence/openshell-v2/T30/commands.jsonl`
- `evidence/openshell-v2/T30/source-map.json`
- `evidence/openshell-v2/T30/diff.patch`

#### Stop conditions

- Block public exposure without its explicit grant and authentication contract.
- Do not invent a port route when the upstream API is unsupported.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T31 — Implement caches prewarm diagnostics and resource views

**Owner:** Operations subagent.

**Dependencies:** T15, T28, T29, T33.

**Source basis:** SPEC L127; L371; L465-L475.

#### Write scope

- crates/services/jackin-roles/src/cache/
- crates/services/jackin-control/src/doctor/
- crates/services/jackin-control/src/resources/
- Cache and diagnostics tests

#### Required inputs

- Artifact references, retained checkpoint records, and runtime observations.
- C07 and C16.

**Task test directory:** `tests/v2/tasks/T31/`.

#### Procedure

1. Track builder cache, registry artifacts, prepared images, and recreation artifacts separately.
2. Compute artifact liveness from active and retained v2 references.
3. Keep uncertain remote consumers live for pruning decisions.
4. Prune only explicitly owned artifacts under the selected retention policy.
5. Prewarm immutable tools and images with bounded concurrency.
6. Do not attach accounts or start companions during ordinary prewarm.
7. Report builder, gateway, target, optional engine, and host health separately.
8. Report requested, enforced, reserved, and measured resources separately.
9. Include all domains, supervisors, caches, and companions in instance accounting.
10. Measure build, launch, attach, reconnect, checkpoint, stop, and cleanup phases.

#### Pass conditions

- A target outage cannot cause deletion of an image needed for recreation.
- An unavailable separate builder cannot disable status or OpenShell management.
- Default prewarm creates no account grant or engine.
- Resource totals do not omit declared supervisor or companion overhead.

**Acceptance cases:** A101, A102, A103.

#### Removal and replacement

- Delete role-container-label image liveness and old DinD cache pruning.
- Delete diagnostics that require a Docker agent backend.

#### Required evidence

- `evidence/openshell-v2/T31/implementation.json`
- `evidence/openshell-v2/T31/review.json`
- `evidence/openshell-v2/T31/test-results.json`
- `evidence/openshell-v2/T31/commands.jsonl`
- `evidence/openshell-v2/T31/source-map.json`
- `evidence/openshell-v2/T31/diff.patch`

#### Stop conditions

- Block pruning when ownership or retained consumers are unknown.
- Do not present configured VM RAM as measured host RSS.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T32 — Replace the independent verification worker

**Owner:** Verifier subagent.

**Dependencies:** U02, T13, T20, T10.

**Source basis:** SPEC L326-L334; L503.

#### Write scope

- crates/services/jackin-control/src/verification/
- Verification records and independent-worker tests

#### Required inputs

- C15 and the qualified strict-worker contract.
- Reviewed frozen-policy and result-binding logic from the old workflow code.

**Task test directory:** `tests/v2/tasks/T32/`.

#### Procedure

1. Freeze candidate, image, check-bundle, argv, budget, and deadline digests.
2. Create a separate credential-free OpenShell worker.
3. Use immutable candidate and trusted-check inputs.
4. Require the qualified read-only-root and bounded-scratch capability.
5. Disable ordinary network and coding-agent account access.
6. Observe command completion through the authoritative runtime contract.
7. Keep output bounded and mark truncation explicitly.
8. Record code failure, timeout, infrastructure failure, cancellation, and unknown outcome separately.
9. Bind the host verdict to the exact frozen inputs.
10. Confirm worker cleanup without deleting candidate or checkpoint evidence.
11. Reject unsupported worker profiles before launch.

#### Pass conditions

- A candidate cannot change the check bundle or produce its own passing host verdict.
- A changed digest invalidates earlier verification evidence.
- Worker metadata and content mutation negatives pass.
- No Docker or host worker starts when OpenShell is unavailable.

**Acceptance cases:** A22, A23, A24, A25, A104, A105, A106.

#### Removal and replacement

- Delete jackin-workflow direct Docker run, image, inspect, wait, and remove code.
- Retain only reviewed pure policy and verdict logic in the new module.

#### Required evidence

- `evidence/openshell-v2/T32/implementation.json`
- `evidence/openshell-v2/T32/review.json`
- `evidence/openshell-v2/T32/test-results.json`
- `evidence/openshell-v2/T32/commands.jsonl`
- `evidence/openshell-v2/T32/source-map.json`
- `evidence/openshell-v2/T32/diff.patch`

#### Stop conditions

- Block verification when strict worker support or trustworthy completion is absent.
- Do not treat an infrastructure timeout as a code-test pass.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T33 — Integrate the managed engine capability into Jackin

**Owner:** Companion adapter subagent.

**Dependencies:** U08, T13, T23, T27.

**Source basis:** SPEC L286-L324; L503.

#### Write scope

- crates/adapters/jackin-openshell/src/companions/
- crates/services/jackin-control/src/capabilities/
- Role and target capability integration tests

#### Required inputs

- Qualified upstream companion API and U08 evidence.
- C14 and frozen domain-creation inputs.

**Task test directory:** `tests/v2/tasks/T33/`.

#### Procedure

1. Resolve the engine request before domain creation.
2. Reject unsupported target profiles before any runtime effect.
3. Request the upstream-owned companion through OpenShell only.
4. Record parent identity, capability revision, endpoint generation, and cleanup status.
5. Expose only the admitted engine endpoint to the owning domain.
6. Display Docker run, Compose, and Testcontainers support accurately.
7. Keep default-off domains free of engine secrets and resources.
8. Require checkpoint and reprovision when an existing domain enables engine access.
9. Observe companion lifecycle through upstream receipts.
10. Keep all engine VM reconciliation outside Jackin.

#### Pass conditions

- Engine=false produces no endpoint, credential, prewarm, or companion.
- A Kubernetes target without the capability rejects the request before launch.
- A wrong account domain cannot use another engine endpoint.
- Stopping or deleting the parent revokes access and confirms owned cleanup.

**Acceptance cases:** A28, A29, A30, A31, A39, A107.

#### Removal and replacement

- Delete the old launch-dind and cleanup-dind-gc implementations.
- Do not retain external daemon configuration as a supported runtime fallback.

#### Required evidence

- `evidence/openshell-v2/T33/implementation.json`
- `evidence/openshell-v2/T33/review.json`
- `evidence/openshell-v2/T33/test-results.json`
- `evidence/openshell-v2/T33/commands.jsonl`
- `evidence/openshell-v2/T33/source-map.json`
- `evidence/openshell-v2/T33/diff.patch`

#### Stop conditions

- Block engine-enabled v2 completion until U08 passes.
- Do not substitute a build-only service for Docker compatibility.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T34 — Complete workflow and product behavior coverage

**Owner:** Operations subagent.

**Dependencies:** T32, T26, T27, T29, T30, T31.

**Source basis:** SPEC L61-L63; L326-L344; L375-L383; L499-L507.

#### Write scope

- crates/services/jackin-control/src/workflows/
- Reviewed agent-status and project-context adapters
- Product journey tests

#### Required inputs

- P01 complete feature register.
- Verified session, tool-action, checkpoint, and worker APIs.

**Task test directory:** `tests/v2/tasks/T34/`.

#### Procedure

1. Port required workflow state and repair semantics to v2 operations.
2. Keep host verdict authority outside coding agents and workers.
3. Use the new independent verifier for frozen checks.
4. Use the approved tool-domain route for privileged product actions.
5. Preserve project branch, pull-request, CI, and relevant agent-status behavior.
6. Use trusted host adapters for external product metadata.
7. Keep local project status useful when external services fail.
8. Map every retained P01 journey to an executed v2 test.
9. Document every deliberate removal with its specification basis.
10. Do not introduce team queues or a general scheduler as hidden prerequisites.

#### Pass conditions

- Every retained behavior has a passing implementation test or a visible release blocker.
- External metadata failure does not terminate a useful local session.
- Agent status cannot set workflow acceptance or erase failed verification.
- No product route calls an old runtime helper.

**Acceptance cases:** A108, A109.

#### Removal and replacement

- Delete obsolete workflow runtime and status integration code after its replacement tests pass.
- Do not retain unused old foundations as claimed active guarantees.

#### Required evidence

- `evidence/openshell-v2/T34/implementation.json`
- `evidence/openshell-v2/T34/review.json`
- `evidence/openshell-v2/T34/test-results.json`
- `evidence/openshell-v2/T34/commands.jsonl`
- `evidence/openshell-v2/T34/source-map.json`
- `evidence/openshell-v2/T34/diff.patch`

#### Stop conditions

- Block feature completion when the register contains an unmapped retained behavior.
- Do not remove a required workflow to make the test matrix smaller.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T35 — Qualify local containers and remote container gateways

**Owner:** Qualification subagent.

**Dependencies:** T29, T30, T31, T34, U03.

**Source basis:** SPEC L208; L252-L264; L487-L491; L505.

#### Write scope

- tests/v2/targets/container/
- Container compatibility evidence and deployment instructions

#### Required inputs

- Required container rows in ACCEPTANCE.md.
- The complete v2 application and approved live-account fixtures.

**Task test directory:** `tests/v2/tasks/T35/`.

#### Procedure

1. Test native Linux Docker as a local OpenShell target.
2. Test macOS Docker Desktop and OrbStack as separate target rows.
3. Test an independently hosted remote Linux gateway.
4. Test the required remote macOS gateway row.
5. Use registry delivery for remote role images.
6. Run snapshot and Git materialization journeys.
7. Run trusted live Shared and Worktree journeys only on their admitted profiles.
8. Run the combined two-account, shell, real-agent, reconnect, action, recovery, and cleanup journey.
9. Test a separate builder outage while the gateway remains available.
10. Record exact artifacts, host versions, architecture, and effective enforcement.
11. Keep every missing target or account fixture blocked.

#### Pass conditions

- One local compiler example does not qualify another platform or remote target.
- Host paths refer to the actual runtime host.
- Live editing remains live and snapshot transfer remains explicit.
- Unrelated resources survive failure and cleanup tests.

**Acceptance cases:** A55, A110, A111, A117.

#### Removal and replacement

- Remove platform claims supported only by old Docker or Apple behavior.
- Do not introduce platform-specific execution fallbacks.

#### Required evidence

- `evidence/openshell-v2/T35/implementation.json`
- `evidence/openshell-v2/T35/review.json`
- `evidence/openshell-v2/T35/test-results.json`
- `evidence/openshell-v2/T35/commands.jsonl`
- `evidence/openshell-v2/T35/source-map.json`
- `evidence/openshell-v2/T35/diff.patch`

#### Stop conditions

- Block each required target row without its own fresh evidence.
- Do not infer Docker Desktop behavior from OrbStack results.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T36 — Qualify isolated VM profiles and engine domains

**Owner:** Qualification subagent.

**Dependencies:** T29, T30, T31, T33, T34, U03.

**Source basis:** SPEC L135-L144; L210-L222; L258-L264; L290-L322.

#### Write scope

- tests/v2/targets/vm/
- VM compatibility evidence and operator profiles

#### Required inputs

- Fixed small and large VM profile definitions.
- The qualified VM and companion tuples.

**Task test directory:** `tests/v2/tasks/T36/`.

#### Procedure

1. Test each required VM architecture and fixed resource shape.
2. Verify the actual assigned identity, tool paths, and work directory.
3. Use guest-owned storage without raw host binds.
4. Test role import, image preparation, and project transfer.
5. Test small and large profiles without pretending per-sandbox sizing is enforced.
6. Test mount limits without widening source exposure.
7. Test orderly and interrupted shutdown with flush failures visible.
8. Restore dirty work and conversation state from independent checkpoints.
9. Run engine-enabled and engine-disabled domain journeys.
10. Measure guest resources and host overhead separately.

#### Pass conditions

- A requested VM boundary never becomes a container silently.
- A guest cannot access unrelated host directories through project projection.
- Overlay deletion cannot erase the accepted external checkpoint.
- Engine access does not place the trusted agent runtime inside daemon authority.

**Acceptance cases:** A112, A113, A117.

#### Removal and replacement

- Delete old Apple Container backend assets and VM compatibility assumptions.
- Do not use raw virtiofs host sharing for isolated profiles.

#### Required evidence

- `evidence/openshell-v2/T36/implementation.json`
- `evidence/openshell-v2/T36/review.json`
- `evidence/openshell-v2/T36/test-results.json`
- `evidence/openshell-v2/T36/commands.jsonl`
- `evidence/openshell-v2/T36/source-map.json`
- `evidence/openshell-v2/T36/diff.patch`

#### Stop conditions

- Block a VM shape or architecture that lacks its own runtime evidence.
- Do not report sparse disk capacity as allocated bytes.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T37 — Qualify the Kubernetes deployment tuple

**Owner:** Qualification subagent.

**Dependencies:** T29, T30, T31, T34, U03.

**Source basis:** SPEC L212; L266-L284; L489-L491.

#### Write scope

- tests/v2/targets/kubernetes/
- Operator deployment documentation and compatibility evidence

#### Required inputs

- Official OpenShell Helm and Agent Sandbox resources.
- The pinned dedicated-supervisor tuple and approved cluster access.

**Task test directory:** `tests/v2/tasks/T37/`.

#### Procedure

1. Deploy the official resources through the authorized operator workflow.
2. Use dedicated supervisors for the initial qualified profile.
3. Record chart, gateway, supervisor, trusted runtime, Agent Sandbox, CNI, kernel, runtime, and storage versions.
4. Use separate authority scopes for private and shared accounts.
5. Use separate staging and production registrations for materially different trust classes.
6. Test real ingress and egress enforcement with negative traffic.
7. Verify that developers cannot mutate enforcement or bootstrap resources.
8. Use automatic persistent /sandbox storage without an interfering child mount.
9. Test image-pull authorization, quotas, Pod replacement, stop/start, and Sandbox deletion.
10. Verify provider-private data stays outside shared PVCs.
11. Test gateway interruption, transport loss, and backup/restore expectations.
12. Reject managed-engine requests until this driver has its own qualified integration.

#### Pass conditions

- Policy-object existence alone cannot pass the network test.
- Pod replacement preserves qualified retained files but not old PTYs.
- Sandbox deletion follows documented storage ownership.
- Agents receive no Pod-creation token or physical host Docker socket.
- Jackin creates no Pods or custom operator resources.

**Acceptance cases:** A114, A115, A116, A117.

#### Removal and replacement

- Remove any proposed Jackin Kubernetes controller or direct Pod lifecycle code.
- Do not mix development and release components silently.

#### Required evidence

- `evidence/openshell-v2/T37/implementation.json`
- `evidence/openshell-v2/T37/review.json`
- `evidence/openshell-v2/T37/test-results.json`
- `evidence/openshell-v2/T37/commands.jsonl`
- `evidence/openshell-v2/T37/source-map.json`
- `evidence/openshell-v2/T37/diff.patch`

#### Stop conditions

- Block Kubernetes release support without the complete tested tuple.
- Do not use cluster-admin credentials to hide missing developer-scope APIs.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T38 — Run cross-area failure security and resource tests

**Owner:** Qualification subagent.

**Dependencies:** T35, T36, T37, T32, T33.

**Source basis:** SPEC L336-L355; L471-L491; L505-L509.

#### Write scope

- tests/v2/faults/
- tests/v2/security/
- Cross-target performance and review evidence

#### Required inputs

- Every accepted task and target tuple.
- Historical regression fixtures and the complete acceptance matrix.

**Task test directory:** `tests/v2/tasks/T38/`.

#### Procedure

1. Inject failure before and after every journaled external effect.
2. Test lost responses, duplicate requests, stale generations, and same-name replacements.
3. Test wrong workspace, wrong account, and ended-grant requests.
4. Kill clients, relays, Capsule, host service, gateway, and companion at distinct boundaries.
5. Restart host control after uncertain create, exec, stop, delete, and publication effects.
6. Test dirty work, interrupted transfers, storage errors, and unknown cleanup.
7. Run the complete combined journey on each required target mechanism.
8. Have an independent agent repeat critical journeys from fresh resources.
9. Measure one-instance and two-instance resource totals and launch phases.
10. Record cache state, sample count, median, range, and host details.
11. Report percentiles only when the declared sample requirement is met.
12. Review every failure without deleting inconvenient evidence.

#### Pass conditions

- No injected failure causes unrelated-resource deletion or silent data loss.
- Each external effect is replayed safely or remains explicitly uncertain.
- Runtime and publication uncertainty never appears as guessed success.
- All stale-authority and same-name replacement cases fail safely.
- Every mandatory target journey has independent fresh evidence.
- Measurements distinguish requested limits, enforced scopes, and observed usage.

**Acceptance cases:** A41, A117, A118, A119.

#### Removal and replacement

- Remove test exceptions that bypass real credential, worker, or companion paths.
- Do not erase failing historical fixtures.

#### Required evidence

- `evidence/openshell-v2/T38/implementation.json`
- `evidence/openshell-v2/T38/review.json`
- `evidence/openshell-v2/T38/test-results.json`
- `evidence/openshell-v2/T38/commands.jsonl`
- `evidence/openshell-v2/T38/source-map.json`
- `evidence/openshell-v2/T38/diff.patch`

#### Stop conditions

- Block release after any missing negative case or unexplained failure.
- Do not replace a required runtime case with a mock.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T39 — Delete all obsolete implementation and rebuild packaging

**Owner:** Integration subagent.

**Dependencies:** P01, T31, T34, T35, T36, T37, T38.

**Source basis:** SPEC L5-L7; L125-L127; L156; L334; L404-L419; L503-L509.

#### Write scope

- All paths classified DELETE or replaced by P01
- Cargo files, CI sources, generated manifests, scripts, packaging, and current documentation

#### Required inputs

- The complete P01 disposition inventory.
- Passing replacement evidence and the new production dependency graph.

**Task test directory:** `tests/v2/tasks/T39/`.

#### Procedure

1. Delete every replaced runtime-prefix crate and the old Docker execution adapter.
2. Delete Apple backend source, backend enums, and obsolete resource manifests.
3. Delete old Capsule privilege, peer-PID, firewall, and sudo paths.
4. Delete DinD launch, discovery, adoption, and garbage collection.
5. Delete direct Docker probes, workers, attach routes, and snapshot routes.
6. Delete v1 parsers, importers, compatibility flags, and dormant feature-gated implementations.
7. Port only approved pure helpers with their new contract tests.
8. Remove obsolete dependencies, generated jobs, fixtures, completions, and installer entries.
9. Regenerate CI through the actual pinned generator recorded by P01.
10. Audit every host subprocess and Docker API call by ownership.
11. Compare the final tree against every inventory record.
12. Run clean-build and installation tests from fresh directories.
13. Rerun A13 and A14 against the final graph and composition after legacy deletion.

#### Pass conditions

- Every legacy inventory record has a verified final disposition.
- No production path can execute an agent outside OpenShell.
- No old backend remains behind a feature flag or generic command helper.
- Every retained Docker call belongs to approved image infrastructure.
- Fresh installation requires no old executable, state, or configuration.
- The final crate graph and application composition pass the production-boundary checks.

**Acceptance cases:** A120, A121, A122, A13, A14.

#### Removal and replacement

- Delete the old implementation, not only its workspace references.
- Keep historical evidence as non-executable documentation only.

#### Required evidence

- `evidence/openshell-v2/T39/implementation.json`
- `evidence/openshell-v2/T39/review.json`
- `evidence/openshell-v2/T39/test-results.json`
- `evidence/openshell-v2/T39/commands.jsonl`
- `evidence/openshell-v2/T39/source-map.json`
- `evidence/openshell-v2/T39/diff.patch`

#### Stop conditions

- Block completion when one executable legacy path remains unclassified.
- Do not delete a valid replacement test to make the clean build pass.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---

### T40 — Approve the full v2 release candidate

**Owner:** Independent review subagent.

**Dependencies:** T39.

**Source basis:** SPEC L9; L477-L509.

#### Write scope

- Release-candidate documentation, final compatibility matrix, and acceptance summary
- Version and packaging changes through the integration subagent

#### Required inputs

- All task reports, independent reviews, and raw test evidence.
- The frozen feature register and complete compatibility tuples.

**Task test directory:** `tests/v2/tasks/T40/`.

#### Procedure

1. Run the release_gate_protocol preflight while T40 is RUNNING or REVIEW.
2. Check that every upstream patch has a pinned diff, owner, upstream plan, and regression evidence.
3. Require explicit maintainer approval for any supported patched tuple.
4. Document fresh setup and manual preservation of old user data.
5. Do not add an old-state importer or runtime cleanup fallback.
6. Document changed commands, unsupported debugging, and each target's actual limits.
7. Document checkpoint restore, account scope, engine default-off behavior, and unknown-outcome recovery.
8. Set the approved release candidate version to 2.0.0.
9. Build and inspect packages from a clean checkout.
10. Produce the final removal inventory, test summary, blocker list, and source references.
11. Do not publish a release or alter production infrastructure without explicit authorization.
12. Check prior task statuses and assignment results.
13. Check reviewed artifacts, merged evidence, and required target results.
14. Exercise A123 refusals for missing, failed, blocked, and unreviewed prerequisites.
15. Confirm every refusal prevents approval. Treat synthetic refusal fixtures as contract tests only.
16. Run A124 and A125 against the actual candidate. Record actual results for all three T40 cases.
17. Obtain an independent T40 review and rerun checks against the merged source.
18. Finalize only after all prior tasks and every case assignment pass, including A123, A124, and A125.
19. Bind completion and approval to the reviewed source commit, package digests, and evidence hashes.
20. Write T40 PASS, completion, finalization receipt, and approval together only after every prerequisite passes.

#### Pass conditions

- A blocked engine, verifier, mutation, account, or target gate prevents full release approval.
- Documentation matches tested behavior rather than desired behavior.
- The final report distinguishes code review, mock tests, live tests, and independent repeats.
- No backward-compatibility path or legacy execution artifact ships.
- Preflight requires every task except T40 and each prior task assignment to pass.
- Prior task artifacts, independent reviews, merged evidence, and required target results must pass.
- T40 is RUNNING or REVIEW during preflight.
- Preflight excludes A123, A124, and A125, returns PREFLIGHT_ONLY, and never approves release.
- Finalization requires every task except T40 and every case assignment to pass, including A123, A124, and A125.
- Finalization also requires all targets and artifacts, an independent T40 review, and merged-source checks.
- Finalization excludes only persisted T40=PASS and its own future receipt from the input set.
- It writes T40 PASS, a finalization receipt, completion, and approval together.
- Approval binds the reviewed source commit, package digests, and evidence hashes.
- Synthetic gate fixtures test refusals only and never count as candidate evidence.
- A failed prerequisite writes neither a completion record nor approval.

**Acceptance cases:** A123, A124, A125.

#### Removal and replacement

- Remove temporary implementation scaffolding and obsolete v1 documentation from active guidance.
- Do not remove source evidence or required failure records.

#### Required evidence

- `evidence/openshell-v2/T40/implementation.json`
- `evidence/openshell-v2/T40/review.json`
- `evidence/openshell-v2/T40/test-results.json`
- `evidence/openshell-v2/T40/commands.jsonl`
- `evidence/openshell-v2/T40/source-map.json`
- `evidence/openshell-v2/T40/diff.patch`

#### Stop conditions

- Report partial completion when any mandatory gate remains blocked.
- Do not relabel a demonstration as the completed rewrite.

**Review:** A different subagent reviews the result. The integration subagent reruns affected tests after merge.

---
