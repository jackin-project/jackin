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
| Container/runtime behavior | `cargo xtask ci --e2e` (Docker running) | capsule/runtime PRs |
| Docs/roadmap | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | any docs edit |
| File-size gate | `cargo xtask lint files` (`--format json\|github`) | structure / split PRs |
| Agents gate | `cargo xtask lint agents` (`--format json\|github`) | new crate / AGENTS files |
| TUI snapshots | `cargo nextest run -p jackin-capsule -p jackin-console` (insta snapshots live only in these two crates today) | TUI render changes |

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
