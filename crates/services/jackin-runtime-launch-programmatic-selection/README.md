# jackin-runtime-launch-programmatic-selection

Ephemeral launch-identity
selection on cloned
configs.

## What this crate owns

- Selection
  (`selection`):
  `with_account_selection`,
  `with_configuration_selection` —
  one-launch account picks
  and exact configuration
  pins validated through
  launch admission.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`selection.rs`](src/selection.rs) | selection | hub `launch::programmatic` suite (`case_01`, `case_02`) |

## Public API

`selection::with_account_selection`,
`selection::with_configuration_selection`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-programmatic-selection
cargo clippy -p jackin-runtime-launch-programmatic-selection --all-targets -- -D warnings
```
