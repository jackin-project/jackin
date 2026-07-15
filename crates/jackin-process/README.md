# jackin-process

Shared subprocess transport for jackin❯: capture, timeout, retry, and exit status.

## What this crate owns

- `ExecRequest` / `ExecResult` and the async + sync run helpers used by xtask, capsule probes, and runtime shell execution.
- Timeout and retry policy knobs only — **not** redaction, protected-value classification, environment policy, or telemetry (callers own those).

## Architecture tier

**T0 foundational.** Allowed deps: external crates only (`anyhow`, `tokio`). No jackin❯ workspace dependencies.

## Structure

| Module | Owns |
|---|---|
| [`lib.rs`](src/lib.rs) | request/result types, async core, sync facade |

## How to verify

```sh
cargo nextest run -p jackin-process
cargo clippy -p jackin-process --all-targets -- -D warnings
```
