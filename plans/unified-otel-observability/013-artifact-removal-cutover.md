# Plan 013: The cutover — remove every jackin❯-owned telemetry file, reader, log command, and legacy key

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-diagnostics/src/run.rs crates/jackin-diagnostics/src/summary.rs crates/jackin-runtime/src/runtime/logs.rs crates/jackin/src/cli/logs.rs crates/jackin/src/cli/diagnostics.rs crates/jackin-usage/src/logging.rs crates/jackin-usage/src/telemetry_store.rs crates/jackin-runtime/src/host_daemon.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH (removals; operator-visible CLI surface changes; PTY-fixture tooling must be re-anchored)
- **Depends on**: plans/unified-otel-observability/007-identity-lifecycle-roots.md, 011-legacy-callsite-migration.md, 012-diagnostics-validate-health.md (validate must exist before summary/compare vanish)
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the removal rows of "Required jackin❯ end state" (run JSONL, sidecars, build logs, `multiplexer.log`, host-daemon log, log commands/readers, file-specific configuration, `telemetry_store` rename, legacy key removal) and the acceptance criterion "Workspace searches find no production telemetry/log file writer or reader"; the roadmap item is the binding contract and overrides this plan on any conflict. Pre-release policy applies: breaking removals, no migration shims (`PRERELEASE.md`); `DEPRECATED.md` stays empty.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The shipped state must not retain parallel old/new telemetry contracts. After plans 002–012, every signal flows through governed OTLP; the local files, their readers, the log CLI surface, and the legacy correlation keys are now dead weight that contradicts the model ("bounded product memory and no dependency on a local telemetry file when Parallax is unavailable"). This plan deletes them and renames the mislabeled usage store. It is the point of no return — everything before it is additive; this is subtractive.

## Current state — the removal inventory

(verified at planning commit; every item below is production code unless marked)

**A. Run JSONL + sidecars + summary/compare (host):**
- Writer: `crates/jackin-diagnostics/src/run.rs` — `RUN_DIR = "diagnostics/runs"` (`:47`), file creation/persist (`:253-270`), `JsonEvent` v2 (`:133-164`), sidecars `command_output_path`/`write_command_output` (`:342-390`), pruning (`:976-1457` incl. `prune_all_runs` stdout rows), `JACKIN_DIAGNOSTICS_FILE` gate (`:1115-1117`), external run-id adoption (`:1126-1161`), `mint_run_id` (`:1119`).
- Readers: `crates/jackin-diagnostics/src/summary.rs` (whole module), `run/jsonl_adapter.rs` (whole module), `crates/jackin/src/cli/diagnostics.rs` Summary/Compare (`:15-21` and handlers `:77,:84`, `resolve_run_path` `:140-154`), bench `crates/jackin-diagnostics/benches/summarize_jsonl.rs`, fixtures `tests/fixtures/summary/`.
- Sidecar consumers: `crates/jackin-image/src/image_build.rs:224`, `crates/jackin-runtime/src/runtime/launch/failure.rs:51`, readers `crates/jackin-launch-tui/src/progress.rs:159-162`, `tui/components/failure_dialog.rs:66`, `jackin-core/src/launch_progress.rs:153` (`command_output_path` field on `LaunchFailure`), `diagnostics_path` field beside it.
- `JackinDiagnosticsLayer` (JSONL sink): `crates/jackin-diagnostics/src/observability.rs:128-155` + emit ladders `:1400-2051` + `JSONL_TARGET` (`:18`).
- KEEP (in-memory current-invocation/progress state): `RunDiagnostics`'s in-memory metrics/progress accumulators feeding the loading cockpit and `build_log.rs` ring buffer (`crates/jackin-diagnostics/src/build_log.rs` — memory-only, cap 5000; consumers in jackin-launch-tui) — the roadmap keeps "in-memory current-invocation/progress state only". The refactor extracts what the launch TUI needs (stage/timing state) into a plain state type without file writers, or reduces `RunDiagnostics` to that role under a truthful name (e.g. `InvocationProgress`).

**B. Capsule `multiplexer.log` + trace-payload fallback:**
- Path const `jackin-core/src/container_paths.rs:63` (`MULTIPLEXER_LOG`); writer `crates/jackin-usage/src/logging.rs` (`init` `:87-183`, `write_line` `:191-207`, rotation `:64-82`, `JACKIN_CAPSULE_LOG_PATH` `:59-62`) — after plan 011, only banner/panic writers remain; delete the module's file machinery (the panic hook keeps its `app.crash` facade emission — plan 010 step 6 — plus stderr).
- Host readers: `crates/jackin-runtime/src/runtime/logs.rs` (whole module — `jackin logs` impl), `crates/jackin/src/cli/logs.rs` (whole module), dispatch `crates/jackin/src/app.rs:162`, `Command::Logs` (`cli.rs:144`); premature-exit tail `crates/jackin-runtime/src/runtime/launch/exit_diagnosis.rs:122-126` + `restore.rs:404-413` (`capsule_multiplexer_log_path`), `launch_runtime.rs:33,1045`; capsule self-references `daemon.rs:979,1245`, `pr_context.rs:70,286`, `runtime_setup.rs:1443`, `tui/effect.rs:20`, `alloc_telemetry.rs:9` (DHAT log lines — test-only feature, KEEP but repoint its output to stderr), `telemetry.rs:29`.
- PTY fixture extractor (TEST tooling, KEEP but re-anchor): `crates/jackin-xtask/src/pty_fixture.rs:8-23` reads raw `multiplexer.log`; `TESTING.md:88-102` describes the flow. Re-anchor: fixture recording writes to an explicit fixture-capture file enabled by a dedicated env (`JACKIN_PTY_FIXTURE_CAPTURE=<path>`) in the capsule's PTY read path — a test-tooling gate, not production telemetry (mirrors how `alloc_telemetry` is double-gated).

**C. Host-daemon log:**
- `crates/jackin-runtime/src/host_daemon.rs` — `LOG_FILE_NAME` (`:23`), `write_log` (`:617-621`), `read_log` (`:442-446`), `DaemonStatus.log_path` (`:81`), unit files' Std{Out,Error} redirection (`:385-414`); `crates/jackin/src/app/daemon_cmd.rs` — `start()` log-file creation/piping (`:77-87`), `logs` subcommand (`:169-173`), `DaemonCommand::Logs` (`crates/jackin/src/cli/daemon.rs:4` enum). Child stdout/stderr piping: daemonized processes need SOME sink — redirect to `/dev/null` (launchd/systemd units drop the log paths; `daemon serve` runs with governed telemetry only). The `jackin-daemon` roadmap item (`docs/content/docs/roadmap/(reactive-daemon-program)/jackin-daemon.mdx:5`) already records this removal.

**D. Env/config surface:**
- Delete: `JACKIN_DIAGNOSTICS_FILE` (`run.rs:15,86,231,1112,1116`; `jackin/src/app.rs:303`), `JACKIN_CAPSULE_LOG_PATH`, `JACKIN_RUN_DIAGNOSTICS_PATH` (`container_context.rs:14,53-54`, `launch_runtime.rs:1224`), `JACKIN_RUN_DIR` (`daemon/file_export.rs:21,245-257` — check actual use: if it serves file EXPORT (downloads), it is NOT telemetry, keep), `JACKIN_TELEMETRY_FILE_LEVEL` + `TelemetrySink::DiagnosticsFile` (`jackin-diagnostics/src/logging.rs:29,70`), `PARALLAX_RUN_ID` + `OTEL_RESOURCE_ATTRIBUTES` run-id adoption (`run.rs:1126-1161`).
- Keep: `JACKIN_TELEMETRY_LEVEL`/`_CATEGORIES` + config `[telemetry]` keys (they gate governed DEBUG/TRACE detail — still contract-valid: "--debug … may also enable governed DEBUG telemetry"), `--debug`/`JACKIN_DEBUG`, OTLP standard env.
- Host affordance "Copy diagnostics path" (`docs/.../host-affordances.mdx:39` + its implementation — locate via `grep -rn "diagnostics" crates/jackin/src/console --include='*.rs' -il` and the Debug-info dialog surfaces in `docs/.../tui/{chrome,dialogs}.mdx`): the copyable diagnostics-log row disappears with the artifact; replace with the invocation id (copyable `cli.invocation.id`) — TUI dialog rules apply (read `docs/content/docs/reference/tui/index.mdx`; update matching TUI docs pages same PR — coordinate with plan 015).

**E. Legacy keys + old instruments (the namespace purge):**
- `parallax.run.id`: const `observability.rs:38`; JSONL ladders (deleted with A); `screen.rs` (deleted plan 009); `jackin-usage/src/telemetry.rs:168-212` (deleted plan 011); `jackin/src/app.rs:247` backend banner (repoint to `cli.invocation.id`); test files. `JACKIN_RUN_ID` env injection (`launch_runtime.rs:1250-1256`, capsule reads) — delete; `JACKIN_INVOCATION_ID` (plan 007) is the survivor.
- `jackin.*` attribute keys (`otel_keys`, `observability.rs:26-69`) and `jackin.*`/legacy metric instruments (`otel_metrics` `:73-116`, `metrics.rs` HotPathMetrics — superseded by plan 004/009/010 instruments; migrate remaining recorder call sites `record_frame`/`record_render`/`incr_*` to the new instruments and delete the old module), `registry.rs` EventDef string table + `DiagnosticStage` (superseded by the schema registry; delete after confirming zero references), `observability_events.rs` legacy kinds.
- The plan 001 namespace-ban allowlist shrinks to EMPTY; flip the xtask check to allowlist-free.

**F. `telemetry_store` rename (usage state is not telemetry):**
- `crates/jackin-usage/src/telemetry_store.rs` → `usage_snapshot_store.rs` (module + file rename; SQLite schema/content untouched — it stores provider quota snapshots). Update: `lib.rs:13` (`pub mod`), `usage.rs:547` call, `usage.rs:190,326-331,548,592` field/method `telemetry_store_path` → `usage_snapshot_store_path`, tests (`usage/tests.rs:234-296`, `telemetry_store/tests.rs` → `usage_snapshot_store/tests.rs` incl. test name `:479`), bench `benches/snapshot_upsert.rs:13`, `Cargo.toml` bench entry if named, crate README, docs crate page (`docs/.../reference/crates/jackin-usage.mdx:7` — plan 015 but the rename lands here; update the page in the same PR to keep `cargo xtask docs repo-links` green).

**G. Tests/benches/gates tied to files:**
- Delete: JSONL adapter tests, summary tests/fixtures/bench, run-file tests in `jackin-diagnostics/src/tests.rs` (46 tests — the file-writing subset), `logs.rs` tests, daemon-log tests. The export-volume ratchet provider reads `target/telemetry-volume.json` from a conformance test — KEEP (it measures OTLP export volume, not a product file).
- `TESTING.md:104-129` local-validation flow references the JSONL fallback — plan 015 rewrites; keep the `--debug` mandate.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Full fast gate | `cargo xtask ci --fast` | exit 0 |
| Acceptance grep (writers/readers) | `grep -rn "multiplexer.log\|diagnostics/runs\|jackin-daemon.log\|JACKIN_DIAGNOSTICS_FILE\|PARALLAX_RUN_ID" crates/ --include='*.rs' \| grep -v "tests.rs\|pty_fixture\|fixture"` | empty |
| Acceptance grep (keys) | `grep -rn "parallax\.run\.id\|\"jackin\." crates/ --include='*.rs' \| grep -v tests` | empty (attribute-key matches) |
| Namespace gate | `cargo xtask telemetry-registry` | exit 0 with empty allowlist |
| Docker e2e (operator-run if Docker present) | `cargo xtask ci --e2e` | exit 0 |

## Scope

**In scope:** every file named in the inventory above, plus `crates/jackin/src/cli.rs` (drop `Logs` variant), `cli/dispatch.rs`, `app.rs`, launch failure-dialog code that displayed sidecar paths (`jackin-launch-tui` failure dialog shows the invocation id + "see Parallax" instead), protocol/daemon status field drops, and the PTY-fixture re-anchor (`crates/jackin-capsule` capture gate + `crates/jackin-xtask/src/pty_fixture.rs` reading the new capture file).

**Out of scope:** docs pages except those whose repo-links break in the same PR (full docs sweep = plan 015); the wire receiver/soak tests (plan 014); any Parallax behavior.

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. This plan contributes the breaking commits of that PR — Conventional Commits with `!` where operator-visible surface is removed: `feat(cli)!: remove jackin logs, daemon logs, diagnostics summary/compare` with `BREAKING CHANGE:` footers. Sign `-s`, push after every commit. Recommend one commit per inventory letter (A–G) in the order below.

## Steps

Ordered so the tree compiles between commits:

### Step 1 (F): `telemetry_store` rename
Mechanical rename per inventory F. **Verify**: `cargo nextest run -p jackin-usage --locked` → pass; `grep -rn "telemetry_store" crates/ --include='*.rs'` → empty.

### Step 2 (B-fixture): Re-anchor PTY fixture capture
Add the `JACKIN_PTY_FIXTURE_CAPTURE` gate writing raw PTY bytes to the given path in the capsule PTY read path (`session.rs` reader pump); repoint `pty_fixture.rs` extractor at the capture-file format; update its tests. **Verify**: `cargo nextest run -p jackin-xtask -p jackin-capsule --locked` → pass.

### Step 3 (C): Host-daemon log removal
Per inventory C: delete write_log/read_log/log_path plumbing, `daemon logs` subcommand, `/dev/null` the child pipes, drop unit-file log paths. **Verify**: `cargo nextest run -p jackin-runtime -p jackin --locked` → pass; `jackin daemon --help` no longer lists `logs` (manual: `cargo run --bin jackin -- daemon --help`).

### Step 4 (B): `jackin logs` + multiplexer.log removal
Delete reader modules + CLI command + exit-diagnosis tail (premature-exit diagnosis loses the log tail — it keeps `docker logs`-based stderr capture if present, else reports exit status only; check `exit_diagnosis.rs` for what remains without the file) + capsule file writer machinery + path const. **Verify**: `cargo nextest run --workspace --all-features --locked` → pass; acceptance grep for `multiplexer.log` clean (modulo fixture capture naming — the capture file must NOT be named multiplexer.log).

### Step 5 (A): Run JSONL + sidecars + summary/compare removal
Per inventory A: delete writers/readers/CLI/bench/fixtures; extract the in-memory progress state the launch TUI consumes; `LaunchFailure` loses `diagnostics_path`/`command_output_path` (failure dialog shows invocation id); docker-build output stays visible via the in-memory `build_log` ring buffer. Delete `JackinDiagnosticsLayer` + emit ladders from `observability.rs` (the OTel pipeline from plans 002–004 is now the only sink). **Verify**: workspace tests pass; `jackin diagnostics --help` lists only `validate`.

### Step 6 (D): Env/config surface
Delete the env vars/gates per inventory D (keeping the KEEP list); replace the "Copy diagnostics path" affordance with copyable invocation id. **Verify**: `grep -rn "JACKIN_DIAGNOSTICS_FILE\|JACKIN_CAPSULE_LOG_PATH\|JACKIN_RUN_DIAGNOSTICS_PATH\|TelemetrySink::DiagnosticsFile" crates/ --include='*.rs'` → empty; console snapshot tests updated + passing.

### Step 7 (E): Namespace purge
Delete `parallax.run.id`/`JACKIN_RUN_ID`/`PARALLAX_RUN_ID` adoption, `otel_keys`/`otel_metrics`/old `metrics.rs`/`registry.rs` string table/`observability_events.rs`; migrate surviving recorder call sites to plan 004 instruments; repoint `app.rs:247` banner to invocation id; empty the namespace-ban allowlist and make `cargo xtask telemetry-registry` enforce allowlist-free. **Verify**: both acceptance greps clean; `cargo xtask telemetry-registry` → exit 0.

### Step 8 (G): Test/gate reconciliation + full lanes
Delete dead tests/benches/fixtures; regenerate export-volume ratchet; run `cargo xtask ci --fast`; if Docker available run `cargo xtask ci --e2e`. **Verify**: both exit 0.

## Reopened audit additions (2026-07-16)

- Remove dead `R/O reveal diagnostics` hints from all Capsule, console, and launch surfaces and regression-test the rendered hint text.
- Rename `TELEMETRY_STORE`, `TELEMETRY_STORE_PATH`, and `/jackin/state/usage/telemetry.db` to usage-snapshot terminology/path everywhere, including tests, benches, README and research references; no compatibility shim is retained.
- Add `conformance_no_local_artifacts`: isolated init→emit→shutdown walks the state tree and rejects telemetry/log artifacts while explicitly allowing retained usage state and test-only ratchet output.
- Rewrite the PTY fixture README for the explicit capture-file flow and test the xtask parser; remove dead artifact-era `RunDiagnostics` capsule-log arguments/details/JSON allocations and correct JSONL/file-fallback comments.
- Remove broad namespace-prefix test exemptions so wire/testbed sources cannot hide reintroduced `jackin.*`/`parallax.*` literals; retain only exact proven non-telemetry product-name exemptions.
- Add negative help/parser assertions for `logs`, `daemon logs`, and removed diagnostics `summary`, `compare`, `follow`, `reveal`, and `bundle` surfaces.

## Test plan

- The acceptance criteria greps ARE the primary structural tests (wired as done criteria).
- Updated behavior tests: premature-exit diagnosis without log tail; failure dialog rendering; daemon status without log path; console Debug-info dialog with invocation id.
- Conformance suite must remain green throughout (`-E 'test(/conformance/)'`).
- Add `conformance_no_local_artifacts`: run a full in-process telemetry lifecycle (init → operations → shutdown) under a temp HOME/data dir and assert NO file is created under the data dir by telemetry (walk the temp dir; allow the usage snapshot DB and product state).

## Done criteria

- [ ] Acceptance greps (command table) return empty
- [ ] `cargo nextest run --workspace --all-features --locked` exits 0; `cargo xtask ci --fast` exits 0
- [ ] `conformance_no_local_artifacts` passes
- [ ] `jackin --help` shows no `logs`; `jackin daemon --help` no `logs`; `jackin diagnostics --help` only `validate`
- [ ] Namespace-ban gate allowlist-free; `cargo xtask telemetry-registry` exits 0
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- Any consumer of the JSONL/sidecar/log files exists beyond the inventory (fresh grep disagrees) — enumerate before deleting.
- The launch TUI cannot render failure/progress without the removed artifacts (missing in-memory state) — the extraction in step 5 must precede deletions; if the state shape is unclear, stop and report the concrete gap.
- The PTY fixture flow cannot reproduce existing fixtures' byte-exactness through the new capture gate (render-conformance suite would rot).
- `parallax.run.id` external-adoption removal breaks a documented Parallax launch flow the operator still relies on — this contradicts the roadmap text ("The legacy `parallax.run.id` … removed"), so surface it to the operator rather than deciding.
- Memory note `parallax-run-id-contract` (project memory) says the key is "never rename" ecosystem compat — the roadmap item explicitly supersedes it; if the operator has not confirmed that supersession in-session, ask before executing step 7.

## Maintenance notes

- This is the PR where reviewers check the acceptance criterion "no production telemetry/log file writer or reader" literally — keep the greps in the PR body.
- After landing: update the project memory (`parallax-run-id-contract`) — the compat key is gone by design.
- `DEPRECATED.md` stays empty (pre-release breaking change, not a deprecation).
- Plans 014/015 depend on this landing: the soak/receiver tests assert the absence of files; docs describe the post-cutover reality.
