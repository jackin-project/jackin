# Security and Codex Review

Status: IN PROGRESS

## Coordinator prerequisite

The coordinator model prerequisite is FAIL. Task section 2.1 explicitly supersedes older instructions and requires Luna/max. The active root process uses Sol/medium.

Work agents are tool-assigned Luna/max. Runtime confirmation remains pending. Delegation does not clear the coordinator failure.

## Codex schema and catalog

- The official [Codex configuration reference](https://developers.openai.com/codex/config-reference) defines `model_reasoning_effort` as a string.
- Supported reasoning levels depend on the selected model.
- Codex CLI is `0.160.0`. The command was `codex app-server generate-json-schema --experimental --out /tmp/codex-schema.jOAb5D`.
- The v2 bundle is `/tmp/codex-schema.jOAb5D/codex_app_server_protocol.v2.schemas.json`. `stat` reported its filesystem mtime as `2026-10-05 03:44:00.610074727 +0200`; this is not the command start time.
- The bundle SHA-256 is `e77b7d1436a78f431a74b2cb263a862e92ae40d70411bc63835b47ab2168827c`.
- `v2/ThreadStartResponse.json#/properties/model` and `v2/ThreadStartResponse.json#/properties/reasoningEffort` expose response fields.
- `v2/ThreadResumeResponse.json#/properties/model` and `v2/ThreadResumeResponse.json#/properties/reasoningEffort` expose response fields.
- `v2/ThreadStartedNotification.json#/properties/thread` refers to `#/definitions/Thread`. The bundle's `#/definitions/Thread/properties/model` and `#/definitions/Thread/properties/reasoningEffort` fields describe configured or persisted thread state.
- The v2 schema defines `ThreadResumeResponse`, but no thread-resumed notification. `TurnStartedNotification` and `TurnCompletedNotification` expose `threadId` and `turn`; `#/definitions/Turn` has no model or `reasoningEffort` fields.
- The filtered local catalog output is recorded in [checklist](checklist.md#codex-catalog-command).
- Luna supports `low`, `medium`, `high`, `xhigh`, and `max`.
- Sol supports those levels and `ultra`.
- Current root settings are Sol/medium; task section 2.1 requires Luna/max.
- Runtime confirmation of agent settings remains pending.

## Preliminary security requirements

The preliminary review sets these gates:

- Review root and unprivileged execution boundaries before builds or live accounts.
- Review the exact container and authentication route.
- Treat Docker socket access as root-equivalent.
- Do not copy the complete Codex home.
- Record MBX cache and object provenance before compilation.

The `unprivileged_exec_design` owner is still working. The preliminary security review is complete, but execution remains gated.

## Jackin redaction review

Disposition: REJECT.

The Sol review rejected Jackin redaction commit `08135f1ae010c63cec1bd7ff3a8125036a5c1576` at that exact head.

| Finding | Reported canary |
|---|---|
| `Authorization=Bearer` leaks the token suffix. | Exercise the bearer suffix case. |
| A token body in triple quotes leaks. | Exercise the triple-quoted token body. |
| A BuildKit-prefixed block scalar leaks. | Exercise the prefixed block-scalar case. |
| Interleaved BuildKit records cross stream suppression. | Interleave records across stream-suppression boundaries. |

The correction owner is `consolidation_review`. Tests remain NOT RUN pending MBX review and activation. This source rejection does not complete the final security review.

## Redaction follow-up review

Disposition: REJECT.

Sol rejected Jackin redaction commit `63d5ef9046d4948a3cddb239e891db49be654d34` at that exact head. The review reports five P1 findings:

| Finding | Required coverage |
|---|---|
| BuildKit records can interleave across stream suppression. | Cover interleaved records in both streams. |
| YAML block scalar with explicit indentation indicator `2` is not handled. | Cover the explicit indentation indicator. |
| A PEM value nested inside triple-quoted text leaks. | Cover nested PEM and triple-quote boundaries. |
| Whole-text Basic Authorization values and block scalars leak. | Cover both complete-value forms. |
| `push_line` resets per-call state. | Cover suppression state across calls. |

The correction owner is `consolidation_review`. Provide one architecture-level replacement and request exact-head Sol re-review. This review does not approve the redaction implementation.

## CI workflow review

Disposition: NOT APPROVED.

The review used Jackin base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. It compared the pinned Velnor release source `c57c700459bbe1549fe7eedcb7d8689585c38986` with Velnor main `0c40d077fcad5450521351f497ce003c69915eff`, dated `2026-10-04T23:43:56Z`.

Velnor main later moved to `ad73ae9f0500ddd02d64aad142bbecb2122c0617`. A direct `git ls-remote` check recorded it at `2026-10-05T00:39:47Z`. Recheck the review against this newer generator head.

### Accepted findings

- Generated CI lacks release, native, non-Rust, excluded Rust-test, and security-policy coverage.
- `.velnor/config.toml` excludes native, documentation, Docker, fuzz, lint, brand, build-metadata, picker, and PR-trailer paths.
- `.github/workflows/ci.yml` is 500,239 bytes. No unconditional size guard was found.
- The workflow is NOT APPROVED. Do not merge it as a completed coverage result.

### Rejected cache-writer claim

The claim that pull requests write these caches is rejected by the workflow conditions.

Cargo and Mise cache-save steps require `success()` and `github.event_name == 'push'`. See [.github/workflows/ci.yml](../../.github/workflows/ci.yml), including the Cargo save condition at line 111.

MBX uses `ACTIONS_CACHE_MODE` with `write` only for push events. Other events use `read`. The workflow shows this at lines 429, 701, and later crate jobs.

This is a static finding at Jackin base. Check current generator output before finalizing the disposition.

### Velnor design review

The Sol design review accepts a separate release and signing path. This decision does not close the required CI coverage gaps.

The review records missing native, non-Rust, excluded Rust-test, and security-policy coverage. The required coverage disposition remains IN PROGRESS. Keep release and signing gates separate from pull-request CI. Document their own checks.

The review used Velnor main `0c40d077fcad5450521351f497ce003c69915eff`. A later `git ls-remote` result reported main at `ad73ae9f0500ddd02d64aad142bbecb2122c0617` at `2026-10-05T00:39:47Z`. Recheck the design against the later head.

Before generator implementation, a later fetch recorded Velnor `origin/main` at `d9f3f3be03d67021748fd6adb4a18684d046e5e7` at `2026-10-05T01:09:24Z`. The commit timestamp was `2026-10-05T07:52:37+07:00`. The work branch was rebased onto that head before edits. Run the final generator review after upstream fixes merge.

## Account consolidation review

The account-consolidation branch at `18bc09e9536d9b662876d2fb4205357a829caa9a` contains commit `455526b92a2a4350476bb192455e5e3414f7ab9a`, titled `feat(op): persist canonical section identifiers`. Four later commits retain the fixture defect.

This commit raises the current config version from v1alpha12 to v1alpha13 and the workspace version from v1alpha10 to v1alpha11. It adds migration logic without corresponding new migration fixtures in `crates/jackin-config/src/migrations/tests.rs`.

An exact-head Sol source review at `18bc09e9536d9b662876d2fb4205357a829caa9a` confirms these fixture findings. Tests were not run.

- Config `from-v1alpha11` still targets and expects v1alpha12. Workspace `from-v1alpha9` still targets and expects v1alpha10.
- Config `from-v1alpha12` and workspace `from-v1alpha10` predecessor directories are absent. `crates/jackin-xtask/src/schema.rs:124-134` requires three fixture files in each directory.
- Migration code at `crates/jackin-config/src/migrations.rs:602,617,770` stamps the new versions. `crates/jackin/tests/migration_fixtures.rs:219-239` checks versions and exact golden contents.
- The migration changes legacy `path` data into versioned `breadcrumb` data. Fixtures must cover breadcrumb behavior and malformed-input preservation.

The reviewer labels the concrete missing-fixture failures P2. Task tracking records the consolidation finding as P1. Do not accept or merge this change until both predecessor directories and their meta and golden fixtures exist. Run migration fixtures and `schema-check` under reviewed MBX. Re-review the exact fixing commit.

Add both predecessor fixture directories. Update successful fixture metadata and goldens. Cover breadcrumb transformations and malformed-input preservation.

## Account snapshot follow-up

Sol source review PASS at Jackin commit `1638522184ef45f0cd51fa5601a5e80c7fd89762`. The change handles stale WAL suffixes after uncommitted current-generation frames and adds a focused fixture and test.

The review leaves a stale-comment follow-up. Cargo tests remain NOT RUN pending MBX activation. This source review does not report test results.

The earlier root-fix plan called for a bounded database and WAL snapshot under a source-directory pin, a committed token for WAL-only state, and provider-and-selector revalidation. Review any provider-heavy dependency before adoption.

## Architect integration review

The Architect repository is [jackin-the-architect](https://github.com/jackin-project/jackin-the-architect). Its base is `2cf461e2fed1b95d9fd1e7ba74c10d4d8b1c685d`.

PR [#479](https://github.com/jackin-project/jackin-the-architect/pull/479) is a draft on `fix/architect-role-manifest-v1alpha7`. Its exact head is `0592d0deeaeaa5b785fa67a43d23d3b627552720`.

The change updates only `jackin.role.toml`. It changes the manifest from v1alpha5 to v1alpha7 and removes six obsolete Claude and OpenCode provider tables. Static validation and exact-head Sol review PASS.

DCO, Actionlint, Plan, Required, and Sonar pass at that head. Publish baseline is skipped by its main-only condition. This is not merge approval.

`jackin-role validate`, repository validation, and the live role route remain NOT RUN. Validation awaits reviewed MBX. Maintained CI support for real roles belongs to the generator.

## MBX provenance

The activation worker reports immutable MBX `1.21.0` release commit `201b9df3d18e8e96831bee631035f6b7c7ae20e0` and GNU checksum `1ed3fd18da0decc106a6242d1b724e6a4b8d0f6173b8abe4d4d68c929ed47120`.

No local provenance transcript was saved. Artifact installation and activation remain IN PROGRESS. Security review must finish before compilation.

### Launcher preflight

Disposition: FAIL at launcher SHA `3bb10a21339c0aab3e7fc10f11ee57f97fc6d98a37e4972aa9fcac10ad6ef8c6`.

The `register-rust` phase used unsupported `/usr/bin/mount --remount,bind,ro` syntax. Installed `mount` help requires `mount -o remount,bind,ro <target>`.

The guarded namespace exited. Read-only checks found no task mount and no UID 65534 process. The script hash remained unchanged. No artifact download or build occurred.

The owner corrected this syntax in launcher SHA `1388fb22db4b61de44f3fa18fec9159ea7da57f6ab7f5c577d34068fde21d112`. Sol approved that script. The separate offline runtime failure is recorded below. MBX activation remains NOT RUN.

### Offline launcher attempt

Disposition: FAIL at Sol-reviewed script SHA `1388fb22db4b61de44f3fa18fec9159ea7da57f6ab7f5c577d34068fde21d112`.

The `register-rust` phase failed when Mise tried to resolve its version list online inside the offline namespace. No compilation or toolchain acquisition occurred.

The `unprivileged_exec_design` owner is checking supported offline `mise link` behavior. The exact command and output remain pending. Do not mark MBX activation PASS.

## Review owners

| Owner | Work | State |
|---|---|---|
| `preflight_security_review` | Initial security gate | Initial review complete; follow-up pending |
| `unprivileged_exec_design` | Execution boundary and MBX launcher | IN PROGRESS; launcher correction pending; activation NOT RUN |
| `codex_schema_runtime` | Agent settings confirmation | IN PROGRESS |
| `branches` | Fetch passed; full diff review | IN PROGRESS |
| `baseline_method_review` | PR #1108 exact-head source review | Complete; final build-performance review NOT STARTED |
| `execution_crosscheck` | Consolidation source review and final gates | OMP source review complete; route source review pending; final correctness review NOT STARTED |
| `consolidation_review` | Migration fixtures and redaction correction | IN PROGRESS; redaction tests NOT RUN pending MBX |
| `jackin_ci_consumer` | Collector and Mise/MBX integration | IN PROGRESS |
| `omp` | Account database and WAL root fix | Source review PASS at `1638522184ef45f0cd51fa5601a5e80c7fd89762`; stale-comment follow-up; Cargo tests NOT RUN |
| `debian_codex_route` | Codex account discovery route | Source review pending at `688057f40173d32dda04a55bff1e3868c219710d`; runtime route NOT RUN |
| `architect_schema_review` | Independent runtime-performance review | NOT STARTED; exact runtime evidence pending |

The `architect_manifest_fix` owner supplied the exact-head review. The `velnor_recon` owners supplied generator history and workflow evidence.

## Final review ownership

All final reviews are NOT STARTED. Each review requires final commits and complete evidence. Implementation workers provide evidence. They do not approve their own work.

| Review | Independent reviewer | Evidence producers | Status and start condition |
|---|---|---|---|
| Correctness | `execution_crosscheck` (Sol/medium) | Implementation and test owners | NOT STARTED; wait for exact final commits and test evidence. |
| Security | `preflight_security_review` (Sol/medium) | `unprivileged_exec_design` and `mbx_activation` | NOT STARTED; wait for final execution design and artifact provenance. |
| Build performance | `baseline_method_review` (Sol/medium) | `build_baseline` | NOT STARTED; wait for repeated baseline and split measurements for every scenario. |
| Runtime performance | `architect_schema_review` (Sol/medium) | `architect_contract` and `debian_codex_route` | NOT STARTED; wait for role restart and host account recheck at final heads. |

See [checklist](checklist.md), [build results](build-results.md), and [Debian results](debian-results.md).
