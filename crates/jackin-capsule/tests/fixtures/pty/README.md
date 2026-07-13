# Recorded PTY fixtures for the render-conformance harness

Binary PTY byte streams (`<agent>-<scenario>.bin`) replayed by `crates/jackin-capsule/src/daemon/tests.rs`.
Record new streams with `cargo xtask pty-fixture <run.jsonl> <session-label> <out.bin>` from a `--debug`
run, or from a raw `multiplexer.log`; see the "Recording capsule render-conformance fixtures" section in
the repository's `TESTING.md`.

Committed fixtures:

- `codex-version.bin` — real Codex CLI version output, extracted through `cargo xtask pty-fixture` from
  `crates/jackin-term/tests/fixtures/real/codex-version.vt`.
- `vim-tiny-open-edit-quit.bin` — real Vim alt-screen redraw/edit/quit PTY capture, extracted through
  `cargo xtask pty-fixture` from
  `crates/jackin-term/tests/fixtures/real/vim-tiny-open-edit-quit.bin`.
