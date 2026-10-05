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
| MODEL-01 | Use the root model required by the latest user instruction. | PASS | `coordinator` | Root `turn_context` record | Root uses gpt-6.1-sol/medium and delegates substantive work. | [Codex review](reviews.md#coordinator-prerequisite) |
| MODEL-02 | Confirm local collaboration-tree runtime settings. | PASS | `codex_schema_runtime` | Audit of 41 local collaboration-tree sessions | Every successful session matches its assigned role; failed spawn attempts created no sessions. This does not cover unrun Jackin role sessions, provider selection, or future sessions. | [Codex review](reviews.md#coordinator-prerequisite) |
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
| ARCH-04 | Complete full Architect role validation, including marketplace preflight. | FAIL | `architect_contract` | Exact merged role content and validator rerun | The old validator run at `0592d0deeaeaa5b785fa67a43d23d3b627552720` returned 404. No post-merge full validation ran. | [Architect review](reviews.md#architect-integration-review) |
| ARCH-05 | Review the merged Architect source and feedback at the exact head. | IN PROGRESS | `execution_crosscheck` | Merge `7b72b38fe1d66e35c0931899c53bf3719592bbcc`; PR head `7db69b62f598a0971809ee4a006ad3f5477d0996` | Source is merged. The automated review summary completed after merge, so pre-merge feedback timing is not PASS. | [Architect review](reviews.md#architect-integration-review) |
| ARCH-06 | Verify the Jackin consumer fixture against the merged role source. | PASS | `jackin_ci_consumer` | Consumer commit `07f5ce7efe38c6c608fb975013df43e770d92b2b` | Source review passed; generated CI still lacks task jobs and Plan fails on the pinned generator. Tests remain NOT RUN. | [Consumer CI result](ci-coverage.md#current-jackin-consumer-check) |
| ARCH-07 | Revalidate the current Architect image against merged role source. | NOT RUN | `architect_contract` | Merged role head `7db69b62f598a0971809ee4a006ad3f5477d0996` | Image prefix `ad3b0069` predates the role content and has no source match. | [Architect review](reviews.md#architect-integration-review) |
| ACCOUNT-01 | Complete runtime account discovery through workspace launch. | IN PROGRESS | `debian_codex_route` | Jackin binary, config, security gate | CLI path checks are complete; Jackin runtime registration and launch remain NOT RUN. | [Debian results](debian-results.md#account-route) |
| ACCOUNT-02 | Review the current `CODEX_HOME` source correction. | PASS | `execution_crosscheck` | Commits `0556ce39b1abb9cd6b387583d932e1556ca9dfd4`, `688057f40173d32dda04a55bff1e3868c219710d`, and `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a` | Exact-source review PASS; Cargo tests and runtime route remain NOT RUN. | [Debian results](debian-results.md#account-route) |
| ACCOUNT-03 | Run the bounded existing-profile Codex probe. | PASS | `debian_codex_route` | Exact-reviewed v11 wrapper and notification fixture | The host probe passed; it does not prove Jackin discovery or role execution. | [Host probe](debian-results.md#bounded-host-codex-route-probe) |
| CLI-01 | Review removal of the unsupported initial prompt option. | PASS | `execution_crosscheck` | Commit `640b33f9598307a360484526a46c9c20bd068f4e` | Source review PASS. Cargo tests remain NOT RUN pending MBX. | [CLI prompt cleanup](reviews.md#cli-prompt-cleanup) |
| CLI-02 | Run focused launch and restore tests after prompt removal. | NOT RUN | `jackin_cli` | Reviewed MBX activation | Cargo tests remain NOT RUN. | [CLI prompt cleanup](reviews.md#cli-prompt-cleanup) |
| RESTORE-01 | Review identity-preserving restore source change. | PASS | `execution_crosscheck` | Commit `234fc0ea3813d8cabe579a8a8e0a0b1162eb5230` | Source review PASS; restore tests remain NOT RUN. | [Route and restore review](reviews.md#account-route-and-restore-source-reviews) |
| SEC-01 | Review execution boundary, container, auth, and cache provenance. | IN PROGRESS | `preflight_security_review` and `unprivileged_exec_design` | Exact execution design | Resolve every preliminary gate. | [Security review](reviews.md) |
| IMAGE-01 | Review and correct the tokenless BuildKit source. | IN PROGRESS | Unassigned | Commits `a67ef88d5d9889a94696d306fffcfc5249e74ceb` and `3b1a7789fe41e679eb9554e04862b6033cd82c94` | Owner reports source-only checks PASS. Exact independent review, Cargo tests, and image build remain NOT RUN. | [Source review](reviews.md#tokenless-buildkit-source-review) |
| IMAGE-02 | Review and test the corrected image-build source. | NOT RUN | Unassigned | Exact fixing commit and reviewed MBX | No correction commit or post-fix test result is recorded. | [Source review](reviews.md#tokenless-buildkit-source-review) |
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
| MBX-08 | Review measurement artifacts and output collection before more runs. | IN PROGRESS | `preflight_security_review` and `unprivileged_exec_design` | New collector artifact and exact Sol review | V6 reviews and linker verification passed; cold-1 collection failed on a hard-linked output. | [MBX review](reviews.md#measurement-launcher-reviews) |
| MBX-09 | Review the v3 measurement launcher security boundary. | FAIL | `preflight_security_review` | v3 measurement method | Review found intermediate `work/out` symlink traversal. | [MBX review](reviews.md#measurement-launcher-reviews) |
| MBX-10 | Review the v4 measurement launcher security boundary. | FAIL | `preflight_security_review` | Launcher SHA `2d1c6ce45fa163b0bfed598af3f8b6ada424c5486654132d7c675520578f27d9` | Opening a FIFO leaf can block before `fstat`; the timing method and all phases remain NOT RUN. | [MBX review](reviews.md#measurement-launcher-reviews) |
| MBX-11 | Review and run the v5 linker verification guard. | FAIL | `preflight_security_review` and `baseline_method_review` | Launcher SHA `460341e26528da42b7aae5be0449a3438868bf0fe21a2588b55bdbaad264a804` | Both artifact reviews passed. `prepare-linker` passed; `verify-linker` failed before compilation. Do not rerun v5. | [MBX review](reviews.md#measurement-launcher-reviews) |
| MBX-12 | Review and verify the v6 measurement launcher. | PASS | `preflight_security_review` and `baseline_method_review` | Launcher SHA `9040ccad259d81b4c8705c687b10fbe174a0c3ef34612cc1c055672df0d3c856` | Both reviews and `verify-linker` passed. Cold-1 collection failed before cold-2; see BUILD-13. | [MBX review](reviews.md#measurement-launcher-reviews) |
| BUILD-13 | Complete a valid v6 cold-1 measurement record. | FAIL | `unprivileged_exec_design` | Timer SHA `d3789bf2bc38616eb0b6adb594ea8c8f4a724f9af40ce973350085cf278776e9` | The inner build passed, but the collector stopped on a hard-linked Cargo HTML output before cold-2. Do not count the partial run. | [Build results](build-results.md#measurement-launcher-review-sequence) |
| PRIV-01 | Avoid credential or auth-content disclosure. | PASS | `task_records` | None | Do not read or copy auth contents. | [Debian results](debian-results.md) |
| SOURCE-01 | Keep the initial evidence-record commit separate from implementation. | PASS | `task_records` | Initial record commit | The initial record commit changed task records only; later implementation commits have separate source attribution. | [Branch inventory](branches.md#current-task-branch-source-inventory) |
| BUILD-02 | Complete the initial main-source build through MBX. | PASS | `build_baseline` | Reviewed linker-v2 launcher | One build completed at the base SHA. Comparable baseline remains NOT RUN. | [Build proof](build-results.md#reviewed-linker-v2-source-proof) |
| TEST-01 | Keep tests unrun in this checkpoint. | NOT RUN | `build_baseline` | Later execution authorization | No test commands. | [Build results](build-results.md) |
| LIVE-01 | Keep Jackin provider, account, and role requests unrun. | NOT RUN | `debian_codex_route` | Jackin binary, config, and security review | The host Codex probe does not exercise Jackin discovery, provider, workspace, or role paths. | [Debian results](debian-results.md) |
| REVIEW-01 | Complete implementation acceptance review. | NOT RUN | `execution_crosscheck` | Proposed implementation | Review an exact implementation SHA. | No implementation change exists. |
| CACHE-01 | Measure cold and warm MBX cache behavior. | NOT RUN | `build_baseline` | Investigate bypass causes; security review and reviewed MBX artifact | One proof recorded zero hits and misses, but source/cache coldness and reuse were not established. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-03 | Compare an empty target with an isolated empty MBX store. | NARROW PASS | `build_baseline` | Reviewed v8 MBX sequence | Three observations completed; contention prevents an uncontended baseline or performance claim. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-04 | Compare a fresh target with a warm MBX store. | NOT RUN | `build_baseline` | Reviewed v8 MBX sequence | One warm observation exists; three repetitions and the unexplained `aws-lc-sys` miss remain open. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-05 | Measure unchanged source with an existing target. | NARROW PASS | `build_baseline` | Reviewed v10 no-op sequence | Three no-op runs passed collection; contention and timing-report writes preclude performance or immutable-target claims. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-06 | Measure a private change in one small crate. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-07 | Measure a shared account-types change. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-08 | Measure a Codex discovery or authentication change. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-09 | Measure a usage-provider change. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-10 | Measure a CLI or Console change. | NOT RUN | `build_baseline` | Security gate, extraction, and reviewed MBX artifact | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| TEST-02 | Measure focused tests for the changed crate. | NOT RUN | `build_baseline` | Security gate and implementation | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-11 | Measure workspace verification. | NOT RUN | `build_baseline` | Security gate and implementation | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| BUILD-12 | Measure the required release build. | NOT RUN | `build_baseline` | Release design, security gate, and implementation | Run at least three repetitions. | [Build matrix](build-results.md#scenario-matrix) |
| CI-03 | Complete two consecutive compatible CI runs. | NOT RUN | `velnor_recon` and `jackin_generator_config` | Coverage gaps closed and task PR exists | Pass both consecutive runs. Repeat comparable runs at least three times. | [CI review](reviews.md#ci-workflow-review) |
| CI-04 | Track Velnor PR #55 checks at its exact head. | PASS | `velnor_recon` | Head `f17ebbc992da8549197f63d2aaaf1c317ed57426`; run `37260546503` | GitHub API reports the run succeeded and PR merged via `3ec6f32b5bafa5fa34ce9aa22afd7cfe2e797132`. | [Velnor PR #55](ci-coverage.md#velnor-pr-55) |
| CI-05 | Verify main-branch CI after the Velnor merge. | PASS | `velnor_recon` | Merge commit `3ec6f32b5bafa5fa34ce9aa22afd7cfe2e797132`; run `37261457091` | All 20 jobs succeeded. A later ref check found Velnor main at `1856b5b9f47569515c8fa00657a2c8dde6aada9f`; checks for that newer head are NOT RUN. | [Velnor PR #55](ci-coverage.md#velnor-pr-55) |
| CI-06 | Run Jackin Plan with the pinned Velnor generator. | FAIL | `jackin_generator_config` | Jackin head `41265a7550dda498c2e32126a9e68d4733215da5`; run `37262862647` | Plan rejected `tasks` as an unknown configuration field; Required failed and Rust jobs were skipped. | [Consumer CI result](ci-coverage.md#current-jackin-consumer-check) |
| CI-07 | Generate CI with all configured Jackin task jobs. | FAIL | `jackin_ci_consumer` | Jackin head `41265a7550dda498c2e32126a9e68d4733215da5` | The current generated `ci.yml` lacks the three configured task jobs. | [Consumer CI result](ci-coverage.md#current-jackin-consumer-check) |
| CI-08 | Review and validate Velnor PR #59 at its exact head. | IN PROGRESS | `architect_manifest_fix` | PR head `81e65fee08edc3b73839d9ff810cb7da6a170a65`; base `1856b5b9f47569515c8fa00657a2c8dde6aada9f` | Immutable source/archive/isolation packet and applicable review remain pending. MBX and Cargo have not run. | [Velnor PR #59](ci-coverage.md#velnor-pr-59) |
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
| CONSOL-01 | Fix and review the account-consolidation migration fixtures. | IN PROGRESS | `coordinator` and `execution_crosscheck` | Predecessor inputs are source-reviewed; generated goldens and verified MBX execution remain pending. | Source PASS at `17b2b1be`; rebake writer and migration tests have NOT RUN. | [Migration fixture checkpoint](reviews.md#current-migration-source-and-fixture-checkpoint) |
| CONSOL-02 | Select rule-bundle commit `c72e25d384ce2d8a80cf584457ec4b28e619980b` for integration. | PASS | `execution_crosscheck` | Exact source commit and three-file unit | Selected and integrated with source attribution preserved. | [Integration gates](branches.md#fixing-and-integration-gates) |
| CONSOL-03 | Integrate the rule-bundle unit with provenance. | PASS | `consolidation_review` | Destination commit `c8d20fb3a9660e1ed7819d53b3fbef410be43610` | `git cherry-pick -x` trailer names source `c72e25d384ce2d8a80cf584457ec4b28e619980b`; changed paths are `rules.rs`, `rules/tests.rs`, and `signed_bundle.rs`. | [Branch matrix](branches.md#account-and-capsule-consolidation-matrix) |
| CONSOL-04 | Run focused agent-status rule and signed-bundle tests. | NOT RUN | `build_baseline` | Controlled MBX schedule and approved Mise/MBX environment | Planned command: `mise exec -- mbx test --locked -p jackin-agent-status`. Include default, all-feature, and integration coverage as applicable. | [Branch matrix](branches.md#account-and-capsule-consolidation-matrix) |
| VELNOR-01 | Record the generator design decision and remaining coverage. | IN PROGRESS | `velnor_recon` and `jackin_generator_config` | Current generator head review | Keep release signing separate and close required CI gaps. | [Velnor design review](reviews.md#velnor-design-review) |
| SNAPSHOT-01 | Complete the account database and WAL snapshot root fix. | IN PROGRESS | `omp` | Reviewed MBX activation | Source review passed; address the stale-comment follow-up and run Cargo tests. | [Snapshot follow-up](reviews.md#account-snapshot-follow-up) |
| SNAPSHOT-02 | Review the OMP WAL fix at its exact source head. | PASS | `execution_crosscheck` | Commit `1638522184ef45f0cd51fa5601a5e80c7fd89762` | Source review PASS; Cargo tests remain NOT RUN. | [Snapshot follow-up](reviews.md#account-snapshot-follow-up) |
| MD-01 | Check every local link, path, and Markdown structure. | PASS | `task_records` | Eight task records | Resolve paths and validate tables and ticks. | [Records](final-report.md) |
| GIT-01 | Commit and push documentation progress. | PASS | `task_records` | Task branch | Push without opening a PR. | Commit and branch in [final report](final-report.md) |
| PR-01 | Merge ready task PRs after every gate passes. | IN PROGRESS | `coordinator` | Final reviews, required checks, and resolved feedback for each PR | Velnor PR #59 permission fix merged at reported main `7ccc761`; PR #65 remains draft with source PASS but failed generated-workflow CI. No Jackin task PR is accepted by these upstream results. | [CI coverage](ci-coverage.md#velnor-pr-59); [branch record](branches.md) |

## Active owners

| Owner | Area | State |
|---|---|---|
| `branches` | Branch and PR inventory | Path/disposition matrix in progress; integration pending |
| `build_baseline` | Build measurements | Narrow initial-main cold and no-op sequences recorded; no split benefit or post-integration baseline accepted |
| `crate_design` | Crate and dependency analysis | IN PROGRESS |
| `architect_contract` | Architect manifest contract | IN PROGRESS |
| `velnor_recon` / `jackin_generator_config` | Generator and CI coverage | IN PROGRESS |
| `jackin_ci_consumer` | Current CI collector and Mise/MBX integration | IN PROGRESS |
| `coordinator` / `execution_crosscheck` | Migration fixture and redaction correction | Migration source PASS at `17b2b1be`; rebake/test execution NOT RUN; redaction tests NOT RUN pending MBX |
| `omp` | Account database and WAL root fix | Source review PASS; stale-comment follow-up; Cargo tests NOT RUN |
| `execution_crosscheck` | Independent Sol/medium correctness reviewer | OMP and route source reviews complete; final correctness review NOT STARTED |
| `codex_schema_runtime` | Codex schema and runtime settings | IN PROGRESS |
| `jackin_cli` | Launch prompt cleanup | Source review PASS at `640b33f9598307a360484526a46c9c20bd068f4e`; Cargo tests NOT RUN pending MBX |
| `unprivileged_exec_design` | Execution boundary and MBX launcher | One build-only proof PASS; investigate bypasses and complete security validation |
| `preflight_security_review` | Sol/medium security reviewer | Preliminary review complete; final review NOT STARTED |
| `baseline_method_review` | Sol/medium build-performance reviewer | PR #1108 review complete; final build review NOT STARTED |
| `architect_schema_review` | Sol/medium runtime-performance reviewer | Final review NOT STARTED; runtime evidence pending |
| `debian_codex_route` | Debian account route | Source review PASS; bounded host Codex probe PASS; Jackin discovery and role route NOT RUN |

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
