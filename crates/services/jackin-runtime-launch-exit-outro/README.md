# jackin-runtime-launch-exit-outro

Exit outro
rendering from
an observed
universe boundary.

## What this crate owns

- Rendering
  (`exit_outro`):
  `render_exit_observation` —
  still-running summary
  plus the two-screen
  outro (decelerating
  warp, then closing
  caption) from an
  already-observed
  `(running, ExitClaim,
  force_outro, data_dir)`.
  The hub keeps the
  `universe` observation
  (S7 split 95,
  observation-inversion).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`exit_outro.rs`](src/exit_outro.rs) | outro rendering | hub `launch` suite (`launch/tests/case_21.rs`, `case_22.rs`) |

## Public API

`exit_outro::render_exit_observation`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-exit-outro
cargo clippy -p jackin-runtime-launch-exit-outro --all-targets -- -D warnings
```
