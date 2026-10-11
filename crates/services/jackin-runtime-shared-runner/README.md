# jackin-runtime-shared-runner

Shared runner
handle.

## What this crate owns

- Runner
  (`shared_runner`):
  `SharedCommandRunner` —
  cloneable handle
  over one serialized
  command stream.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`shared_runner.rs`](src/shared_runner.rs) | handle | none yet |

## Public API

`shared_runner::SharedCommandRunner`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-shared-runner
cargo clippy -p jackin-runtime-shared-runner --all-targets -- -D warnings
```
