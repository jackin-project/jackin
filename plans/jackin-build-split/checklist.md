# Jackin Build Split

- Status: IN PROGRESS
- Record date: 2026-10-05
- Scope: Initial evidence record.

## Starting state

- Host: `bastion`, Debian 13.7, x86_64, kernel 6.12.94.
- Repository: `https://github.com/jackin-project/jackin.git`.
- Base: `main` at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`.
- Task worktree: `/root/Projects/tailrocks/jackin-project/jackin-refactor-build-split`.
- Task branch: `refactor/build-split`.
- The task worktree was clean at the base SHA.
- The original `main` worktree had one unrelated `mise.lock` modification.
- The original lock hash was `6be630be77daa3073ec74340cd6a113b969ee9117a69b8d48ebe5d5501df764c`.
- The task worktree lock hash was `ac3b0998f110538c5bb34f2653afaf7f2713d7f9f026dde1bb3f50845cef06ef`.
- The task worktree lock blob was `c78d5f9ff2b48c70d488fa9a4cfbf5f750ef11ce`.
- Documentation work preserves both lock states. No lock edit is in scope.

## Requirement trace

The IDs below map each assigned outcome to its scenario, interface, gate, owner, dependencies, and evidence.

| ID | Required outcome, scenario, and interface | Status | Owner | Dependencies | Gate | Evidence |
|---|---|---|---|---|---|---|
| DOC-01 | Create eight task records in `plans/jackin-build-split/`. | PASS | `task_records` | Assigned worktree | All eight files exist and link. | [Records](final-report.md) |
| STYLE-01 | Apply ASD-STE100 to descriptions and procedural sentences. | PASS | `task_records` | None | Descriptions ≤25 words; imperatives ≤20 words. | All eight Markdown records. |
| FACT-01 | Record evidence and mark unknown outcomes with reasons. | IN PROGRESS | `coordinator` and work owners | Static and execution reviews | No unverified result is complete. | Records linked below. |
| MODEL-01 | Check coordinator model prerequisite for section 2.1. | FAIL | `coordinator` | Active root process settings | Required model is Luna/max. | [Codex review](reviews.md) |
| MODEL-02 | Confirm work-agent runtime settings. | IN PROGRESS | `codex_schema_runtime` | Runtime evidence | Confirm every active agent. | [Catalog result](#codex-catalog-command); [Codex review](reviews.md) |
| HOST-01 | Record Debian host inventory and repository identity. | PASS | `task_records` | Initial memory sample is historical. | Keep timestamped command evidence. | [Host results](debian-results.md); [host command](#host-inventory-command) |
| REPO-01 | Record base SHA, task worktree, and branch. | PASS | `task_records` | Clean task worktree | Match initial `main` SHA. | Starting state above; [branches](branches.md) |
| LOCK-01 | Record the initial dirty lock hash and preserve it. | PASS | `task_records` | Original and task worktrees | Hashes remain unchanged. | Starting state above; [branches](branches.md) |
| BRANCH-01 | Inventory branch and PR scenario; record branch interface and owners. | IN PROGRESS | `branches` | Fetched refs and PR review | Group paths and dependency gates remain under review; do not merge a whole branch. | [Branch record](branches.md) |
| CRATE-01 | Inspect crate boundaries and build metadata statically. | IN PROGRESS | `crate_design` | Naming and consumer proposal; build evidence | No crate name or boundary is selected. | [Crate plan](crate-plan.md) |
| CRATE-02 | Compare usage-crate naming and ownership proposals. | IN PROGRESS | `crate_design` | Consumer map and protocol DTO ownership | Session/host and token/provider names remain alternatives. Do not infer boundaries from names. | [Usage proposals](crate-plan.md#usage-crate-naming-proposals) |
| BUILD-01 | Measure baseline before extraction and compare after extraction. | IN PROGRESS | `build_baseline` | Repeated builds, compiler image, and security review | One build proof succeeded. Comparable baseline remains NOT RUN. | [Build results](build-results.md) |
| CI-01 | Inspect generator and CI scenario; preserve required-job interface. | IN PROGRESS | `velnor_recon` and `jackin_generator_config` | Current generator refs | Complete coverage review. | [CI coverage](ci-coverage.md) |
| CI-02 | Decide if the generated workflow meets required coverage. | IN PROGRESS | `preflight_security_review` | Missing coverage findings | Workflow disposition is NOT APPROVED; close gaps before approval. | [Workflow review](reviews.md#ci-workflow-review) |
| ARCH-01 | Review Architect manifest at the exact PR head. | PASS | `architect_contract` | Head `0592d0deeaeaa5b785fa67a43d23d3b627552720` | Static review only. | [Architect review](reviews.md#architect-integration-review) |
| ARCH-02 | Run role parser and local repository validation. | PASS | `architect_contract` and `execution_crosscheck` | Binary SHA `e899a8e5f51ebb5f20fce5a379a3f4de4555911549e743ca625efb4a3988c2ac`; manifest SHA `eb08cf89aa32971c17db9875ec633ac22fe609927abf23fd18182756819e7fca` | Parser and local strict-manifest, Dockerfile, and hook checks passed. | [Architect review](reviews.md#architect-integration-review) |
| ARCH-03 | Run the actual role through Jackin. | NOT RUN | `architect_contract` | Security approval and Jackin runtime | Record the live role response. | [Architect review](reviews.md#architect-integration-review) |
| ARCH-04 | Complete full Architect role validation, including marketplace preflight. | FAIL | `architect_contract` | Authoritative marketplace identity and path | Unauthenticated public marketplace GET returned 404; no auth, container, hook, or role load occurred. | [Architect review](reviews.md#architect-integration-review) |
| ACCOUNT-01 | Complete runtime account discovery through workspace launch. | IN PROGRESS | `debian_codex_route` | Jackin binary, config, security gate | CLI path checks are complete; Jackin runtime registration and launch remain NOT RUN. | [Debian results](debian-results.md#account-route) |
| ACCOUNT-02 | Review the current `CODEX_HOME` source correction. | PASS | `execution_crosscheck` | Commits `688057f40173d32dda04a55bff1e3868c219710d` and `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a` | Exact-source review PASS; Cargo tests and runtime route remain NOT RUN. | [Debian results](debian-results.md#account-route) |
| CLI-01 | Review removal of the unsupported initial prompt option. | PASS | `execution_crosscheck` | Commit `640b33f9598307a360484526a46c9c20bd068f4e` | Source review PASS. Cargo tests remain NOT RUN pending MBX. | [CLI prompt cleanup](reviews.md#cli-prompt-cleanup) |
| CLI-02 | Run focused launch and restore tests after prompt removal. | NOT RUN | `jackin_cli` | Reviewed MBX activation | Cargo tests remain NOT RUN. | [CLI prompt cleanup](reviews.md#cli-prompt-cleanup) |
| RESTORE-01 | Review identity-preserving restore source change. | PASS | `execution_crosscheck` | Commit `234fc0ea3813d8cabe579a8a8e0a0b1162eb5230` | Source review PASS; restore tests remain NOT RUN. | [Route and restore review](reviews.md#account-route-and-restore-source-reviews) |
| SEC-01 | Review execution boundary, container, auth, and cache provenance. | IN PROGRESS | `preflight_security_review` and `unprivileged_exec_design` | Exact execution design | Resolve every preliminary gate. | [Security review](reviews.md) |
| REDACT-01 | Fix and re-review the Jackin redaction findings. | IN PROGRESS | `consolidation_review` | Architecture-level replacement and reviewed MBX activation | Cover the initial canaries and current P1 findings. Obtain exact-head Sol re-review. | [Redaction review](reviews.md#jackin-redaction-review) |
| REDACT-02 | Fix the five P1 findings from the exact-head redaction review. | IN PROGRESS | `consolidation_review` | Architecture-level replacement | Fix findings from rejected commits `63d5ef9046d4948a3cddb239e891db49be654d34` and `69b82de1a48cc18add3933e1995028c8aa2722e8`; obtain exact-head review. | [Redaction follow-up](reviews.md#redaction-follow-up-review) |
| REDACT-03 | Review the redaction source correction at its exact head. | PASS | `execution_crosscheck` | Commit `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578` | Sol accepted the source correction. Tests and Clippy remain NOT RUN. | [Nested PEM review](reviews.md#nested-pem-and-current-correction) |
| REDACT-04 | Run redaction tests and Clippy on the accepted source. | NOT RUN | `consolidation_review` | Reviewed MBX activation | The sequential marker fixture is not a cryptographic PEM parser test. | [Nested PEM review](reviews.md#nested-pem-and-current-correction) |
| CODEX-01 | Record official configuration schema and local model catalog. | IN PROGRESS | `codex_schema_runtime` | Active-agent runtime confirmation | Confirm settings at runtime. | [Official reference](https://developers.openai.com/codex/config-reference); [catalog command](#codex-catalog-command) |
| CODEX-02 | Record configured model and effort fields in the app-server schema. | PASS | `codex_schema_runtime` | Generated v2 schema bundle evidence | Record schema exposure; confirm active settings separately. | [Codex schema evidence](reviews.md#codex-schema-and-catalog) |
| CODEX-03 | Replace the MCP-list probe with endpoint-free app-server configuration reading. | IN PROGRESS | `debian_codex_route` and `execution_crosscheck` | Schema-verified `config/read` wrapper | Fail closed on enabled or incomplete `mcp_servers`; do not read a real profile. | [Probe review](reviews.md#endpoint-free-codex-configuration-probe); [Debian evidence](debian-results.md#rejected-mcp-list-preflight) |
| MBX-01 | Verify official MBX artifact provenance and activation. | IN PROGRESS | `unprivileged_exec_design` | Security review | Verify before compilation. | [Security review](reviews.md#mbx-provenance) |
| MBX-02 | Validate the initial `register-rust` launcher syntax. | FAIL | `unprivileged_exec_design` | Launcher SHA `3bb10a21339c0aab3e7fc10f11ee57f97fc6d98a37e4972aa9fcac10ad6ef8c6` | This SHA failed; approved SHA `1388fb2` fixed the mount syntax. | [Launcher preflight](reviews.md#launcher-preflight) |
| MBX-03 | Run the Sol-reviewed MBX launcher inside the offline namespace. | FAIL | `unprivileged_exec_design` | Supported offline Mise setup and exact Sol review | Runtime tried online version-list resolution; no compilation or acquisition occurred. | [Offline launcher attempt](reviews.md#offline-launcher-attempt) |
| MBX-04 | Record the first main-source launcher attempt. | FAIL | `unprivileged_exec_design` | Launcher SHA `d31315193da05f29746509b3e395aca1394c6b7160c39b8bb075782c9998c748` | Registration/acquisition/locked fetch passed by owner report. The build failed because `/usr/bin/cc` targets missing `/etc/alternatives`; four crates attempted. | [First MBX attempt](build-results.md#first-main-source-attempt) |
| MBX-05 | Review the direct-linker compiler-image candidate. | FAIL | `unprivileged_exec_design` | Candidate hash `9fe5f1143587373d0ab5df821ff68e6abcc72280f642bdea986df02d1d0e85d4` | Sol rejected it before execution because the environment checker rejects compiler variables. No phase ran. | [Compiler image review](reviews.md#main-source-compiler-image-attempt) |
| MBX-06 | Run one bounded initial-source build proof. | PASS | `unprivileged_exec_design` and `build_baseline` | Sol-reviewed launcher hash `9d94884ac525c91f5a2e7c5abb6f068cdb814ef0780097fa840d35b57c212851` | One build succeeded. It does not establish cache reuse or a performance baseline. | [Build proof](build-results.md#reviewed-linker-v2-source-proof) |
| MBX-07 | Investigate compiler bypasses before controlled cache measurement. | IN PROGRESS | `unprivileged_exec_design` | Exact MBX statistics | Review 626 bypasses. Diagnostic candidate `a56a1822bbec56025fc2cd499de3a09be57c888a87ce40ba56eef70e99a3f671` is withdrawn and unexecuted. | [Build proof](build-results.md#reviewed-linker-v2-source-proof); [security review](reviews.md#main-source-compiler-image-attempt) |
| MBX-08 | Review rootfs compiler fix and filesystem evidence before another run. | IN PROGRESS | `preflight_security_review` and `unprivileged_exec_design` | Rootfs-only link, no compiler-variable exports, and stronger write evidence | Obtain exact Sol review. Do not launch another build before approval. | [MBX review](reviews.md#main-source-compiler-image-attempt) |
| PRIV-01 | Avoid credential or auth-content disclosure. | PASS | `task_records` | None | Do not read or copy auth contents. | [Debian results](debian-results.md) |
| SOURCE-01 | Leave source and `mise.lock` unchanged. | PASS | `task_records` | Documentation-only edits | Commit contains only task records. | Commit `34c32ca31e58b5e3dac71e88892778568f6f70d1` |
| BUILD-02 | Complete the initial main-source build through MBX. | PASS | `build_baseline` | Reviewed linker-v2 launcher | One build completed at the base SHA. Comparable baseline remains NOT RUN. | [Build proof](build-results.md#reviewed-linker-v2-source-proof) |
| TEST-01 | Keep tests unrun in this checkpoint. | NOT RUN | `build_baseline` | Later execution authorization | No test commands. | [Build results](build-results.md) |
| LIVE-01 | Keep provider, account, and role requests unrun. | NOT RUN | `debian_codex_route` | Binary, config, security review | No live runtime requests. | [Debian results](debian-results.md) |
| REVIEW-01 | Complete implementation acceptance review. | NOT RUN | `execution_crosscheck` | Proposed implementation | Review an exact implementation SHA. | No implementation change exists. |
| CACHE-01 | Measure cold and warm MBX cache behavior. | NOT RUN | `build_baseline` | Investigate bypass causes; security review and reviewed MBX artifact | One proof recorded zero hits and misses, but source/cache coldness and reuse were not established. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-03 | Compare an empty target with an isolated empty MBX store. | NOT RUN | `build_baseline` | Security gate and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-04 | Compare a fresh target with a warm MBX store. | NOT RUN | `build_baseline` | Security gate and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-05 | Measure unchanged source with an existing target. | NOT RUN | `build_baseline` | Security gate and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-06 | Measure a private change in one small crate. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-07 | Measure a shared account-types change. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-08 | Measure a Codex discovery or authentication change. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-09 | Measure a usage-provider change. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-10 | Measure a CLI or Console change. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| TEST-02 | Measure focused tests for the changed crate. | NOT RUN | `build_baseline` | Security gate and implementation | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-11 | Measure workspace verification. | NOT RUN | `build_baseline` | Security gate and implementation | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-12 | Measure the required release build. | NOT RUN | `build_baseline` | Release design, security gate, and implementation | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| CI-03 | Complete two consecutive compatible CI runs. | NOT RUN | `velnor_recon` and `jackin_generator_config` | Coverage gaps closed and task PR exists | Pass both consecutive runs. Repeat comparable runs at least three times. | [CI review](reviews.md#ci-workflow-review) |
| CI-04 | Track Velnor PR #55 checks at its exact head. | IN PROGRESS | `velnor_recon` | Latest page head `f17ebbc992da8549197f63d2aaaf1c317ed57426`; run `37260546503` | Prior run `37259299811` passed required jobs on pre-sync head `6baa3a1f729d45a764fd4250d1300cf17fa196e6`; current run is still running. | [Velnor PR #55](ci-coverage.md#velnor-pr-55) |
| TASK-01 | Route build, test, and lint tasks through the reviewed Mise and MBX boundary. | IN PROGRESS | `jackin_ci_consumer` and `jackin_generator_config` | Rust-MBX design and activation review | Remove the plain `cargo xtask` bypass before CI acceptance. | [Task invocation gate](branches.md#task-invocation-gate) |
| PR1108-01 | Record independent source-review dispositions for PR #1108. | PASS | `baseline_method_review` | Exact PR head `2990df17e25f30afca84804d9c402abc1ce00231` | Keep REJECT, ALREADY PRESENT, SELECT, and REPLACE scopes distinct. | [Sol review](branches.md#independent-sol-review-disposition) |
| PR1108-02 | Fix collector permissions, validation, and failure reporting. | IN PROGRESS | `jackin_ci_consumer` | Current collector owner and generated workflow | Limit `actions: read` and validation changes to collector jobs. | [Collector review](branches.md#artifact-permission) |
| PR1108-03 | Add bounded-child stdin regression coverage if the helper remains needed. | IN PROGRESS | `jackin_ci_consumer` | Current process owner and reviewed MBX | Add a one-second EOF case and preserve exit and signal results. | [Stdin review](branches.md#stdin-regression) |
| PR1108-04 | Classify unknown failures and manual dispatch outcomes correctly. | IN PROGRESS | `jackin_ci_consumer` | Current collector owner and source review | Report unsupported dispatch and missing artifacts; keep the collector advisory. | [Collector review](branches.md#independent-sol-review-disposition) |
| PR1108-05 | Port useful evidence collection to current workflow ownership. | IN PROGRESS | `jackin_ci_consumer` | Current Velnor contract and task invocation | Preserve provenance and immutable attempts; replace obsolete generator contracts. | [Sol review](branches.md#independent-sol-review-disposition) |
| TARGET-01 | Select split targets from measured variation. | IN PROGRESS | `crate_design` | Comparable build matrix | Record target thresholds after measurement. | [Crate plan](crate-plan.md); [build matrix](build-results.md#scenario-matrix) |
| CONSUMER-01 | Verify workspace crates that consume the split target. | IN PROGRESS | `crate_design` | Exact extraction candidate | Inventory all direct and indirect consumers. | [Crate plan](crate-plan.md#candidate-scope) |
| CONSUMER-02 | Preserve the `jackin` binary consumer. | IN PROGRESS | `crate_design` | Extraction design | Verify target and invocation after extraction. | [Crate plan](crate-plan.md#workspace-shape) |
| CONSUMER-03 | Preserve the `jackin-role` binary consumer. | IN PROGRESS | `crate_design` | Extraction design | Verify target and invocation after extraction. | [Crate plan](crate-plan.md#workspace-shape) |
| CONSUMER-04 | Preserve the `jackin-usage-broker` binary consumer. | IN PROGRESS | `crate_design` | Extraction design | Verify target and invocation after extraction. | [Crate plan](crate-plan.md#workspace-shape) |
| CONSUMER-05 | Preserve the `build-jackin-capsule` binary consumer. | IN PROGRESS | `crate_design` | Extraction design | Verify target and invocation after extraction. | [Crate plan](crate-plan.md#workspace-shape) |
| IFACE-01 | Preserve the CLI interface. | IN PROGRESS | `crate_design` | Extraction design | Record public commands and consumers. | [Crate plan](crate-plan.md#interface-requirements) |
| IFACE-02 | Preserve account discovery interfaces. | IN PROGRESS | `debian_codex_route` and `crate_design` | Extraction design | Record account discovery inputs and consumers. | [Crate plan](crate-plan.md#interface-requirements) |
| IFACE-03 | Preserve authentication interfaces. | IN PROGRESS | `debian_codex_route` and `crate_design` | Security review and extraction design | Record authentication inputs and consumers. | [Crate plan](crate-plan.md#interface-requirements) |
| IFACE-04 | Preserve launch interfaces. | IN PROGRESS | `architect_contract` and `crate_design` | Extraction design | Record role launch inputs and outputs. | [Crate plan](crate-plan.md#interface-requirements) |
| IFACE-05 | Preserve Console and TUI interfaces. | IN PROGRESS | `crate_design` | Extraction design | Record Console entry points and consumers. | [Crate plan](crate-plan.md#interface-requirements) |
| IFACE-06 | Preserve storage interfaces. | IN PROGRESS | `crate_design` | Extraction design | Record storage schemas and consumers. | [Crate plan](crate-plan.md#interface-requirements) |
| IFACE-07 | Preserve FFI and native interfaces. | IN PROGRESS | `crate_design` | Extraction design | Record native targets and consumers. | [Crate plan](crate-plan.md#interface-requirements) |
| IFACE-08 | Preserve release interfaces. | IN PROGRESS | `jackin_generator_config` and `crate_design` | Release design and extraction design | Record release outputs and signing boundary. | [Crate plan](crate-plan.md#interface-requirements) |
| ROLE-01 | Restart the Architect role after manifest correction. | NOT RUN | `architect_contract` | Reviewed MBX and role parser validation | Record restart outcome at the exact role head. | [Architect review](reviews.md#architect-integration-review) |
| HOST-02 | Recheck host account discovery after restart. | NOT RUN | `debian_codex_route` | Jackin runtime and role restart | Record account selection and launch state. | [Debian results](debian-results.md) |
| FINAL-01 | Complete final correctness review. | NOT RUN | `execution_crosscheck` (Sol/medium) | Exact implementation SHA, tests, and worker evidence | Not started until final changes and test evidence exist. Review exact-head correctness independently. | [Review record](reviews.md#final-review-ownership) |
| FINAL-02 | Complete final security review. | NOT RUN | `preflight_security_review` (Sol/medium) | Final execution design and artifact provenance from evidence owners | Not started until exact final design and artifact evidence exist. Clear all security gates independently. | [Security review](reviews.md#final-review-ownership) |
| FINAL-03 | Complete final build-performance review. | NOT RUN | `baseline_method_review` (Sol/medium) | Repeated measurements from `build_baseline` at exact baseline and split commits | Not started until every scenario and cache record exists. Worker measurements do not approve themselves. | [Build results](build-results.md); [review ownership](reviews.md#final-review-ownership) |
| FINAL-04 | Complete final runtime-performance review. | NOT RUN | `architect_schema_review` (Sol/medium) | Role and host evidence from `architect_contract` and `debian_codex_route` at final heads | Not started until role restart and host account recheck evidence exist. Worker evidence is not approval. | [Debian results](debian-results.md); [Architect review](reviews.md#architect-integration-review); [review ownership](reviews.md#final-review-ownership) |
| MERGE-01 | Merge only after feedback and required checks close. | NOT RUN | `coordinator` | Four final reviews and task PR | Re-fetch feedback at final head and close every thread. | [Branch record](branches.md) |
| MAIN-01 | Verify the final change on the default branch. | NOT RUN | `execution_crosscheck` | Authorized merge and final main SHA | Re-fetch main and verify the merged commit and checks. | [Final report](final-report.md) |
| CONSOL-01 | Fix and review the account-consolidation migration fixtures. | IN PROGRESS | `consolidation_review` and `execution_crosscheck` | Required predecessor, golden, and meta fixtures; verified MBX | Do not accept or merge before fixture checks and exact-head review pass. | [Consolidation review](reviews.md#account-consolidation-review) |
| CONSOL-02 | Review rule-bundle commit `c72e25d384ce2d8a80cf584457ec4b28e619980b`. | IN PROGRESS | `execution_crosscheck` | Exact dependency, equivalence, and security review | This is the next candidate; it is not selected. | [Integration gates](branches.md#fixing-and-integration-gates) |
| CONSOL-03 | Select and integrate the rule-bundle unit with provenance. | NOT RUN | `consolidation_review` | CONSOL-02 review PASS; prerequisites and reviewed MBX | If approved, preserve attribution with `git cherry-pick -x`; run unit and signed-bundle tests. | [Branch matrix](branches.md#account-and-capsule-consolidation-matrix) |
| VELNOR-01 | Record the generator design decision and remaining coverage. | IN PROGRESS | `velnor_recon` and `jackin_generator_config` | Current generator head review | Keep release signing separate and close required CI gaps. | [Velnor design review](reviews.md#velnor-design-review) |
| SNAPSHOT-01 | Complete the account database and WAL snapshot root fix. | IN PROGRESS | `omp` | Reviewed MBX activation | Source review passed; address the stale-comment follow-up and run Cargo tests. | [Snapshot follow-up](reviews.md#account-snapshot-follow-up) |
| SNAPSHOT-02 | Review the OMP WAL fix at its exact source head. | PASS | `execution_crosscheck` | Commit `1638522184ef45f0cd51fa5601a5e80c7fd89762` | Source review PASS; Cargo tests remain NOT RUN. | [Snapshot follow-up](reviews.md#account-snapshot-follow-up) |
| MD-01 | Check every local link, path, and Markdown structure. | PASS | `task_records` | Eight task records | Resolve paths and validate tables and ticks. | [Records](final-report.md) |
| GIT-01 | Commit and push documentation progress. | PASS | `task_records` | Task branch | Push without opening a PR. | Commit and branch in [final report](final-report.md) |
| PR-01 | Merge ready task PRs after every gate passes. | IN PROGRESS | `coordinator` | Final reviews, required checks, and resolved feedback for each PR | Architect and Velnor PRs are open; no merge is approved by this checkpoint. Re-fetch feedback at each final head before merge. | [Branch record](branches.md) |

## Active owners

| Owner | Area | State |
|---|---|---|
| `branches` | Branch and PR inventory | Path/disposition matrix in progress; integration pending |
| `build_baseline` | Build measurements | IN PROGRESS; measurements not run |
| `crate_design` | Crate and dependency analysis | IN PROGRESS |
| `architect_contract` | Architect manifest contract | IN PROGRESS |
| `velnor_recon` / `jackin_generator_config` | Generator and CI coverage | IN PROGRESS |
| `jackin_ci_consumer` | Current CI collector and Mise/MBX integration | IN PROGRESS |
| `consolidation_review` | Migration fixture and redaction correction | IN PROGRESS; redaction tests NOT RUN pending MBX |
| `omp` | Account database and WAL root fix | Source review PASS; stale-comment follow-up; Cargo tests NOT RUN |
| `execution_crosscheck` | Independent Sol/medium correctness reviewer | OMP and route source reviews complete; final correctness review NOT STARTED |
| `codex_schema_runtime` | Codex schema and runtime settings | IN PROGRESS |
| `jackin_cli` | Launch prompt cleanup | Source review PASS at `640b33f9598307a360484526a46c9c20bd068f4e`; Cargo tests NOT RUN pending MBX |
| `unprivileged_exec_design` | Execution boundary and MBX launcher | One build-only proof PASS; investigate bypasses and complete security validation |
| `preflight_security_review` | Sol/medium security reviewer | Preliminary review complete; final review NOT STARTED |
| `baseline_method_review` | Sol/medium build-performance reviewer | PR #1108 review complete; final build review NOT STARTED |
| `architect_schema_review` | Sol/medium runtime-performance reviewer | Final review NOT STARTED; runtime evidence pending |
| `debian_codex_route` | Debian account route | Source review PASS across `688057f40173d32dda04a55bff1e3868c219710d` and `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a`; runtime route NOT RUN |

## Records

- [Branch and PR findings](branches.md)
- [Crate and build plan](crate-plan.md)
- [Build results](build-results.md)
- [CI coverage](ci-coverage.md)
- [Debian results](debian-results.md)
- [Security and Codex review](reviews.md)
- [Initial report](final-report.md)

## Host inventory command

Captured at `2026-10-05T02:37:32+02:00` in the task worktree. The commands were read-only.

```sh
hostname -s
cat /etc/os-release
uname -sr
id -u
nproc
free -h
df -h .
```

The output reported `bastion`, Debian `13.7`, Linux `6.12.94+deb13-amd64`, UID `0`, and `96` logical processors. It reported `125Gi` total RAM and `113Gi` available RAM. It reported `3.5T` total and available disk space.

## Codex catalog command

Captured at `2026-10-05T02:38:11+02:00`. The filter printed model slugs and supported reasoning levels only.

```sh
codex --version
codex debug models | python3 -c 'import json,sys; data=json.load(sys.stdin); print("\n".join(model["slug"] + "\t" + ",".join(level["effort"] for level in model["supported_reasoning_levels"]) for model in data["models"] if model["slug"] in {"gpt-6-luna", "gpt-6.1-sol"}))'
```

Output: `codex-cli 0.160.0`; Luna supports low, medium, high, xhigh, and max. Sol also supports ultra. This catalog does not confirm active agent settings.
