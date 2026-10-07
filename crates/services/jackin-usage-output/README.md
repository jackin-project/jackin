# jackin-usage-output

Plain locked stdout/stderr writers for capsule CLI and entrypoint
output. Dependency-free (`std` only). Re-exported by `jackin-usage`
as `output`.

## What this crate owns

- Lines (`stdout_line`, `stderr_line`): locked line writers over
  `format_args!`.
- Fragments (`stdout_fragment`, `stdout_empty_line`): partial and
  blank output.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | writer surface | — |

## Public API

`stdout_line`, `stdout_empty_line`, `stdout_fragment`,
`stderr_line`.

## How to verify

```sh
cargo nextest run -p jackin-usage-output
cargo clippy -p jackin-usage-output --all-targets -- -D warnings
```
