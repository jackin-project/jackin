# Testing

Use [cargo-nextest](https://nexte.st) as test runner.

Install:

```sh
mise install
```

This installs the pinned Rust toolchain and dev tools (`cargo-nextest`,
`cargo-deny`, `cargo-audit`, and the rest of [mise.toml](mise.toml)) at the
same versions CI uses. Do not install these tools with ad hoc `cargo install`
commands.

Run all tests:

```sh
cargo nextest run
```

Run specific test:

```sh
cargo nextest run -E 'test(test_name)'
```

Run tests for specific module:

```sh
cargo nextest run -E 'test(/module::tests/)'
```

Run all feature-gated Rust tests except profile-isolated environment-backed smoke tests:

```sh
cargo nextest run --all-features
```

Run Docker-backed smoke tests:

```sh
cargo nextest run -p jackin --features e2e --profile docker-e2e
```

In PR checkouts, run `jackin-dev pr sync <PR_NUMBER>` and source
`$(jackin-dev pr path <PR_NUMBER>)/env.sh` first. Outside the PR sync flow, use
`eval "$(cargo run --bin build-jackin-capsule -- --export)"` before the
Docker-backed smoke command.

Never `cargo test` for normal Rust tests — always `cargo nextest run`.
The one sanctioned `cargo test` invocation is doctests, which nextest does
not run:

```sh
cargo test --doc --workspace --locked
```

## Verification matrix

| Change surface | Command | When |
|---|---|---|
| One module | `cargo nextest run -E 'test(/module::tests/)'` | inner loop |
| One crate | `cargo nextest run -p <crate>` + `cargo clippy -p <crate> --all-targets -- -D warnings` | before commit |
| Cross-crate Rust | `cargo xtask ci --fast` | before PR |
| Full non-Docker gate | `cargo xtask ci` | merge readiness |
| One CI partition | `cargo xtask ci --only <lint\|policy\|tests\|snapshots\|docs\|msrv\|powerset>` | inner loop mirroring a CI lane |
| Scoped feature powerset | `cargo hack check -p jackin -p jackin-diagnostics -p jackin-capsule -p jackin-agent-status -p jackin-term -p jackin-runtime --feature-powerset --all-targets --locked` | optional-feature crates (PR gate) |
| Container/runtime behavior | `cargo xtask ci --e2e` (Docker running) | capsule/runtime PRs |
| Docs/roadmap | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | any docs edit |
| File-size gate | `cargo xtask lint files` (`--format json\|github`) | structure / split PRs |
| README freshness (advisory) | `cargo xtask lint readme-freshness --base origin/main` | structural `crates/*/src` A/D/R without README touch |
| Agents gate | `cargo xtask lint agents` (`--format json\|github`) | new crate / AGENTS files |
| TUI snapshots | `cargo nextest run -p jackin-capsule -p jackin-console` (insta snapshots live only in these two crates today) | TUI render changes |

### Snapshot review policy

Changed `.snap` files are enumerated in CI against the PR merge-base with `origin/main` (step summary + job log). Reviewers must acknowledge each listed snapshot; hand-edited snapshots that merely match buggy output are rejected in review. Pending files (`*.pending-snap`) still fail CI. Prefer `cargo insta review` / `cargo insta accept` over hand-editing `.snap` bodies.


Every crate is verified by `cargo nextest run -p <crate>`. Exceptions worth naming: `jackin` E2E tests need `--features e2e --profile docker-e2e`; doctests need `cargo test --doc --workspace --locked`. The machine-checkable per-member map is also emitted by `cargo xtask health --format json` under `verification_map`.

## Recording capsule render-conformance fixtures

Capsule echo-back harness ([crates/jackin-capsule/src/daemon/tests.rs](crates/jackin-capsule/src/daemon/tests.rs)) replays PTY byte streams through multiplexer, asserts emitted frames reproduce pane model on virtual client terminal. Synthetic streams live in harness; real-agent fixtures are recorded from a trace-level `--debug` run:

1. Run session with `JACKIN_TELEMETRY_LEVEL=trace` (e.g. `JACKIN_TELEMETRY_LEVEL=trace cargo run --bin jackin -- console --debug`), exercise agent. Note run id CLI prints.
2. Extract one session's PTY stream from run log into binary fixture:

   ```sh
   cargo xtask pty-fixture ~/.jackin/data/diagnostics/runs/<run-id>.jsonl <session-label> \
     crates/jackin-capsule/tests/fixtures/pty/<agent>-<scenario>.bin
   ```

   Session label = pane label in capsule tab (e.g. `Codex`). When the run JSONL contains only the `capsule_log` pointer, the extractor follows that path to the raw in-container `multiplexer.log`; passing `multiplexer.log` directly also works.
   The trace payload lines are written to local files only when OTLP export is inactive. If `OTEL_EXPORTER_OTLP_ENDPOINT` is set in your shell, the backend is the sink and raw payloads are not mirrored to `multiplexer.log`; unset it for local fixture extraction. `JACKIN_DIAGNOSTICS_FILE=1` can force the host JSONL file, but it does not mirror raw capsule payloads while capsule OTLP is active.
3. Reference fixture from harness scenario with `include_bytes!`.

## Walking the operator through local validation

Every `jackin <subcommand>` invocation in manual validation MUST include `--debug`. Includes `cargo run --bin jackin -- <subcommand> --debug` from checkout.

`--debug` captures every external command (`docker`, `git`, `id`, etc.) with output plus `[jackin debug ...]` instrumentation into `~/.jackin/data/diagnostics/runs/<run-id>.jsonl` only when OTLP export is inactive. If `OTEL_EXPORTER_OTLP_ENDPOINT` is set in the shell, the backend is the sink and no file is written; unset it for JSONL-based triage or set `JACKIN_DIAGNOSTICS_FILE=1` to write both. CLI prints the run id either way: in OTLP-only mode, ask for the run id and query the backend for `parallax.run.id=<run-id>` instead of looking for a file.

Smoke tests: suggest `jackin console` first, prefer `the-architect` role over `agent-smith`. Standard smoke command:

```bash
cargo run --bin jackin -- console --debug
```

Use `jackin load` only when PR specifically needs that CLI path:

```bash
cargo run --bin jackin -- load the-architect . --debug
```

No `--no-intro` on debug smoke — debug mode already suppresses intro; `--debug --no-intro` = redundant.

Unexpected behavior from clean (non-debug) run → first ask operator to rerun with `--debug`, share run id; agent reads JSONL before proposing fixes.

Does not apply to:

- Inspection commands operator runs (`pgrep`, `pmset`, `cat`, `ls`) — not `jackin` invocations.
- Production recommendations or scripted automation (debug output too noisy).

## Flakes and fuzz

### Flake policy

CI nextest uses `[profile.ci]` (`.config/nextest.toml`): fixed 2 retries with a 1s delay and `final-status-level = "flaky"`. A pass-on-retry is reported as flaky — never silently absorbed. The sharded workflow uploads `target/nextest/ci/junit.xml` per group and fails if any flaky test is not listed in the shrink-only quarantine ledger `flaky-tests.toml` (repo root; each `[[test]]` needs `name`, `owner`, `reason`, `since`). Prefer fixing the flake over quarantining.

Junit artifacts are named `nextest-junit-<group>-<lane>` and seed the Phase 0 suite-wall-time baseline once measured.

### Fuzz targets

| Target | Crate path | Smoke (PR / ci.yml) | Long (hygiene) |
|---|---|---|---|
| `damage_grid_process` | `crates/jackin-term/fuzz` | 60s `--sanitizer none` | 300s; ASan 300s |
| `config_migrate` | `crates/jackin-config/fuzz` | 30s | 120s |
| `workspace_migrate` | `crates/jackin-config/fuzz` | 30s | 120s |
| `manifest_migrate` | `crates/jackin-manifest/fuzz` | 30s | 120s |
| `manifest_validate` | `crates/jackin-manifest/fuzz` | 30s | 120s |
| `env_resolve` | `crates/jackin-env/fuzz` | 30s | 120s |
| `decode_frames` | `crates/jackin-protocol/fuzz` | 45s | 120s |

Local smoke (nightly + cargo-fuzz via mise):

```sh
cd crates/jackin-term && cargo fuzz run --sanitizer none damage_grid_process -- -max_total_time=30
cd crates/jackin-config && cargo fuzz run --sanitizer none config_migrate -- -max_total_time=30
```

Committed seeds live under each fuzz crate's `corpus/<target>/` (fixture-derived TOML for migrate/validate targets; tag+payload frames for `decode_frames`). **Promotion rule:** when a fuzzer finds a crash or hang, (1) minimize with `cargo fuzz cmin <target>` / `tmin`, (2) commit the minimized input under `corpus/<target>/`, (3) add a deterministic regression test in the owning crate that feeds the same bytes (or the decoded fixture) so the finding never re-enters CI only via the fuzzer. Do not grow corpora with non-minimized corpus dirs from long runs without `cmin`.

Migration fixture harness ([`crates/jackin/tests/migration_fixtures.rs`](crates/jackin/tests/migration_fixtures.rs)) enforces golden equality against `after.toml` and second-pass idempotence for every config/workspace/manifest fixture.

### DinD chaos lane (scheduled)

Hygiene job `dind-chaos` runs three seeded fault scenarios against real Docker
(`chaos_kill_container_mid_session`, `chaos_sigkill_capsule`, `chaos_drop_control_socket`).
Replay: `JACKIN_CHAOS_SEED=<n> cargo nextest run -p jackin --features e2e --profile docker-e2e -E 'test(chaos_kill_container_mid_session)'`.
Default seed is fixed (`0xc4a0_55eed`); `workflow_dispatch` input `chaos_seed` overrides.

## Allocation lane (dhat) — static budget policy (plan 026)

The `dhat-heap` allocation suites in `jackin-term` and `jackin-capsule` run on
the scheduled Hygiene workflow (`dhat-allocation` job, advisory /
`continue-on-error`). **Ratchet decision:** keep `perf_dhat_budgets` fed from
the static ceilings in [`crates/jackin-capsule/src/perf_budgets.rs`](crates/jackin-capsule/src/perf_budgets.rs) (in-test
guardrails + textual ratchet). Measured dhat output is artifacted for trend
inspection but does **not** yet drive the ratchet — re-evaluate after ≥3
stable scheduled runs on the same runner class. Never budget from a single run.

## Advisory measurement lanes (hygiene schedule)

Trigger manually: `gh workflow run Hygiene` (or wait for the daily cron).

| Lane | Job | Artifact | Gate? |
|---|---|---|---|
| Beta clippy canary | `beta-clippy-canary` | `beta-clippy-log` | advisory — `continue-on-error` |
| Coverage (llvm-cov) | `coverage` | `coverage.lcov` | advisory — artifact only |
| Miri pure crates | `miri` | step summary | advisory |
| ASan fuzz (scheduled) | `scheduled-hygiene` step | step log | advisory (PR fuzz stays `--sanitizer none`) |
| cargo-mutants | `mutants` | `mutants-out` | advisory — never fails job |
| hakari timing | `hakari-timing` | `cargo-timings-hygiene-baseline` | advisory investigation only |
| Cold-start + PTY frames | `cold-start-bench` | `cold-start.json`, `frame-timing.json` (first frame + input repaint, 3 samples) | advisory measurement |
| rust-analyzer clean | `rust-analyzer-clean` | `ra-stats.txt` | advisory — `continue-on-error` on error grep |
| Per-crate build times | `build-time-measure` | `build-times.json` (5 crates × clean/incremental) | scheduled `build-time` ceiling ratchet |
| dylint render purity | `dylint-advisory` | `dylint-findings` | advisory — `continue-on-error`; nightly pin in `crates/jackin-lints` |



## First-frame / input-to-frame harness (plan 026)

`cargo xtask frame-timing` launches the built host console through a 120×36 PTY,
waits for alternate-screen entry plus the first substantial paint, injects a
Down-arrow event, and measures the next repaint. Three independent samples are
written to `frame-timing.json`; the scheduled lane keeps this advisory because
host scheduling noise is still material, but a missing/blank frame fails the
job instead of silently producing a number.

The same Hygiene workflow writes `build-times.json`, copies it to `target/`, and
runs the `build-time` artifact-ceiling ratchet. The family is skipped explicitly
when the scheduled artifact is absent (normal local/PR lint) and hard-fails the
scheduled job when any clean or incremental build exceeds its reviewed ceiling.
