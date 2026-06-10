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

The capsule's echo-back render-conformance harness
(`crates/jackin-capsule/src/daemon/render_conformance_tests.rs`) replays PTY byte streams through
the multiplexer and asserts the emitted frames reproduce the pane model on a virtual client
terminal. Synthetic streams live in the harness; real-agent fixtures are recorded from a `--debug`
run:

1. Run a session with `--debug` (for example `cargo run --bin jackin -- console --debug`) and
   exercise the agent. Note the run id the CLI prints.
2. Extract one session's PTY stream from the run log into a binary fixture:

   ```sh
   cargo xtask pty-fixture ~/.jackin/data/diagnostics/runs/<run-id>.jsonl <session-label> \
     crates/jackin-capsule/tests/fixtures/pty/<agent>-<scenario>.bin
   ```

   The session label is the pane label shown in the capsule tab (e.g. `Codex`). The extractor also
   accepts a raw in-container `multiplexer.log`.
3. Reference the fixture from a harness scenario with `include_bytes!`.

## Merge-readiness Verification

Do not run formatting, clippy, and the full test suite before every commit by
default. Run the full verification suite when a pull request is ready to be
merged, or earlier only when the operator explicitly asks for it. CI runs both
the default feature set and all enabled features so feature-gated tests do not
silently drift:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run
cargo nextest run --all-features
```

All commands must pass with zero warnings and zero failures.
If formatting fails, run `cargo fmt` to fix it.
