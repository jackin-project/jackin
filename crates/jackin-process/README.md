# jackin-process

Shared subprocess transport for jackin❯: capture, timeout, retry, and exit status.

## What this crate owns

- `ExecRequest` / `ExecResult` and the async + sync run helpers used by xtask, capsule probes, and runtime shell execution.
- Independent capture byte limits (16 MiB default per stream), elapsed timeout, retry, and kill/reap ownership for run helpers. Unix runs own an isolated process group through pipe completion.
- Bare spawn helpers return caller-owned streams and lifecycle; explicit capture limits are rejected because these helpers cannot enforce them.
- Callers own redaction, protected-value classification, environment policy, and telemetry.

## Architecture tier

**T0 foundational.** Allowed deps: external crates only (`anyhow`, `tokio`, and `nix` on Unix). No jackin❯ workspace dependencies.

## Structure

| Module | Owns |
|---|---|
| [`lib.rs`](src/lib.rs) | request/result types, async core, sync facade |

## How to verify

```sh
cargo nextest run -p jackin-process
cargo clippy -p jackin-process --all-targets -- -D warnings
```
