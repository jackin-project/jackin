# jackin-runtime-launch-post-run-failure

Jackin launch
post-run failure
telemetry helper.

## What this crate owns

- Post-run failure
  (`post_run_failure`):
  `emit_post_run_failure`
  — reports a failed
  post-run step to
  diagnostics (the
  isolation-firewall
  breach in allowlist
  network mode when
  `is_firewall` is set,
  otherwise a no-op)
  (S7 split 117).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`post_run_failure.rs`](src/post_run_failure.rs) | post-run failure emit | hub `launch_runtime` post-run path (no dedicated suite) |

## Public API

`post_run_failure::emit_post_run_failure`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-post-run-failure
cargo clippy -p jackin-runtime-launch-post-run-failure --all-targets -- -D warnings
```
