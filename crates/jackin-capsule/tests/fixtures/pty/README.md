# Recorded PTY fixtures for the render-conformance harness

Binary PTY byte streams (`<agent>-<scenario>.bin`) replayed by `crates/jackin-capsule/src/daemon/tests.rs`.
Record a scenario to an operator-selected temporary path by setting
`JACKIN_PTY_FIXTURE_CAPTURE=<capture.bin>`, review the raw bytes for secrets and
unstable content, then copy them byte-for-byte into a fixture:

```sh
cargo xtask pty-fixture <capture.bin> <out.bin>
```

The capture gate is explicit test tooling and is disabled when the variable is
unset. It is not a telemetry or log artifact. See the "Recording capsule
render-conformance fixtures" section in the repository's `TESTING.md`.

Committed fixtures:

- `codex-version.bin` — real Codex CLI version output, extracted through `cargo xtask pty-fixture` from
  `crates/jackin-term/tests/fixtures/real/codex-version.vt`.
- `vim-tiny-open-edit-quit.bin` — real Vim alt-screen redraw/edit/quit PTY capture, extracted through
  `cargo xtask pty-fixture` from
  `crates/jackin-term/tests/fixtures/real/vim-tiny-open-edit-quit.bin`.
