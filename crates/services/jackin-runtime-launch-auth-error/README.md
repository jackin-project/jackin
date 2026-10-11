# jackin-runtime-launch-auth-error

Credential source
display and proxy
env helpers.

## What this crate owns

- Display
  (`auth_error`):
  `auth_token_source_reference` —
  `"KEY ← value"`
  source reference.
- Proxy
  (`auth_error`):
  `PROXY_VAR_NAMES`,
  `NO_PROXY_UPPER`,
  `NO_PROXY_LOWER`,
  `is_proxy_env_name`,
  `append_no_proxy_host`,
  `push_env_if_present` —
  container env
  composition.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`auth_error.rs`](src/auth_error.rs) | display + proxy | — |

## Public API

`auth_error::auth_token_source_reference`,
`auth_error::append_no_proxy_host`,
`auth_error::is_proxy_env_name`,
`auth_error::push_env_if_present`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-auth-error
cargo clippy -p jackin-runtime-launch-auth-error --all-targets -- -D warnings
```
