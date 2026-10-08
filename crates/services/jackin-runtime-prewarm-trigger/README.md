# jackin-runtime-prewarm-trigger

Background image
and DinD sidecar
prewarm triggers
for runtime.

## What this crate owns

- Targets
  (`prewarm_trigger`):
  `BackgroundPrewarmTarget`,
  `background_prewarm_targets` —
  per-workspace role/agent
  resolution from saved
  configuration.
- Sweeps
  (`prewarm_trigger`):
  `spawn_background_image_prewarm`,
  `spawn_background_sidecar_prewarm` —
  best-effort detached
  refresh of stale baked
  images and the kept
  `DinD` sidecar.
- Attempt
  (`prewarm_trigger`):
  `SidecarPrewarmOutcome`,
  `classify_sidecar_prewarm_attempt` —
  sidecar-attempt outcome
  vocabulary shared with
  the hub suite.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`prewarm_trigger.rs`](src/prewarm_trigger.rs) | prewarm targets + sweeps | hub `prewarm_trigger` suite (`prewarm_trigger/tests.rs`) |

## Public API

`prewarm_trigger::BackgroundPrewarmTarget`,
`prewarm_trigger::background_prewarm_targets`,
`prewarm_trigger::spawn_background_image_prewarm`,
`prewarm_trigger::spawn_background_sidecar_prewarm`,
`prewarm_trigger::SidecarPrewarmOutcome`,
`prewarm_trigger::classify_sidecar_prewarm_attempt`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-prewarm-trigger
cargo clippy -p jackin-runtime-prewarm-trigger --all-targets -- -D warnings
```
