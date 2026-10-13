# jackin-runtime-launch-dry-run

Canonical `--dry-run`
identity resolution
and model projection.

## What this crate owns

- Dry run
  (`dry_run`):
  `DryRunIdentity`,
  `DryRunModelProjection`,
  `resolve_dry_run_identity`,
  `resolve_dry_run_model_projection` —
  admission-equivalent
  identity and per-instance
  model projection for
  `--dry-run` plans.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`dry_run.rs`](src/dry_run.rs) | identity + projection | hub `launch::dry_run` suite (`case_01`, `case_02`) |

## Public API

`dry_run::DryRunIdentity`,
`dry_run::DryRunModelProjection`,
`dry_run::resolve_dry_run_identity`,
`dry_run::resolve_dry_run_model_projection`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-dry-run
cargo clippy -p jackin-runtime-launch-dry-run --all-targets -- -D warnings
```
