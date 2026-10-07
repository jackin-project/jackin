# jackin-runtime-launch-dind

Docker network
creation, DinD
sidecar launch,
and retained-sidecar
prewarm state
for launch.

## What this crate owns

- Sidecar
  (`launch_dind`):
  `run_dind_sidecar_headless`,
  `create_role_network`,
  `DIND_IMAGE` —
  role network
  creation plus
  TLS DinD sidecar
  start with
  readiness wait.
- Prewarm
  (`launch_dind`):
  `prewarm_dind_sidecar_container_with_paths`,
  `adopt_prewarmed_dind_sidecar`,
  `ensure_prewarm_state_identity`,
  `write_prewarmed_dind_state`,
  `prewarmed_dind_state_container_name`,
  `prewarmed_dind_state_is_live`,
  `try_lock_prewarmed_dind`,
  `DindSidecarPrewarm` —
  kept-sidecar prewarm,
  identity-validated
  adoption, and the
  `prewarm-dind.json`
  retained identity
  on disk.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`launch_dind.rs`](src/launch_dind.rs) | sidecar launch + prewarm state | hub `launch::launch_dind` suite (`launch_dind/tests.rs`) + hub `launch` suite |

## Public API

`launch_dind::run_dind_sidecar_headless`,
`launch_dind::create_role_network`,
`launch_dind::DIND_IMAGE`,
`launch_dind::prewarm_dind_sidecar_container_with_paths`,
`launch_dind::adopt_prewarmed_dind_sidecar`,
`launch_dind::ensure_prewarm_state_identity`,
`launch_dind::write_prewarmed_dind_state`,
`launch_dind::prewarmed_dind_state_container_name`,
`launch_dind::prewarmed_dind_state_is_live`,
`launch_dind::try_lock_prewarmed_dind`,
`launch_dind::DindSidecarPrewarm`,
`launch_dind::DindSidecarPrewarmState`,
`launch_dind::AdoptedDindSidecar`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-dind
cargo clippy -p jackin-runtime-launch-dind --all-targets -- -D warnings
```
