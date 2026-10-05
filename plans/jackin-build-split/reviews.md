# Security and Codex Review

Status: IN PROGRESS

## Coordinator prerequisite

The root model prerequisite is PASS under the latest user `AGENTS.md`, which assigns the root session Sol/medium and requires it to delegate substantive work. The root rollout `turn_context` at `2026-10-05T02:02:06.083Z` confirms gpt-6.1-sol/medium.

The coordinator work session's `turn_context` at `2026-10-05T03:04:08.357Z` confirms Luna/max. The reviewer session's `turn_context` at `2026-10-05T03:47:30.181Z` confirms Sol/medium. The completed audit PASS covers 41 local collaboration-tree sessions: the root and 40 successful spawns. It found no unmapped successful spawn. Eight failed thread-limit attempts created no sessions. Every session's own `turn_context` matches its assigned role; inherited contexts were matched by parent turn IDs.

This audit does not establish models for unrun Jackin role sessions, provider or backend selection, or future sessions. No settings changed. Luna research sessions with names ending in `review` are not independent Sol approvals.

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
- The root session uses Sol/medium as required by the latest user instruction. The coordinator work session uses Luna/max; the independent reviewer uses Sol/medium.
- Runtime confirmation and mapping of every active agent and nested session remain pending.

### Endpoint-free Codex configuration probe

Sol rejected `codex mcp list --json --disable plugins` as a general profile preflight. Auth-status discovery can contact configured MCP endpoints. The earlier synthetic empty-home run returned zero configured servers, but it does not approve use against a real profile. No real profile or model request ran.

The route owner replaced this probe with a candidate wrapper using schema-verified app-server `config/read`. Candidate v3, `/tmp/jackin_codex_probe_review_0_160_v3.py`, SHA-256 `8e7b4ad7ea827394ce78c913e41d4c27c9e1269c72a64fbfffc1c6091d352d1b`, received an exact Sol/medium review FAIL from `model_policy_audit`. It set `experimentalApi:false` while sending experimental `environments:[]`, which the installed v0.160.0 server rejects. It also failed to inspect managed `sqlite_home` requirements, accepted configuration warnings indiscriminately, and did not verify effective sandbox, approval policy, or cwd from the thread response. Managed configuration can override `CODEX_SQLITE_HOME`, so the task-owned database path was not enforced. Candidate v4 is `/tmp/jackin_codex_probe_review_0_160_v4.py`, SHA-256 `2a7642c4d3793db6ab2014bf44f696024849e2dab128c32f5a8c1b8bd11094b0`; its exact Sol/medium review also FAILS: legacy `config.notify` remains executable even when hooks are disabled, because app-server forwards the setting and the legacy hook spawns its command. Tool-event rejection does not observe that process. The wrapper must clear and verify effective empty notify configuration, with a sentinel-command regression. `environments:[]` correctly suppresses apply_patch and shell tools; accepting the bundled `bwrap` warning is not evidence that sandboxing is absent. App-server SQLite initializes before the config-read/requirements response and may back up or reinitialize a redirected corrupt DB before rejection, so no claim of a zero-write startup boundary is justified. Candidate v5 `/tmp/jackin_codex_probe_review_0_160_v5.py`, SHA-256 `f5424346956a3d381feaac91c194d1cbe456939b3e636035a6fe91fa17c3eda8`, has a synthetic notify sentinel control/treatment fixture at SHA-256 `c6bb525218eaace7c891c2e61969abfc5b6dd1980e92b7262402fa04f700a1ea`; that local fixture passes and exact Sol review passes. The reviewer confirms notification commands are disabled and verified, empty `environments` omits apply-patch and shell tools, and earlier privacy, timeout, and cleanup gates remain. The bounded host invocation then exited 1 after 3.27 s with `BLOCKED code=unknown_notification`: the classifier accepted ChatGPT status, config/read+requirements, and thread/start model/effort/provider/cwd/approval/readOnly/path-null checks, then rejected an unallowlisted turn-phase notification. No authenticated marker response or runtime-turn PASS was recorded; no Jackin discovery/role proof occurred. App-server SQLite initialization and managed auth/config handling occur before its config-read/requirements response and may write task-owned DB state before a mismatch aborts; do not claim a zero-write startup boundary. The server was stopped and task temporary captures removed; no auth/config/session deletion or edits occurred. No unchanged retry is authorized. The host invocation is not a passing authenticated request. Host inspection confirmed `CODEX_HOME` is unset/empty; no `auth.json` contents were read. Source review of `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a` distinguishes ambient `CODEX_HOME` used for default account import from registered-account routing through the saved profile path and scoped launch environment; broker refresh is a direct read-only GET, while the CLI can refresh its container copy. This is source behavior, not a live account or provider result.

## Preliminary security requirements

The preliminary review sets these gates:

- Review root and unprivileged execution boundaries before builds or live accounts.
- Review the exact container and authentication route.
- Treat Docker socket access as root-equivalent.
- Do not copy the complete Codex home.
- Record MBX cache and object provenance before compilation.

The preliminary security review is complete. Final security approval remains pending. One Sol-reviewed build-only proof was separately authorized. See [MBX execution evidence](#main-source-compiler-image-attempt).

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

### Nested PEM and current correction

Sol rejected `69b82de1a48cc18add3933e1995028c8aa2722e8` at its exact head. The nested `PRIVATE KEY` and `RSA PRIVATE KEY` marker sequence leaked `nested-pem-canary` in whole text, streaming output, and build-log snapshots.

Commit `c2a80a7dc0618007840533e956b06f4af3458a70` fixed the nested structure but failed exact-head review. Two `map_or` calls at `redact.rs:950-952` and `975-977` trigger Clippy's denied `unnecessary_map_or` lint. Sol also requested a positive sequential marker-block regression.

Follow-up `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578` replaces those calls and adds sequential marker-block tests for whole-text and `StreamRedactor` paths. Exact-head Sol source review PASS. The marker fixture does not test cryptographic PEM parsing. Pinned rustfmt and `git diff --check` passed. Clippy and Cargo tests remain NOT RUN. Runtime and test acceptance remain pending.

Earlier source-revision transcriptions contained an incorrect final character and a malformed 39-character route reference. Neither is a source revision. The corrected redaction commit `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578` and route commit `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a` resolve as local commit objects. Validate each full SHA as 40 hexadecimal characters and a resolvable local or remote object. Final review must revalidate source claims.

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

The separate optional setup-factoring proposal used Velnor main `6180ccebc7eff8b8f40f988eea2cf948bb235c9d` and PR head `d10472a32b227e98ac09180feba0ca6f8899ccf8`. Its design disposition is FAIL pending redesign. Extraction would hide `steps.mbx.outputs` and `steps.mbx-bundle.outputs` from outer export/save logic in `mbx_bundle.rs`. A composite has no `id` or outputs. `document.rs:219-260` also derives credential scrubbing, `RUSTUP_TOOLCHAIN`, and acquisition provenance from remaining steps. Compute these properties from original steps. These are proposal findings, not defects in PR #55.

Hosted jobs use an unqualified shell. Scale-set jobs use `bash -e {0}`. Explicit Bash can change `pipefail` behavior.

Preserve metadata and environment. Preserve expanded steps and external `steps.*` references. Preserve cache election and pull-request versus push trust. Check out before using a local action. Pin action versions immutably. Use fixed safe paths. Avoid token interpolation. Preserve Required IDs, dependencies, and verdicts. Reject renderer drift. Test the full 27-Rust and 3-verification graph repeatedly under the byte cap.

The 375–380 KB estimate is not measured. No generator code or tests passed this design review.

### Velnor PR #55 source security review

Sol source review PASS at PR head `6baa3a1f729d45a764fd4250d1300cf17fa196e6` against main `6180ccebc7eff8b8f40f988eea2cf948bb235c9d`. The review accepts fixes for typed-job Mise environment true/unset handling, a workflow-wide byte guard, UTF-8 accounting, and no-partial/in-place tests. It rejects the untrusted-cache-writer allegation because cache saves are push-only.

At the time of this source review, general and issue-comment dispositions, P13, T24, and required checks remained pending. The GitHub API later reported PR #55 merged at head `f17ebbc992da8549197f63d2aaaf1c317ed57426` via `3ec6f32b5bafa5fa34ce9aa22afd7cfe2e797132`; the PR run succeeded. The merge-commit check-runs query showed 17 successes and one orchestrator check in progress. See [CI coverage](ci-coverage.md#velnor-pr-55).

## Account consolidation review

The account-consolidation branch at `18bc09e9536d9b662876d2fb4205357a829caa9a` contains commit `455526b92a2a4350476bb192455e5e3414f7ab9a`, titled `feat(op): persist canonical section identifiers`; its tree is `759bd5ed12499604cf43d52a01fa252f430d2318`, parent `78e38612824c4e6d69ffb9d01a834a75537e7c5a` has tree `787ab923816b655aa69ce7573329b5cbfab07091`. Four later commits retain the fixture defect.

This commit raises the config version from v1alpha12 to v1alpha13 and the workspace version from v1alpha10 to v1alpha11. Its 37-file feature delta adds migration and OpRef consumers without updating the migration fixtures under `crates/jackin/tests/fixtures/migrations/{config,workspace}`.

An exact-head Sol/medium source review by `baseline_method_review` compared commit `455526b92a2a4350476bb192455e5e3414f7ab9a` against its parent and current Jackin source head `3b1a7789fe41e679eb9554e04862b6033cd82c94`; current task head `114effca63bb5c4ce83f8bd56d80187e800cd5d8` has tree `bbaba873ad1f960737038c254a455027053ffb28` and adds documentation only. The review blocks merging the whole commit. Its behavior is SELECT, but the patch must be REPLACED as a complete current-source port with fixtures and consumer tests. No code was edited and no tests/build ran.

The Luna source audit confirmed this is not currently integrated: the current task branch has no `OpSectionTarget`, breadcrumb codec, or v1alpha13/v1alpha11 schema bump. Its config and workspace versions remain v1alpha12 and v1alpha10. Therefore the reported migration-fixture break is a defect in the candidate branch, not a failure on the current task branch.

- In commit `455526b92a2a4350476bb192455e5e3414f7ab9a`, config `from-v1alpha11` still targets and expects v1alpha12. Workspace `from-v1alpha9` still targets and expects v1alpha10.
- In that commit, config `from-v1alpha12` and workspace `from-v1alpha10` predecessor directories are absent. `crates/jackin-xtask/src/schema.rs:124-134` requires three fixture files in each directory.
- Migration code at `crates/jackin-config/src/migrations.rs:602,617,770` stamps the new versions. In commit `455526b…`, config `from-v1alpha11/meta.toml` and `after.toml` still target v1alpha12; workspace `from-v1alpha9` counterparts still target v1alpha10. `crates/jackin/tests/migration_fixtures.rs:216-224` checks resulting versions and `:236-240` checks exact golden contents.
- The migration changes legacy `path` data into versioned `breadcrumb` data. Fixtures must cover breadcrumb behavior and malformed-input preservation.
- The exact source audit found `crates/jackin-config/src/editor.rs` manually writes `{breadcrumb={version=1,value=r.path}}` in `ConfigEditor::set_env_var` instead of using the new typed codec. The reviewer initially raised a validation concern, then corrected it: `ConfigEditor::save_with_stager` at `editor.rs:793-805` runs `validate_candidate`, reparsing the versioned TOML through `OpRef`'s deserializer before staging or writing. This is duplicated encoding/maintainability work, not a demonstrated invalid-persistence defect; do not report it as a blocker.
- `crates/jackin-env/src/picker.rs:357-360,376-380` selects the first `FieldTarget::New` by section ID and label, then overwrites it. Independent Sol review found this first-match behavior already existed at the parent, so do not attribute it to `455526b`; fix it as part of the operation-ID consumer port by rejecting ambiguous metadata before mutation.

The reviewer labels the concrete missing-fixture failures P2. Task tracking records the consolidation finding as P1. Do not accept or merge this change until both predecessor directories and their meta and golden fixtures exist. The migration test target is `jackin --test migration_fixtures`; run it through approved `mise exec -- mbx test --locked`. Also run the project `xtask schema-check --base <base>` gate through MBX. No fixture rebake generator was identified; the exact rebake invocation remains undocumented and must be established under the approved MBX path rather than guessed. Re-review the exact fixing commit.

The useful source hunks are coupled: schema-version bumps and recursive migration (`crates/jackin-config/src/versions.rs`, `migrations.rs`); canonical ID URI construction and escaped/versioned breadcrumb encoding (`op_reference.rs`, `env_value.rs`); item section metadata and cache types (`op_types.rs`, `op_cache.rs`); OpRef producer, selection, display, and resolution consumers in `jackin-env`, `jackin-oppicker`, console, runtime, and diagnostics. Port them together, preserve opaque section metadata and stable IDs, route public writes through the typed codec, and reject ambiguous legacy matches before mutation. Add immediate-predecessor and historical fixtures, breadcrumb conversion, ambiguity and malformed-input no-write regressions. The supported test target is `jackin --test migration_fixtures`, run using the approved `mise exec -- mbx test --locked` path; run `xtask schema-check --base <base>` through MBX as well. No fixture rebake generator was identified; the exact rebake invocation remains undocumented and must be established under the approved MBX path rather than guessed. `PRERELEASE.md` requires predecessor fixtures and re-baking historical `after.toml` files. Do not cherry-pick only the schema declaration or accept 455 as merge-ready.

Add both predecessor fixture directories. Update successful fixture metadata and goldens. Cover breadcrumb transformations and malformed-input preservation.

### Current migration source and fixture checkpoint

The selected `455526b92a2a4350476bb192455e5e3414f7ab9a` behavior is now ported on `refactor/build-split` as a coherent current-source change. Migration commits `20cb8f7054ef6322c653b5c92bec7ad8f826d810` and `17b2b1be6a58a0e34af6d8308df915d110f4a785` have exact-source Sol review PASS. The migration correction routes legacy and versioned OpRefs through the same strict validator before writes or version stamping. The new source fixtures reject malformed inputs without changing their bytes.

The two required immediate-predecessor directories are committed. Their `before.toml` and `meta.toml` files use controlled four-segment OpRef values with section metadata, percent escapes, query data, and non-secret account and on-demand fields. Commit `17b2b1be6a58a0e34af6d8308df915d110f4a785` also makes the rebake test fail if either input directory is missing. Exact source review confirms the output checks require a real legacy-to-versioned transformation. The archive does not include generated `after.toml` output for these predecessors, and no generated golden has been accepted.

The ignored rebake writer is `crates/jackin/tests/support/migration_fixture_rebake.rs`. Its source review accepts execution only beneath a task-exclusive private output parent. The MBX owner is preparing a separate 1.22.0-bound sandbox. The rebake writer, migration tests, schema checks, Rust compilation, Clippy, and generated-golden review remain NOT RUN.

The source archive is `/root/.velnor-work/jackin-migration-17b2b1b/source.tar.gz` with SHA-256 `97c2f22da3425734086bb3445778674c7d9e54d236c9705702824c2c562091da`. The execution packet is `/root/.velnor-work/jackin-migration-17b2b1b/execution-packet.md` with SHA-256 `550f7f050e218666511345ae3a172de8478da1ed8ac92a3543bbdd11614dade3`.

The intended focused invocation is `JACKIN_MIGRATION_FIXTURE_OUTPUT_DIR=/work/out/migration-fixtures mise exec --locked -- mbx test --locked --offline -p jackin --test migration_fixtures rebake_migration_fixtures_to_output_dir -- --ignored`. It must run only after the exact sandbox launcher passes independent review. Keep generated output separate until it has a source and byte-level review.

### Exact-source compile correction and Mise wrapper checkpoint

The current task branch includes migration source commit `20cb8f7054ef6322c653b5c92bec7ad8f826d810` and required predecessor-input commit `17b2b1be6a58a0e34af6d8308df915d110f4a785`. The first focused test attempt used that source and stopped during compilation, before tests: `jackin-config/src/migrations.rs` called `toml_edit::de::from_document` while the locked `toml_edit` dependency lacked its `serde` feature, and the qualified `toml_edit::DocumentMut` use violated `unused_qualifications`. This is a compile failure, not a migration-test failure. The exact failed log SHA-256 is `281939521ad9b69fc322f829bfe4f5abc7b211b6cc7a71cef32b544905a13f60`; the execution record SHA-256 is `1b574138267eaa9fd904824698fee0907d9de9fea34e7114d4937f5adaa4e038`.

Commit `b184081fffde49ed57d72b593d1fe8311435446e` enables `toml_edit/serde` for the existing locked dependency and uses the already imported `DocumentMut`. Exact-source Sol review passed. The strict canonical `jackin_core::EnvValue` decode and byte-preserving no-write migration regressions remain in place. This review does not establish compilation or test execution.

Commit `091bbae649ba663ce8a77239938ac31493eb7eda` adds a Mise Cargo command wrapper using `mbx` and `MBX_CARGO_SHIM_MODE=1`, plus a task-contract assertion. Mise 2026.10.1 accepts the wrapper configuration. The independent source review checked the pinned Mise and MBX dispatch paths and passed, with scope limited to source behavior. The added contract assertion does not establish wrapper argv, exit-status, environment, session reuse, or process-cleanup behavior at runtime. The `debian_codex_route` owner is preparing one compiler-free fake-command harness for those behaviors; its runtime result is pending and no second harness is authorized.

The exact current-source archive is `/root/.velnor-work/jackin-source-b184081-20261005/jackin-b184081fffde49ed57d72b593d1fe8311435446e.tar.gz`, SHA-256 `2bcd0b0d77eb81d81aec7e4b4b17974e3de96ad8e369ae1ae4df7ca9d8a0407a`; the matching private execution packet is `source-packet-v2.md`, SHA-256 `ef05a36adfed08d56eb4891ed208fb2cdd11417ca06918602acb1529bfab6ba2`. These bind the focused invocation `mise exec -- mbx test --locked --offline -p jackin-config -p jackin-xtask`. The reviewed source correction has not yet been tested: the b184-bound MBX rebind and wrapper harness require their exact independent source/security reviews before the next run. The failed 17b run is preserved and will not be retried against changed source.

The first current-source MBX 1.22.0 sandbox registration attempt used source `17b2b1be6a58a0e34af6d8308df915d110f4a785` / tree `0e8921d2f01482b4e8b03e68bc9f060216394884` and launcher SHA-256 `da3145114114a6b5dfdb581f7ba5a21f6efd256bf4a639a22076a3b075d059ad`. Its `register-tools` phase stopped with exit 1 at `2026-10-05T10:46:05Z`–`10:46:06Z` on `unsafe Rustup proxy directory`; the launcher used umask `077`, so creating that directory with mode `0755` yielded `0700` and failed its mode check. The private log SHA-256 is `f6c71321f2a202f1c4eb398a081460b612a5dc5f750d0fa2c7402bdc48574d5d`; execution-record SHA-256 is `b95cdca673a0d40c4aac9cc40b86598c8c94c00627ef3f348858ac1a1bfd497e`. Only an empty root-owned proxy directory was created in the task rootfs. No namespace, Rustup, Mise, compiler, or Jackin binary ran, so this is a launcher registration failure, not a source build result.

The v7 candidate's exact launcher/helper/diff hashes are `e3be0d468ef159f1430d781a82333116640d96ee4f5d45c9497f722b2e40c5f8`, `2a330e7cf670917b95d4e4a3745df95bad0f1a24c5964e856b3bb504721ec9f8`, and `e89eeee8d77a3645c19fb415f4c20c7cacd7e755ef789006db988afdf690a355`; its local fixture record SHA-256 is `1707d9c4b2585960576a7d5ae34aa8df7c0f15028e2de163de44cb3166e06e56`. Exact v7 review passed and the harmless temporary fixture passed. The `register-tools` phase then exited 1 at `mise which rustc` (`mise ERROR rustc is not a mise bin`), after Rustup proxy preparation, seven aliases, Rustup Home validation, and Mise version/path checks. It ran 2026-10-05T10:58:20.638714Z–10:58:21.230619Z. The private log SHA-256 is `64832c0f237b8932b36ffc14fa6363138aa1896496c1a146857d6640c3519b0a`; execution JSON SHA-256 is `63e95bc2f121d4fd1169b6b28068f0164eac69dfb8ba42d4c22b32a9f5f35497`. No Cargo compile, test, writer, or binary handoff occurred. The owner is diagnosing Mise bin registration; no unchanged retry is approved.

## Account snapshot follow-up

Sol source review PASS at Jackin commit `1638522184ef45f0cd51fa5601a5e80c7fd89762`. The change handles stale WAL suffixes after uncommitted current-generation frames and adds a focused fixture and test.

The review leaves a stale-comment follow-up. Cargo tests remain NOT RUN pending MBX activation. This source review does not report test results.

The earlier root-fix plan called for a bounded database and WAL snapshot under a source-directory pin, a committed token for WAL-only state, and provider-and-selector revalidation. Review any provider-heavy dependency before adoption.

## Architect integration review

The Architect repository is [jackin-the-architect](https://github.com/jackin-project/jackin-the-architect). Its base is `2cf461e2fed1b95d9fd1e7ba74c10d4d8b1c685d`.

PR [#479](https://github.com/jackin-project/jackin-the-architect/pull/479) is a draft on `fix/architect-role-manifest-v1alpha7`. Its exact head is `0592d0deeaeaa5b785fa67a43d23d3b627552720`.

The change updates only `jackin.role.toml`. It changes the manifest from v1alpha5 to v1alpha7 and removes six obsolete Claude and OpenCode provider tables. Static validation and exact-head Sol review PASS.

DCO, Actionlint, Plan, Required, and Sonar pass at that head. Publish baseline is skipped by its main-only condition. This is not merge approval.

Parser and local repository contract checks ran with MBX-built binary SHA-256 `e899a8e5f51ebb5f20fce5a379a3f4de4555911549e743ca625efb4a3988c2ac`, built from Jackin base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. `jackin role validate --help` exited 0. `jackin role validate /tmp/architect-contract-base-DMcKHD` exited 1 as expected: the v1alpha5 fixture manifest SHA-256 `52ca2ec67a40888c0da73e738d69ede8fb18f4efc952e798e4f335ea153db135` rejects `providers` as an unknown field; it allows `model`, `marketplaces`, and `plugins`.

The current role manifest SHA-256 is `eb08cf89aa32971c17db9875ec633ac22fe609927abf23fd18182756819e7fca`. `validate_role_repo` returned success after local strict-manifest, Dockerfile, and hook-structure checks at PR head `0592d0deeaeaa5b785fa67a43d23d3b627552720`. Full `jackin role validate /root/Projects/tailrocks/jackin-project/jackin-the-architect-task` exited 1 after an unauthenticated GET of `https://raw.githubusercontent.com/tailrocks/tailrocks-skills/HEAD/.claude-plugin/marketplace.json` returned 404. Sanitized stderr: `fetching Claude marketplace manifest from https://raw.githubusercontent.com/tailrocks/tailrocks-skills/HEAD/.claude-plugin/marketplace.json returned 404 Not Found`. Stdout contained only a redacted telemetry invocation line; it did not print `Role repository is valid`. No authentication, container, hook, or role load occurred. Parser and local contract checks PASS; full validation FAIL on that head.

Read-only GitHub metadata later confirmed that [`tailrocks/asd-ste100-skill`](https://github.com/tailrocks/asd-ste100-skill) is a separate private repository at main `0572839a13afc145083535b3e29c79b78489af40`. It contains `.claude-plugin/plugin.json` and `.codex-plugin/plugin.json`, but no `.claude-plugin/marketplace.json`; it is not a marketplace substitute. The public [`tailrocks-skills`](https://github.com/tailrocks/tailrocks-skills) README lists eight independent repositories. Sol's design review PASS recommends only public `tailrocks-rust-skills` and `tailrocks-roadmap-skills` as marketplace sources matching the role's advertised capabilities. It does not claim a one-to-one replacement for the prior eight-repository set.

Architect owner reports marketplace correction commit `7db69b62f598a0971809ee4a006ad3f5477d0996`. It later merged as `7b72b38fe1d66e35c0931899c53bf3719592bbcc`. The earlier full-validator 404 remains the latest role-validation result. See the merged-source and consumer status below.

The command runs used task-private environment paths, working directory `/`, and disabled OpenTelemetry. The available binary predates the Architect manifest update. Live role loading remains NOT RUN. Maintained CI support for actual roles belongs to the generator.

### Merged Architect and consumer status

Architect PR #479 is closed and merged. PR head `7db69b62f598a0971809ee4a006ad3f5477d0996` merged as `7b72b38fe1d66e35c0931899c53bf3719592bbcc` at `2026-10-05T04:10:07Z`. The automated Codex summary completed at `04:11:05Z`, after merge. Pre-merge feedback timing is therefore NOT PASS.

Post-merge Sonar check run `111612018729` failed at the merge SHA. The reported S8264 finding concerns workflow-level `actions: read`. The PR merged despite this later failed main check; record it as an unresolved main-quality finding.

Jackin consumer source review PASS at `07f5ce7efe38c6c608fb975013df43e770d92b2b`. It updates the immutable role snapshot to manifest SHA-256 `b38e506587c98137d0a1a88247fb68afc9f9f215c8104c838df251933a917ae5`. The subsequent Plan run fails against pinned Velnor `0.1.0`; generated CI still lacks configured task jobs. Consumer tests remain NOT RUN.

Architect image prefix `ad3b0069` predates the merged role content and has no source match. No live Codex profile probe, role request, or role launch ran. Codex environment evidence is synthetic-fixture-only.

### Maintained Architect image installer source review (PR #480)

PR [#480](https://github.com/jackin-project/jackin-the-architect/pull/480) is open and draft at source commit `03482411fd5f1ef55c61bb1e2265f198dca3434b`, tree `57070227c97a4f11ef357dd0a471896fe7ea47aa`, against main `7b72b38fe1d66e35c0931899c53bf3719592bbcc`. Exact Sol source review PASS covers the complete Dockerfile installer update and its maintained npm build-only project. GitHub checks at this head show DCO, Actionlint, Plan, Required, and SonarCloud success; Publish baseline is skipped by its main-only condition.

The build-only package pins `ctx7@0.5.11` and `skills@1.5.22`. Its lock is npm lockfile v3 with 89 dependency entries; all entries have registry integrity metadata, and none declare `hasInstallScript`. The package manifest SHA-256 is `8e4644c137635eee9942e08d36459f4bcfc635182eb834fd9d798f689016c2ae`; lock SHA-256 is `b5f6a5fa0a1432d3f2c1e20327a1d0fa0ab9370fb7f71bbd1ad9b7124890cb6f`. The owner-reported lock-generation record used Node `24.21.0` and npm `11.19.0` with `--package-lock-only --ignore-scripts --no-audit --no-fund`; it reports no `node_modules` directory and no package lifecycle scripts. The recorded `npm` command entrypoint was the Mise Bash wrapper (SHA-256 `0a6f43cb58b81269aad0d3eff77f68b5d06ea7c241c199a63bee8b0dd337effc`); the bundled `lib/node_modules/npm/bin/npm-cli.js` hash was `8e5f6f3429f8cdbe693cdc29904e9d5a7b127a494bd15c804bd54c7403bfcbe7`. Node archive SHA-256 is `6e1db87ef58b8819e5d5402eff1536491b18edd8eb7bee5ef7897876e88dc5ff`.

The `ctx7@0.5.11` registry tarball SHA-256 and SRI match the captured bytes. Its embedded source map matches all 31 source files at upstream tag commit `d304ff2c0880110c0b53c8e1e4d4c664feae1e5e`. The tag is unsigned and npm `gitHead` is null; no signature or registry attestation is claimed. The corrected private audit revision is `installer-source-audit.rev1.md`, SHA-256 `b790c63a6a155d8654bc7f882d93275ecd625c83023c6573a91fe440a685d340`; it preserves the original packet and fixes only the transcribed `skills` SRI. The separate ctx7 supplement is SHA-256 `058063c4ae763a6bad8fcb3c42249bf251a2f97b8223de7c41888005199dc093`. The Node/npm lock provenance packet is SHA-256 `27413ecedb649fdc4f9e5e5cca569a8e33f8f2a98cd981526df41319e071ecf1`.

This is source and lock-generation evidence only. Docker image construction, `npm ci`, CLI invocation, role loading, and role execution remain NOT RUN. The CI success does not establish image behavior.

The PR has since advanced to `54cd3d29f99753487a2ac0373483ebc7047831ac`, tree `18a2c5892b9eef275929a1ad6c3d3d55e3f88037`, with the same base; its only changed path from `0348241` is `Dockerfile`. The exact current-head Sol source review also passed. Run `37298705299` reports DCO, Actionlint, Plan, Required, and SonarCloud SUCCESS, with Publish baseline SKIPPED by its main-only condition. This re-review covers the Dockerfile delta; it does not add image-build or runtime evidence.

### Velnor PR #59 permissions follow-up

Exact Sol/medium source review PASS at remote PR head `81e65fee08edc3b73839d9ff810cb7da6a170a65`, tree `4162acc9850461784105c6ce7d3d322df0c05c9d`, against then-current main `1856b5b9f47569515c8fa00657a2c8dde6aada9f`. It scoped `actions:read` to the Required artifact consumer and left the Plan baseline lookup tokenless as a separately documented gap. The PR's generated Plan run and then Required failed; Rust jobs and publish-baseline were skipped. This source PASS does not cover newer main or the current local merge state.

Velnor main advanced to `c4fc31efd2fbb39b7cfc2cce423d99b7c4733c3d`. The remote PR ref remains `81e65fee08edc3b73839d9ff810cb7da6a170a65` with its old `1856b5b9f47569515c8fa00657a2c8dde6aada9f` base. A shared local worktree is at merge commit `c94b51280d3c877c575893591fb53d3b92fe0bd7`, but contains six uncommitted root-owned files (diff SHA-256 `4a3329b57aa38e68d1169042f801b10d4b1701ee83a36a15f8a3f656ae5e5bf1`). The changes add Plan baseline authentication/permission bindings and update related docs; no task worker or read-only research owner claims them, and Git metadata does not identify the actor. Preserve them; do not stage, reset, copy, or attribute them to the merge author. Re-evaluate only an immutable fresh snapshot from the remote PR and exact current main after the full delta and Plan permission changes receive review. No build or generation ran from this local merge/worktree.

### Measurement launcher reviews

The v3 timing method passed review. The security review failed because an intermediate `work/out` path permits symlink traversal. Do not use it.

The frozen v4 launcher SHA-256 is `2d1c6ce45fa163b0bfed598af3f8b6ada424c5486654132d7c675520578f27d9`. Its timer SHA-256 is `fbae0a31d6ba3138153ee64398082203f15446593a74d846228d28176cba1e1b`; diff SHA-256 is `26c9a2a7c8edc6e0cfc29a06cb5c8cc596b5259c329112504f56715891e7f81b`. Sol security review FAIL: opening a FIFO leaf with `O_RDONLY` can block before `fstat`. Ancestor traversal was fixed. The timing method was NOT RUN, and no phase ran.

The v5 launcher SHA-256 is `460341e26528da42b7aae5be0449a3438868bf0fe21a2588b55bdbaad264a804`. Its timer SHA-256 is `31560b177eb620665dc91ef28c5bf44aac42c6e024223a85f827542d0d675e08`. Its diff SHA-256 is `caa6ecf17631048f2dad8d099bce60565d9915054185ad5ba004bbc0b14416a3`. Security and measurement-method reviews passed these exact artifacts.

The v5 `prepare-linker` phase passed in 102.038 ms using the rootfs-only GCC link. `verify-linker` failed before compilation. It matched the resolved compiler path and hash, but expected the target basename at the start of version output. No source compiled. Do not rerun v5.

The v6 launcher SHA-256 is `9040ccad259d81b4c8705c687b10fbe174a0c3ef34612cc1c055672df0d3c856`; timer SHA-256 is `d3789bf2bc38616eb0b6adb594ea8c8f4a724f9af40ce973350085cf278776e9`; diff SHA-256 is `f75ef4f90c0c842e8120465f75d420890df7ab37173759131532f2e2867f0547`. Security and method reviews passed. `verify-linker` passed. The cold-1 inner build exited 0 in 90.373 seconds, but collection failed before cold-2. A Cargo 1.97 HTML output had `st_nlink=2` because its canonical and timestamped names were hard links. The collector stopped before further measurements. The owner salvaged MBX statistics with a validated no-follow directory descriptor. The partial run is not a cache or performance result. A collector correction requires new review. See [build results](build-results.md#measurement-launcher-review-sequence).

The v7 launcher, timer, and diff hashes are `f3e467ab103ef81879b072923725d6ae4153fe85bf6b6cbf07103eae2e068baa`, `14e82441448fe5ad53fddb0ed63170fccb6e3272f900cab0f80cbd02eba73087`, and `8062e8fa4b3c41e41b4e9b75e7b6f9093008c554c8fc935b734a00a8e88c0051`. Exact security review by `preflight_security_review` and method review by `baseline_method_review` both passed. The first recovery invocation stopped before entering the read-only recovery phase: the launcher was 70,659 bytes while its self-reexec guard rejects files above 65,536 bytes. The timer recorded child exit 1 after 11.081 ms with `unsafe launcher file`; no build or target/cache/mount mutation occurred.

The size-guard correction is v8: launcher `45291b393424ab976f8181b8001e08ce3210014d0666ced45d179db7705a37ca`, timer `40c60d10c411d0c8fae2bdd7c3c1449ae7c9d29520f44cdafc71ae9239c0f554`, diff `d0271787510a02fb9dc44346d3e3494e2df8c3f206dbe0f4ee3f365ce27b77c2`. Exact security and method re-reviews passed. The single authorized read-only recovery then succeeded without Cargo; it recovered the original timing and statistics and did not create a new build repetition. The recovered timing HTML is 540,739 bytes, SHA-256 `ef933ce66183a18a84b824b9526681e5deef15961569fc4eea1c96ea88e1f240`. No additional matrix run occurred during recovery. The cold-1 sample remains contended and is not an uncontended median or cache-reuse result.

The method review classifies the original cold-1 sample as contended because unrelated Rust compilers were active. Its successful inner build remains a single contended observation, not an uncontended median or comparable scenario. At least three comparable cold repetitions are still required; add a fresh target/store repetition if contention differs or variance obscures the target.

## Tokenless BuildKit source review

Commit `a67ef88d5d9889a94696d306fffcfc5249e74ceb` failed exact review. The commit changed image building to stop forwarding ambient GitHub tokens to BuildKit.

Follow-up commit `3b1a7789fe41e679eb9554e04862b6033cd82c94` removes the obsolete detector API, re-export, and stale test. It also gates Unix-only imports. Exact Sol/medium source review by `execution_crosscheck` PASS: the four intended paths are limited to detector removal, Unix import gating, and updated launch assertion. Old names remain only in negative source-contract assertions. `git diff --check` and owner-reported source searches passed. Rust tests, Clippy, Windows compilation, and real image builds remain NOT RUN. Keep final acceptance pending.

## Velnor release-manifest helper source review

The pure helper commit `63a3a0783de72dac9f02d55e1c56346c83d9d2f5`, tree `5db4957559ada8b75d1f493dafa3e04fad009846`, is on the separate Velnor branch `fix/generator-release-provenance`, based on main `1856b5b9f47569515c8fa00657a2c8dde6aada9f`. Owner-reported Python fixtures passed the valid manifest case and rejection cases for missing/wrong URL, target set, digest, upload state, and unsafe path; Rust tests are NOT RUN.

Exact Sol/medium review by `execution_crosscheck` found a P2 in `63a3`: `urlsplit` normalized leading spaces and uppercase `HTTPS`, allowing output that Jackin's `ReleaseManifest` validator rejects. Publisher commit `b495e8e0375ad18b94590d09723c1c4a49ae6730` fixed that rule but failed exact review because of a duplicate `RELEASE_VERSION` import, a quoted-source assertion defect, and draft lookup through the published-only release-by-tag endpoint; fake GH fixtures concealed the real draft/tag flow. Follow-up `05d054e8e043832f8ab4889b32db67eb5bdf04dc` fixed those source findings and normally merged then-current main, but still exceeded the 400-line file cap and relied on an Administration:read immutable-setting preflight unavailable to its `contents:write` GITHUB_TOKEN. A read-only existing-account query returned `enabled=true, enforced_by_owner=false`; no setting changed. The current publisher candidate is reported as `ececed021aac561c4fca90c638bb74318bdae437`, based on stale Velnor main; current main is `bcd2fd3ef612dc7e22a7e21d6eed3504bebc1642`. This candidate still performs the Administration:read preflight and uploads the manifest without a checksum sidecar. The Sol lifecycle design review is conditional PASS for consumer acceptance only: require a complete draft sequence (upload binary and checksum assets, bind release ID/source tag and verify observed URLs/digests/sizes, derive manifest only from observed draft assets, upload manifest plus checksum sidecar while draft, reread exact draft inventory/bytes, publish once, then reread the same release ID and require `immutable=true`, published state, source/tag, exact full asset inventory, URLs/digests/sizes and byte-identical manifest/sidecar). Only after verification may a separate downstream artifact/pin proposal be emitted; never auto-edit consumer config or emit provisional metadata. If post-publication checks fail, leave the candidate rejected and emit no consumer metadata. This cannot promise that no mutable candidate was ever briefly published; if that is required, block until a supported enforceable policy capability exists. The host-admin setting query is not CI publisher authentication. Preserve contract §2.1/§4 qualification, provenance review, and separate reviewed pin promotion. The current candidate therefore remains unapproved; exact updated-source review, under-cap tests, generated workflow/snapshot, and executable gates are pending. No release, publication, or Rust test ran. See the [immutable-release concept](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases), [immutable-release endpoint](https://docs.github.com/en/rest/repos/repos#check-if-immutable-releases-are-enabled-for-a-repository), [GITHUB_TOKEN permission model](https://docs.github.com/en/actions/tutorials/authenticate-with-github_token), and [workflow permissions](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#defining-access-for-the-github_token-scopes).

## MBX provenance

The activation worker reports immutable MBX `1.21.0` release commit `201b9df3d18e8e96831bee631035f6b7c7ae20e0` and GNU checksum `1ed3fd18da0decc106a6242d1b724e6a4b8d0f6173b8abe4d4d68c929ed47120`.

No local release-provenance transcript was saved. Selected registration and acquisition phases passed by owner report. A Sol-reviewed isolated launcher completed one source build. This does not complete final security review, cache provenance, or activation for repeated measurements. See [build results](build-results.md#reviewed-linker-v2-source-proof).

### Launcher preflight

Disposition: FAIL at launcher SHA `3bb10a21339c0aab3e7fc10f11ee57f97fc6d98a37e4972aa9fcac10ad6ef8c6`.

The `register-rust` phase used unsupported `/usr/bin/mount --remount,bind,ro` syntax. Installed `mount` help requires `mount -o remount,bind,ro <target>`.

The guarded namespace exited. Read-only checks found no task mount and no UID 65534 process. The script hash remained unchanged. No artifact download or build occurred.

The owner corrected this syntax in launcher SHA `1388fb22db4b61de44f3fa18fec9159ea7da57f6ab7f5c577d34068fde21d112`. Sol approved that script. The separate offline runtime failure is recorded below. MBX activation remains NOT RUN.

### Offline launcher attempt

Disposition: FAIL at Sol-reviewed script SHA `1388fb22db4b61de44f3fa18fec9159ea7da57f6ab7f5c577d34068fde21d112`.

The `register-rust` phase failed when Mise tried to resolve its version list online inside the offline namespace. No compilation or toolchain acquisition occurred.

The `unprivileged_exec_design` owner is checking supported offline `mise link` behavior. The exact command and output remain pending. Do not mark MBX activation PASS.

### Main-source compiler image attempt

At source `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, the Sol-reviewed launcher SHA `d31315193da05f29746509b3e395aca1394c6b7160c39b8bb075782c9998c748` reached `mbx build --locked --offline -p jackin`. It exited 101 because `/usr/bin/cc` points to absent `/etc/alternatives` in the private chroot. MBX attempted four crates, recorded zero hits, zero misses, and six bypasses. It wrote 88.9 KiB of local metadata and uploaded no compiler outputs. No successful source compilation occurred. See [build evidence](build-results.md#first-main-source-attempt).

Direct-linker candidate `9fe5f1143587373d0ab5df821ff68e6abcc72280f642bdea986df02d1d0e85d4` failed review before execution. Its environment checker rejects exported `CC`, `AR`, and Cargo linker variables before stage creation updates that checker. No phase executed.

Sol reviewed bounded build-only launcher hash `9d94884ac525c91f5a2e7c5abb6f068cdb814ef0780097fa840d35b57c212851` PASS. One proof succeeded. It does not authorize cache or performance claims.

The bypass investigation attributes 109 `unportable-native-link` bypasses to MBX rejecting an explicit non-Clang native linker. The task environment exports a Cargo GCC linker. The 495 `unknown-codegen-option` attribution remains plausible but is not traced to packages. The owner is preparing a rootfs-only `/etc/alternatives/cc` link to verified GCC, removing linker environment exports, and recording bypass logs and statistics. Host `/etc` remains unchanged. Require exact Sol review before another build.

Diagnostic candidate `a56a1822bbec56025fc2cd499de3a09be57c888a87ce40ba56eef70e99a3f671` is HOLD and withdrawn. `mbx explain --last` diagnoses MISS events, not the BYPASS events from this run. No phase ran. Metadata snapshots alone do not prove file contents stayed unchanged or exclude atime and private-home writes. The security reviewer requires stronger filesystem evidence before further execution. A new immutable artifact and exact Sol review are pending.

## Review owners

| Owner | Work | State |
|---|---|---|
| `preflight_security_review` | Initial security gate | Initial review complete; follow-up pending |
| `unprivileged_exec_design` | Execution boundary and MBX launcher | One build-only proof passed; cache investigation and full security validation remain IN PROGRESS |
| `codex_schema_runtime` | Agent settings confirmation | IN PROGRESS |
| `branches` | Source disposition matrix | Exact source paths and dependency gates remain IN PROGRESS |
| `baseline_method_review` | PR #1108 exact-head source review | Complete; final build-performance review NOT STARTED |
| `execution_crosscheck` | Consolidation source review and final gates | OMP and route source reviews complete; final correctness review NOT STARTED |
| `consolidation_review` | Migration fixtures and redaction correction | IN PROGRESS; redaction tests NOT RUN pending MBX |
| `jackin_ci_consumer` | Collector and Mise/MBX integration | IN PROGRESS |
| `omp` | Account database and WAL root fix | Source review PASS at `1638522184ef45f0cd51fa5601a5e80c7fd89762`; stale-comment follow-up; Cargo tests NOT RUN |
| `debian_codex_route` | Codex account discovery route | Source review PASS across `688057f40173d32dda04a55bff1e3868c219710d` and `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a`; runtime route NOT RUN |
| `jackin_cli` | Launch prompt cleanup | Source review PASS at `640b33f9598307a360484526a46c9c20bd068f4e`; Cargo tests NOT RUN pending MBX |
| `architect_schema_review` | Independent runtime-performance review | NOT STARTED; exact runtime evidence pending |

The `architect_manifest_fix` owner supplied the exact-head review. The `velnor_recon` owners supplied generator history and workflow evidence.

## CLI prompt cleanup

Source review PASS at commit `640b33f9598307a360484526a46c9c20bd068f4e` (`refactor(load): remove unsupported initial prompt option`). The change removes `LoadOptions.prompt` and its current-role reuse and explicit-restore checks in `crates/jackin-runtime/src/runtime/launch.rs` and `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`.

Cargo tests remain NOT RUN pending reviewed MBX activation. This record reports source review only.

## Account route and restore source reviews

The `CODEX_HOME` source change in `0556ce39b1abb9cd6b387583d932e1556ca9dfd4` and follow-up `688057f40173d32dda04a55bff1e3868c219710d` passed exact-source review with lint follow-up `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a`. The follow-up adds only `#[cfg(test)]` to a test-only scan seam. Production discovery remains unchanged by that follow-up. Both commits are source-review evidence only.

Restore commit `234fc0ea3813d8cabe579a8a8e0a0b1162eb5230` and prompt-removal commit `640b33f9598307a360484526a46c9c20bd068f4e` passed source review. Cargo tests remain NOT RUN. Host discovery, account refresh and selection, workspace launch, and live Codex requests remain NOT RUN.

## Final review ownership

All final reviews are NOT STARTED. Each review requires final commits and complete evidence. Implementation workers provide evidence. They do not approve their own work.

| Review | Independent reviewer | Evidence producers | Status and start condition |
|---|---|---|---|
| Correctness | `execution_crosscheck` (Sol/medium) | Implementation and test owners | NOT STARTED; wait for exact final commits and test evidence. |
| Security | `preflight_security_review` (Sol/medium) | `unprivileged_exec_design` and `mbx_activation` | NOT STARTED; wait for final execution design and artifact provenance. |
| Build performance | `baseline_method_review` (Sol/medium) | `build_baseline` | NOT STARTED; wait for repeated baseline and split measurements for every scenario. |
| Runtime performance | `architect_schema_review` (Sol/medium) | `architect_contract` and `debian_codex_route` | NOT STARTED; wait for role restart and host account recheck at final heads. |

See [checklist](checklist.md), [build results](build-results.md), and [Debian results](debian-results.md).

## DCO-signed history replacement

The original task branch `refactor/build-split` remains frozen at `73ea2117b8e584c64dec92242271a86149a53125`; its DCO check reported ACTION REQUIRED. A replacement branch `refactor/build-split-dco` was created from the same main base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. It replays the original 60 commits in order with `-x`, adding `-s` only to the 39 Codex-authored commits that lacked a matching sign-off. The 21 commits with valid existing sign-offs retain them. No author identity or original content was rewritten.

Independent Sol review by `model_policy_audit` verified each old/new commit pair's tree, author, date, linear parent, and source-attribution trailer. Every commit tree matches its source, and the replacement final tree `9eebc07e80d91f2d29cc9d975b78a53ca4198b3b` exactly matches old head `73ea2117b8e584c64dec92242271a86149a53125`. The exact private mapping packet is `/root/.velnor-work/jackin-dco-replay-20261005/replay-evidence.md`, SHA-256 `4a027a6e1d354e91229e70959eab93827b809f875214767ad29d3da86585a923`, mode `0400`. The reviewer did not independently witness historical push receipts or pre-replay dirty-file hashes; those remain owner-recorded preservation evidence.

Replacement PR [#1113](https://github.com/jackin-project/jackin/pull/1113) was opened as a draft at head `7b6ea934490b6ef0d867a43def527e29caa3b63f`. Its DCO-2 check passed. Its first run `37289881948` failed Plan because pinned Velnor `0.1.0` rejects the configured `tasks` field; Required failed as a consequence and Rust jobs were skipped. This validates the sign-off remediation only; it does not establish source tests, generated-workflow correctness, or merge readiness. PR #1112 remains open until the replacement destination and its applicable gates are verified.

## Native account-selection fix

The selected native navigation/model change is source-reviewed at commit `e17a5dd5c0ef8a34ed20b1837534b851d38e78d1` (tree `5e07bdacd3cdbf4dc24071f9e5ef88c2be53f595`, parent `f8a1a802cafccbaf07eac1c91741174341fb52bf`). `execution_crosscheck` passed the four-path unit: stale explicit accounts route to Overview instead of silently selecting a sibling, unrelated provider rows do not clear a valid Codex route, and the cross-provider fixture now checks that the removed Claude key is absent while the other Claude account remains.

The reviewer found and the follow-up fixed a false-empty fixture assertion: `.catalogNormal` includes both Claude personal and work accounts. Commit `e17a5dd` now asserts the exact removed row is absent and the sibling remains in both store and provider-group results. This review covers native navigation/model source only; it does not accept the broader PR #1111 Rust `HostUsageRuntime` selection behavior. Swift tests and `swift-format` remain NOT RUN because the available host is Linux without the required Apple toolchain. Exact PR run `37300944505` also had no macOS job; Plan failed before the configured native lint/format tasks could be generated. See [exact-head CI](ci-coverage.md#dco-signed-replacement-pr-1113).


## Current migration source and MBX wrapper review

The current task source is `3986dfdd07dcfeb739f09f837d3a3ebdd944cbb9`, tree `50f5bbfc7e89fc66b71356ad12629028abfccea4`, parent `0cf350be6a66fd1316239452a718e890c051ad23`. The exact archive is `/root/.velnor-work/jackin-source-3986dfdd-20261005/source.tar.gz`, SHA-256 `5bb627279152a9e49cdfbbfd31ff6142ef4a30c18f2d8dd6da8a89648cca6563`; packet SHA-256 `e5afbb8c82ba4cc55e83badb48339248eab47215306cb8962d4328fa2c5ce21a`.

Independent source reviews passed the `toml_edit` lock-edge correction at `0cf350be6a66fd1316239452a718e890c051ad23` and the parser correction at `3986dfdd07dcfeb739f09f837d3a3ebdd944cbb9`. The locked versions and checksums are unchanged; `Cargo.lock` SHA-256 is `56ceaf85a8b0740b5f365f1163895c52dddb3bb3aa5a0849387c5c745b400092`. The parser fix accepts pinned Mise `2026.10.1` with its platform suffix and optional prefix, while rejecting wrong or extra-line output. The reviewer verified source fixtures, not the full harness execution.

At the earlier exact source `0cf350be6a66fd1316239452a718e890c051ad23`, locked offline Cargo metadata and fetch passed without compilation. PR run `37312549475` at `3986dfdd` confirms locked source fetching passes in CI. Plan then fails because pinned Velnor `0.1.0` rejects `.velnor/config.toml` field `tasks`; Required fails and Rust jobs are skipped. These results do not pass any test, compile, schema, or migration-golden gate.

The prior focused attempt on source `17b2b1be6a58a0e34af6d8308df915d110f4a785` stopped at compilation before tests. It reported unsupported `toml_edit::de::from_document` and an unused qualified `DocumentMut`; the source fix and lock edges are now reviewed. The new exact-source MBX launcher and wrapper harness remain under review. Do not repeat the failed attempt, run bare Cargo, or write fixture outputs before the new launcher and output path are reviewed.

## MBX registry-install routing finding

At Architect source `a5502dae6100856bb0d5095de3bee1f93805069c`, Dockerfile SHA-256 `880dfc54a3c7e82919f0e738d65bdb852e07ed8b9e04d18648c4c22910a9ef63`, a twelve-tool `mbx install` loop passed registry installs through to plain Cargo. Exact MBX 1.22.0 source `10474d43342ad65df3b02323dd8092d18ab38101` shows that `cargo install` without `--path` uses the passthrough route before scheduler/cache setup. This is a missing-cache-path source defect; no recursion failure was demonstrated.

Velnor/Architect source correction `a3e9f62841359192cd197189bca72c23606f3d3e`, tree `53490dc96068162b8c04afa39473071fdeb61151`, replaces registry installs with locked path installs from checksum-verified crates.io archives. The owner reports shell, static, and archive checks passed; exact Sol re-review and image execution remain pending. No tool install, image build, or Rust compilation is accepted.

## Native build-task capability

The conditional design review accepts a single `macos-26` task that builds the XCFramework and runs native validation on the same runner. The task should pin Xcode 26.6, use the required macOS SDK and Swift toolchain, and set bounded Cargo and Nextest workers. SwiftPM needs a narrow xtask `--jobs` integration.

The Velnor Git owner reserved these implementation files in `/root/.cache/velnor-macos-native-build-task-20261005`, branch `feat/macos-native-build-task-20261005`, based on main `4fffbc22ce159305c62ae039668da2a14e2e3366`: contract `config/verification.rs` and `config/workflow.rs`; orchestrator `config.rs`, `verification_tasks.rs`, optionally `workflow.rs`; renderer `verification_jobs.rs` and its tests; and focused `impl_schema2_verification_tasks.rs` tests. `velnor_freshness_research` owns this source unit. The implementation must validate identifiers and bounds, bind to the current same-repository checkout, render successful-required fan-in, use a supported pinned MBX bootstrap with no raw Cargo fallback, and test task parsing, runner mapping, bounds, fan-in, and rendered output. No implementation or generated output is accepted yet; require exact-source Sol review and current CI.
