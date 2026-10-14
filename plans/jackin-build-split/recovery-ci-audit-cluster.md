# CI audit recovery cluster: behavior and disposition

Read-only audit performed 2026-10-05 against the frozen inventory captured at 2026-10-05T12:27:50Z. The packet records base checkout `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. No source, workflow, or manifest files were changed; no Cargo command or Git operation was run.

## Current source and consumer boundary

The inspected task worktree is `/root/Projects/tailrocks/jackin-project/jackin-refactor-build-split`. Its `crates/jackin-xtask/src/ci_audit.rs` and `ci_audit/tests.rs` bytes produce Git blob IDs `3d78a7758290d2cb1bb551c9c0af363d748d438b` and `40ea879464121b3641509d6c347a3a331fa05d46`, respectively. I computed those IDs from the file bytes without invoking Git. They match the frozen packet's main/task/current-PR-1113 path states.

The xtask CLI registers `ci-audit` in `crates/jackin-xtask/src/main.rs` (`mod ci_audit`, `Command::CiAudit`, and its dispatch to `ci_audit::run`). The audit implementation fetches run/job metadata and logs, parses structured Velnor reports, aggregates cache/build/product markers, appends a summary, and optionally fails the clean gate. Its consumers in this checkout are the CLI and its module tests. Searches of `.velnor/config.toml`, `.github`, and `mise.toml` found no `ci-audit` invocation or `--expect-clean` caller. The generated CI workflow does build/test `jackin-xtask`, but this audit command is not wired into it. Therefore the evidence below establishes code behavior, not an active CI gate.

Current functions include `job_report_expected`, `scan_log`, `scan_velnor_report`, `record_velnor_report`, `parse_mbx_cache_outcome`, `clean_gate_total`, and `Markers::total` in `ci_audit.rs`. Tests include report-step scoping, structured report decoding, cache/Mr. Boxington accounting, clean-gate behavior, and product-step result classification in `ci_audit/tests.rs`.

## Unit dispositions

### `nonmerge:patch:02d9be608b2c3b26077b6aac1080906f59f304b6` — ALREADY PRESENT semantically

All four source commits carry the same two-file patch group:

- `4424e8409cd5e874c1e2c59dc55a05cdc0932d92`
- `9e0fae993320b94d896146b77f43f182ab25a218`
- `a8d137ddc04cc948884e984fc1c6421313ae6825`
- PR #1089 candidate `f8c9170888b7cf4028d69f3fe055144191c2e78f`

The patch scopes missing-report accounting to jobs that contain the named report step, clears false missing-report markers for control jobs, classifies only failure-like product outcomes as failures (failure/cancelled/timed_out/action_required/stale), and labels the summary count as failures. Its frozen raw patch is `sources/matched-content-prior/patches/a4f7e2b438a4a172bc143956e111fa8546cb66c19af3bafbb5fa77d90fcf6a48.patch` (6,430 bytes). The candidate result blobs (`7bd2cbbc4c426b5c9533a0187be3014a8e1c34ea` and `cb44added188068db2868862c1c04c6eb3fbeeca`) differ from the current path blobs above; this is not exact patch/result identity and does not establish ancestry.

The current source already has `job_report_expected`, uses it when handling absent logs/reports, and limits its effect to completed non-skipped jobs with a report step. `ProductMarkers::observe` uses the same failure-like status set. Existing tests `control_jobs_do_not_require_velnor_reports` and `product_steps_are_counted_and_non_success_is_visible` cover the contract. Do not replay the PR #1089 candidate. The source-commit ledger keeps the four source OIDs mapped to this group; none is recorded as an exact main ancestor.

### `nonmerge:patch:072cbfb97308ecfd6f4957273f805b8ed52379a0` — ALREADY PRESENT semantically

The source set is:

- `0669358b967e91e3fb8eebccde9259728db7406a`
- `2b925b960661ea6a0dcdba23085683d8ca847988`
- PR #1089 candidate `4f70d2c5c714fa58cee9faa09157f63f53abdd19`
- `504b3ce60c28bb509c4f7925cc3dc6a0ceaef47c`

The unit adds structured cache/compiler/product reporting, exact versus partial cache-log accounting, Velnor report aggregation, a richer summary, and UTC timestamp normalization that accounts for macOS `date` flags. Its raw patch is `sources/matched-content-prior/patches/29e7f0c5818436aa1d6861e8a099111c65af9a76a65a774841b55fd1205cc252.patch` (17,764 bytes).

Current code already parses schema-3 `VELNOR_CI_REPORT` payloads, nullable cache layers, origin-download counts and MBX outcomes; reports cache hits/restores; identifies product transport steps; and aggregates these metrics into the summary and clean gate. Existing tests `scanner_distinguishes_cache_reuse_and_reads_velnor_report`, `real_velnor_report_records_nullable_layer_as_unknown`, `exact_mbx_hits_allow_cargo_compiling_lines_but_misses_do_not`, `structured_origin_download_counts_feed_clean_gate`, `report_schema_and_cache_states_fail_closed`, and `product_steps_are_counted_and_non_success_is_visible` cover the principal behaviors. Current `epoch` also has the platform-specific UTC date parsing, but there is no focused timestamp-normalization test. Add coverage for fractional-second `Z` timestamps and sentinel/invalid input if that path is changed; preferably isolate normalization/argument construction so the assertions do not rely on machine-specific `date` behavior.

The candidate result blobs (`3878624e0d7ad01fdbc73552e87859ace8f81bbe` for code and `b97cf60bdc385508d963dacf5798d2421378c2f7` for tests) differ from current. The behavior is present in the current owner implementation, so do not replay the PR #1089 candidate. Preserve that distinction from source ancestry and exact patch matching.

### `nonmerge:patch:874b67609bf4408e45d9dd2eef109b4baf80f957` — REPLACE with the current refined gate; do not replay

The source set is:

- `25897d427b172f8620b71a54010a4a33fdc596b7`
- PR #1089 candidate `35b3338ec7774967a20e3f4226ae56728060a3d3`
- `a4be4ce21af8cd8d0507d4b8437a789997597025`
- `abda8f48bf3a01e258ed364ee1f0afcace994a6a`

The patch extends the clean gate to report-missing/fallback/parse failures, partial cache restores, compiler activity, and product failures; recognizes nullable cache fields; and guards report markers against text embedded in scripts. Its raw patch is `sources/matched-content-prior/patches/b3f1ae2b54a6cd10abd5ea244d6f6014e2d253faef7dc442048e41a968d53a9d.patch` (13,079 bytes). Exact parent edges show the layer order: `0669358b967e91e3fb8eebccde9259728db7406a → a4be4ce21af8cd8d0507d4b8437a789997597025`, `2b925b960661ea6a0dcdba23085683d8ca847988 → 25897d427b172f8620b71a54010a4a33fdc596b7`, `4f70d2c5c714fa58cee9faa09157f63f53abdd19 → 35b3338ec7774967a20e3f4226ae56728060a3d3`, and `504b3ce60c28bb509c4f7925cc3dc6a0ceaef47c → abda8f48bf3a01e258ed364ee1f0afcace994a6a`; then `a4be4ce21af8cd8d0507d4b8437a789997597025 → 4424e8409cd5e874c1e2c59dc55a05cdc0932d92`, `abda8f48bf3a01e258ed364ee1f0afcace994a6a → 9e0fae993320b94d896146b77f43f182ab25a218`, `25897d427b172f8620b71a54010a4a33fdc596b7 → a8d137ddc04cc948884e984fc1c6421313ae6825`, and `35b3338ec7774967a20e3f4226ae56728060a3d3 → f8c9170888b7cf4028d69f3fe055144191c2e78f`. Keep these as separate source-attributed units if documenting history.

The current gate implements the purpose with a refined policy: report absence is charged only when the job has the named report step; report schema and required cache layers fail closed; false marker substrings are ignored; and build lines may be discounted only when exact MBX cache evidence supports a warm result. This differs from the older patch's unconditional compiler-line penalty. Tests `control_jobs_do_not_require_velnor_reports`, `report_failures_are_visible_to_clean_gate`, `report_schema_and_cache_states_fail_closed`, and `exact_mbx_hits_allow_cargo_compiling_lines_but_misses_do_not` exercise the current policy. Keep that refined policy; do not select the old gate hunk or overwrite it with its unconditional rule. Candidate blobs (`955d600b84b1cf1cfb37119f2091d14e818736b0` and `357d8a7bb9050ed88983ee494800464e1cd3149f`) differ from the current file results.

### `nonmerge:patch:4073f84bfeef83f472a5aa2b51eafeeeaf3ad05b` — ALREADY PRESENT by exact path content

The source/candidate is `93468de9ce19294b9dbeff312c2d50188dfeb603`, subject `feat(xtask): audit structured CI cache outcomes`, mapped to PR #1106. Its parent is `7a1bc71b2d1fd8dbd0cdce2fac0fd5ff24f349ee`. The raw patch is `sources/matched-content-prior/patches/7340f9c4f99fa4841df7b86540ee301636396897cb3101aeb30ab11ea83e1704.patch` (30,932 bytes).

The frozen path-state/equality records say its final `ci_audit.rs` and `ci_audit/tests.rs` results exactly equal current main/task/PR #1113 blobs: `3d78a775…` and `40ea8794…`. The current source has the structured report parser, cache/compiler/product metrics, report expectation helper, and associated tests. Record this as ALREADY PRESENT; do not replay. This is exact touched-path content evidence, not a claim that the candidate commit is an exact main ancestor.

## Dependencies, acceptance, and remaining gap

Dependency order if evaluating the historical source patches is `072cbfb… → 874b676… → 02d9be…`; do not cherry-pick an upper layer without its predecessor. The current checkout already contains the resulting functionality and the later refined gate, so the three PR #1089 candidate units should not be newly selected. The independent `4073f84…` group is already present by exact path content.

The existing focused unit tests listed above are the relevant acceptance set. If the command is later wired into CI, add a current-owner workflow/configuration test proving the intended jobs invoke `cargo xtask ci-audit --expect-clean`, that control jobs are excluded from report expectations, and that a report-bearing job fails on missing/fallback/malformed reports and non-exact cache evidence while accepting a truly exact warm-MBX case. Run the focused `jackin-xtask` CI-audit tests and generated-workflow validation only after that wiring decision. No tests were run for this read-only audit.

Evidence read: frozen `README.md`, `records/nonmerge-unique-content-dispositions.csv`, `records/source-commit-dispositions.csv`, `records/nonmerge-content-target-equality.csv`, `records/nonmerge-content-target-path-state.csv`, and `sources/matched-content-prior/{matched-oid-content.csv,matched-oid-content.jsonl,patches/*.patch}`; current `ci_audit.rs`, `ci_audit/tests.rs`, `main.rs`, and searches of `.velnor/config.toml`, `.github`, and `mise.toml`.
