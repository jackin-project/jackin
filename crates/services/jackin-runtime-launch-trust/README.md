# jackin-runtime-launch-trust

Workspace trust
seeding: Codex
project trust and
mise paths.

## What this crate owns

- Trust (`trust`):
  `seed_codex_project_trust`,
  `workspace_mise_trusted_config_paths`,
  `inject_workspace_mise_env`,
  `MISE_TRUSTED_CONFIG_PATHS_ENV`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`trust.rs`](src/trust.rs) | seeding | `trust/tests/` |
| [`trust/`](src/trust/) | test suites | `trust/tests/` |

## Public API

`trust::seed_codex_project_trust`,
`trust::inject_workspace_mise_env`,
`trust::workspace_mise_trusted_config_paths`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-trust
cargo clippy -p jackin-runtime-launch-trust --all-targets -- -D warnings
```
