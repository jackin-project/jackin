# jackin-runtime-docker-profile

Docker security profile
resolution and flag emission.

## What this crate owns

- Resolve (`docker_profile`):
  `resolve_profile`,
  `resolve_effective_grants`,
  `ProfileSource`.
- Flags (`docker_profile`):
  `capability_flags`,
  `resource_flags`,
  `readonly_root_flags`,
  `tmpfs_paths`, labels.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`docker_profile.rs`](src/docker_profile.rs) | profile + flags root | `docker_profile/tests/` |
| [`docker_profile/`](src/docker_profile/) | grants/probe/sizes | `docker_profile/tests/` |

## Public API

`docker_profile::resolve_profile`,
`docker_profile::DockerSecurityProfile`,
`docker_profile::EffectiveGrants`,
`docker_profile::validate_grants`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-docker-profile
cargo clippy -p jackin-runtime-docker-profile --all-targets -- -D warnings
```
