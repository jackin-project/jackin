# jackin-runtime-launch-account-config

Launch account
configuration:
provider files,
catalogs,
fingerprints.

## What this crate owns

- Config
  (`account_config`):
  `configure_accounts`,
  `opencode_model` —
  account file
  publication.
- Bounds
  (`account_config::private_config_bounds`):
  size-bounded reads
  for private config.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`account_config.rs`](src/account_config.rs) | config | `account_config/tests/` |
| [`account_config/`](src/account_config/) | bounds + suites | `account_config/tests/` |

## Public API

`account_config::configure_accounts`,
`account_config::opencode_model`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-account-config
cargo clippy -p jackin-runtime-launch-account-config --all-targets -- -D warnings
```
