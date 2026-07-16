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

## CI performance contract

For a dependency graph that has completed before, every required CI job and the
required pipeline target should finish within one minute. A warm job must not
update the crates.io index, download an upstream crate, or compile an unchanged
registry/git dependency. The lane-scoped registry warmup is the sole owner of a
true cold fetch and builds `jackin-xtask` once for artifact reuse by downstream
gates. The same artifact carries the pinned Cargo CI tools so fan-out jobs do
not independently install `nextest`, `deny`, `shear`, `audit`, `fuzz`, `hack`,
or `sccache`. GitHub and Velnor use the same workflow, commands, cache keys, and
failure policy; Velnor may be faster only because its runner-local state
persists. The prepared xtask also has a seven-day artifact keyed by its actual
transitive source inputs, Cargo graph/configuration, toolchain, operating
system, and architecture. Unrelated workflow or docs edits therefore reuse the
binary instead of compiling it again. Runner configuration selects one
canonical cache writer: GitHub when it participates, or Velnor for a
Velnor-only dispatch. Both lanes restore that same portable output.

A cold bootstrap is recorded as a cache miss, not hidden by raising the target.
Fan-out jobs stay offline and consume the warmup result. Cross-run compiler
result sharing is deferred to the [Shared CI compiler cache](<docs/content/docs/roadmap/(infrastructure)/shared-ci-compiler-cache.mdx>)
roadmap item. `jackin-xtask affected-crates` reads the
Cargo metadata graph and maps a diff to changed crates plus their transitive
reverse workspace dependents. Workspace-wide inputs and unrecognized Rust paths
fail safe to every crate. Each selected cache miss owns one job and one
target-cache namespace, including default/all-feature checks, clippy, nextest, doctests,
MSRV, applicable powerset/benchmark/fuzz checks, and conditional Docker E2E.
Scheduling follows the reverse dependency closure; the input-identical target
artifact key follows the forward dependency closure. Each seven-day artifact
stores the crate's first-party and third-party fingerprints, libraries,
binaries, build outputs, and test executables. MSRV outputs live under
`target/msrv`, so the older compiler cannot overwrite current-toolchain
artifacts immediately before packing. This preserves crate-specific Cargo
feature, profile, and toolchain variants without placing 26 mostly overlapping
archives in GitHub's small repository cache quota. The configured canonical
writer publishes one output for both GitHub and Velnor to restore.

An exact target-key miss first restores that crate's latest successful target
as a seed. Cargo still validates every fingerprint and rebuilds changed
first-party outputs, but unchanged registry and git dependencies remain
available instead of being recompiled solely because the crate source key
changed. A successful miss publishes both the new exact target and a small
latest-target pointer; it does not duplicate the target archive.

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

Every first-party `cargo xtask lint <gate>`, `docs <gate>`, `research check`, and
`roadmap audit` command accepts `--format json|github`. JSON violations use
schema 1 with `file`, nullable `line`, `message`, `fix`, and exact `rerun`
fields. The shared problem matcher is registered by CI for human/GitHub output.

### Snapshot review policy

Changed `.snap` files are enumerated in CI against the PR merge-base with `origin/main` (step summary + job log). Reviewers must acknowledge each listed snapshot; hand-edited snapshots that merely match buggy output are rejected in review. Pending files (`*.pending-snap`) still fail CI. Prefer `cargo insta review` / `cargo insta accept` over hand-editing `.snap` bodies.


Every crate is verified by `cargo nextest run -p <crate>`. Exceptions worth naming: `jackin` E2E tests need `--features e2e --profile docker-e2e`; crate-owned doctests use `cargo test --doc -p <crate> --locked`. The machine-checkable per-member map is also emitted by `cargo xtask health --format json` under `verification_map`.

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

CI nextest uses `[profile.ci]` (`.config/nextest.toml`): fixed 2 retries with a 1s delay and `final-status-level = "flaky"`. A pass-on-retry is reported as flaky — never silently absorbed. The matrix is exactly one job per affected crate that requires testing; an input-identical successful result is resolved before matrix expansion and does not consume a runner. It has no shards, multi-crate buckets, or second jobs for crate-specific MSRV, clippy, benchmarks, powersets, fuzzing, or Docker tests. The `jackin` job owns its conditional Docker E2E steps. Every crate job uploads `target/nextest/ci/junit.xml` and fails if any flaky test is not listed in the shrink-only quarantine ledger `flaky-tests.toml` (repo root; each `[[test]]` needs `name`, `owner`, `reason`, `since`). Prefer fixing the flake over quarantining.

An input-identical successful crate result is reused for seven days before any
toolchain, registry, or target restore. Its key includes the crate's forward
workspace dependency closure, Cargo and toolchain inputs, runner platform,
feature and Docker modes, and the test-workflow contract. The selector removes
a hit before matrix expansion, so queued runner capacity is reserved for real
cache misses and the routing summary records every reuse. Any changed input
runs the complete contract in one dedicated crate job and publishes a
replacement marker from the configured canonical writer for both GitHub and
Velnor to consume. Docker inputs and
execution modes affect only the `jackin` result, because that crate owns the
single conditional Docker E2E path.

Construct Image uses the same pattern at its own correctness boundary. Its
seven-day marker covers the construct sources, Docker and Cargo/tool inputs,
publish-versus-rehearsal mode, and requested runner lanes. An unrelated commit
therefore does not rebuild unchanged amd64/arm64 images, while any construct
input or lane-mode change runs the complete platform matrix.

Docs prepares the pinned `codebook-lsp` binary once and publishes a seven-day
platform/tool-contract artifact. The docs and source spell jobs download that
same binary instead of each invoking Cargo through mise; only a genuinely new
Codebook version or platform may take the source-build fallback.
The repository-link job restores the same prepared `jackin-xtask` artifact as
CI and installs only lychee, so it does not maintain a second Rust build/cache
path for identical source inputs.

Required PR/main CI runs the real
`jackin_load_ctrl_q_yes_exits_cold_build_quickly` Docker smoke inside the
`jackin` crate job. Scheduled hygiene runs the complete serialized Docker E2E
suite, including every chaos scenario. This keeps a real construct/launch path
on every relevant change without placing the suite's measured four-minute
runtime on the required pipeline.

Affected crate jobs compile and smoke each owned fuzz target for five seconds.
Scheduled hygiene retains the 120-300 second campaigns for the same targets,
including the nightly AddressSanitizer run.

Junit artifacts are named `nextest-junit-<crate>-<lane>` and seed the Phase 0 suite-wall-time baseline once measured.
Each package job runs `cargo xtask lint ratchet --only suite-time`; it must not invoke
the all-family ratchet because unrelated artifact providers can add hidden build
work. The telemetry conformance job similarly owns generation of
`target/telemetry-volume-ratchet.json` and enforces only `export-volume` after
the producing test completes. Missing artifact-backed inputs skip outside their
owning job instead of launching nested Cargo commands.

### Fuzz targets

| Target | Crate path | Smoke (PR / ci.yml) | Long (hygiene) |
|---|---|---|---|
| `damage_grid_process` | `crates/jackin-term/fuzz` | 5s `--sanitizer none` | 300s; ASan 300s |
| `config_migrate` | `crates/jackin-config/fuzz` | 5s | 120s |
| `workspace_migrate` | `crates/jackin-config/fuzz` | 5s | 120s |
| `manifest_migrate` | `crates/jackin-manifest/fuzz` | 5s | 120s |
| `manifest_validate` | `crates/jackin-manifest/fuzz` | 5s | 120s |
| `env_resolve` | `crates/jackin-env/fuzz` | 5s | 120s |
| `decode_frames` | `crates/jackin-protocol/fuzz` | 5s | 120s |

Local smoke (nightly + cargo-fuzz via mise):

```sh
cd crates/jackin-term && cargo fuzz run --sanitizer none damage_grid_process -- -max_total_time=30
cd crates/jackin-config && cargo fuzz run --sanitizer none config_migrate -- -max_total_time=30
```

Committed seeds live under each fuzz crate's `corpus/<target>/` (fixture-derived TOML for migrate/validate targets; tag+payload frames for `decode_frames`). **Promotion rule:** when a fuzzer finds a crash or hang, (1) minimize with `cargo fuzz cmin <target>` / `tmin`, (2) commit the minimized input under `corpus/<target>/`, (3) add a deterministic regression test in the owning crate that feeds the same bytes (or the decoded fixture) so the finding never re-enters CI only via the fuzzer. Do not grow corpora with non-minimized corpus dirs from long runs without `cmin`.

Migration fixture harness ([`crates/jackin/tests/migration_fixtures.rs`](crates/jackin/tests/migration_fixtures.rs)) enforces golden equality against `after.toml` and second-pass idempotence for every config/workspace/manifest fixture.

### Full DinD E2E lane (scheduled)

Hygiene job `dind-chaos` runs the complete nine-test suite against real Docker,
including the three seeded fault scenarios
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
