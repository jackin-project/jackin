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

## Recording capsule render-conformance fixtures

Capsule echo-back harness ([crates/jackin-capsule/src/daemon/tests.rs](crates/jackin-capsule/src/daemon/tests.rs)) replays PTY byte streams through multiplexer, asserts emitted frames reproduce pane model on virtual client terminal. Synthetic streams live in harness; real-agent fixtures recorded from `--debug` run:

1. Run session with `--debug` (e.g. `cargo run --bin jackin -- console --debug`), exercise agent. Note run id CLI prints.
2. Extract one session's PTY stream from run log into binary fixture:

   ```sh
   cargo xtask pty-fixture ~/.jackin/data/diagnostics/runs/<run-id>.jsonl <session-label> \
     crates/jackin-capsule/tests/fixtures/pty/<agent>-<scenario>.bin
   ```

   Session label = pane label in capsule tab (e.g. `Codex`). Extractor also accepts raw in-container `multiplexer.log`.
3. Reference fixture from harness scenario with `include_bytes!`.

## Walking the operator through local validation

Every `jackin <subcommand>` invocation in manual validation MUST include `--debug`. Includes `cargo run --bin jackin -- <subcommand> --debug` from checkout.

`--debug` captures every external command (`docker`, `git`, `id`, etc.) with output plus `[jackin debug ...]` instrumentation into `~/.jackin/data/diagnostics/runs/<run-id>.jsonl`. CLI prints run id. When something misbehaves, ask for run id — agent reads JSONL to localize issue, not a pasted terminal scrollback.

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
