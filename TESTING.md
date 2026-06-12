# Testing

This project uses [cargo-nextest](https://nexte.st) as its test runner.

Install:

```sh
cargo install cargo-nextest --locked
```

Run all tests:

```sh
cargo nextest run
```

Run a specific test:

```sh
cargo nextest run -E 'test(test_name)'
```

Run tests for a specific module:

```sh
cargo nextest run -E 'test(/module::tests/)'
```

Run Docker-backed smoke tests:

```sh
cargo nextest run --all-features
```

Do **not** use `cargo test` — always use `cargo nextest run`.

## Recording capsule render-conformance fixtures

The capsule's echo-back render-conformance harness (`crates/jackin-capsule/src/daemon/render_conformance_tests.rs`) replays PTY byte streams through the multiplexer and asserts emitted frames reproduce the pane model on a virtual client terminal. Synthetic streams live in the harness; real-agent fixtures are recorded from a `--debug` run:

1. Run a session with `--debug` (e.g. `cargo run --bin jackin -- console --debug`) and exercise the agent. Note the run id the CLI prints.
2. Extract one session's PTY stream from the run log into a binary fixture:

   ```sh
   cargo xtask pty-fixture ~/.jackin/data/diagnostics/runs/<run-id>.jsonl <session-label> \
     crates/jackin-capsule/tests/fixtures/pty/<agent>-<scenario>.bin
   ```

   Session label is the pane label shown in the capsule tab (e.g. `Codex`). Extractor also accepts a raw in-container `multiplexer.log`.
3. Reference the fixture from a harness scenario with `include_bytes!`.

## Walking the operator through local validation (agent rule)

When walking the operator through manual validation of a jackin' feature (smoke testing a PR, reproducing a bug, executing a PR test plan), every `jackin <subcommand>` invocation in the recipe MUST include `--debug`. That includes `cargo run --bin jackin -- <subcommand> --debug` while iterating from a checkout.

`--debug` captures every external command the CLI issues (`docker`, `git`, `id`, etc.) with output, plus `[jackin debug ...]` instrumentation, into the diagnostics run file (`~/.jackin/data/diagnostics/runs/<run-id>.jsonl`). The CLI prints a run id the operator can share. Makes the run triage-able: when something misbehaves, operator shares the run id and the agent reads the structured JSONL at `~/.jackin/data/diagnostics/runs/<run-id>.jsonl` to localize the issue without guessing. Ask for the run id, not a pasted terminal scrollback.

For user smoke tests, suggest `jackin console` first, and prefer the `the-architect` role over `agent-smith` when a role choice is needed. From a checkout, the usual operator-facing smoke command is:

```bash
cargo run --bin jackin -- console --debug
```

Use `jackin load` only when the PR specifically needs the load CLI path. Then prefer:

```bash
cargo run --bin jackin -- load the-architect . --debug
```

Do not add `--no-intro` to debug smoke commands. Debug mode already suppresses the intro, so `--debug --no-intro` is redundant noise.

If the operator reports unexpected behavior from a clean (non-debug) run, the FIRST follow-up is to ask them to rerun with `--debug` and share the run id printed at start (agent then reads the run's JSONL file) before proposing fixes.

Does not apply to:

- Inspection commands the operator runs (`pgrep`, `pmset`, `cat`, `ls`) — not `jackin` invocations.
- Production recommendations or scripted automation (debug output too noisy).

Do not list `git diff --check` as PR verification. Not a meaningful acceptance check for jackin' PRs; prefer targeted commands exercising the changed behavior plus CI.

## Merge-readiness Verification

Do not run formatting, clippy, and the full test suite before every commit by default. Run the full verification suite when a PR is ready to merge, or earlier only when the operator explicitly asks. CI runs both the default feature set and all enabled features so feature-gated tests do not silently drift:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run
cargo nextest run --all-features
```

All commands must pass with zero warnings and zero failures. If formatting fails, run `cargo fmt` to fix it.
