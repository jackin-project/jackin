# jackin-runtime-launch-capsule-setup

Launch capsule
setup: config,
auth bindings,
models, env
transport.

## What this crate owns

- Setup
  (`capsule_setup`):
  `capsule_config`,
  `capsule_config_contents`,
  `prepare_socket_dir` —
  config + dirs.
- Bindings
  (`capsule_setup`):
  `instance_auth_bindings`,
  `resolved_instance_models`,
  `resolved_instance_efforts`,
  `apply_instance_dirs`,
  `apply_account_models` —
  per-instance resolution.
- Transport
  (`capsule_setup`):
  `HostEnvFile`,
  `HostEnvTransport`,
  `extract_host_env_entries` —
  private env files.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`capsule_setup.rs`](src/capsule_setup.rs) | setup | `capsule_setup/tests/` |

## Public API

`capsule_setup::capsule_config`,
`capsule_setup::instance_auth_bindings`,
`capsule_setup::prepare_host_env_transport`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-capsule-setup
cargo clippy -p jackin-runtime-launch-capsule-setup --all-targets -- -D warnings
```
