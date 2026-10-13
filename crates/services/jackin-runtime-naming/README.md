# jackin-runtime-naming

Container naming conventions
and Docker label constants.

## What this crate owns

- Labels (`naming`):
  `LABEL_MANAGED`,
  `LABEL_KIND_ROLE`,
  `LABEL_KEEP_AWAKE`, plus
  role/image key labels.
- Helpers (`naming`):
  `matching_family`,
  `format_role_display`,
  image-name re-exports.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`naming.rs`](src/naming.rs) | labels + helpers | `naming/tests.rs` |
| [`naming/`](src/naming/) | test suites | `naming/tests.rs` |

## Public API

`naming::matching_family`,
`naming::format_role_display`,
`naming::LABEL_MANAGED`,
`naming::LABEL_KIND_ROLE`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-naming
cargo clippy -p jackin-runtime-naming --all-targets -- -D warnings
```
