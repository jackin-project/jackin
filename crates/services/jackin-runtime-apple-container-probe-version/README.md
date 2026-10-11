# jackin-runtime-apple-container-probe-version

Jackin apple-container
`container` CLI version
probe.

## What this crate owns

- Version probe
  (`probe_version`):
  `probe_version` —
  shell out to
  `container --version`
  through the shared
  transport, `None`
  when the CLI is
  missing (S7 split 112).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`probe_version.rs`](src/probe_version.rs) | version probe | hub `apple_container` launch path (no dedicated suite) |

## Public API

`probe_version::probe_version`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-probe-version
cargo clippy -p jackin-runtime-apple-container-probe-version --all-targets -- -D warnings
```
