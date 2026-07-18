# jackin-config

Configuration schema, validation, migration, and workspace resolution for jackin❯. Owns the shape of `config.toml` / per-workspace config and turns it into resolved, validated structures the rest of the system consumes.

Merges the former `config/` and `workspace/` concerns into one crate to dissolve the config↔workspace mutual cycle that previously blocked clean crate extraction.

## What this crate owns

- The config and workspace schema (`schema`, `app_config`, `mounts`), sensitive-value handling (`sensitive`), and auth model (`auth`).
- Resolution (`resolve`, `planner`) and persistence (`persist`, `paths`) of operator + workspace configuration.
- Versioned migrations (`migrations`, `versions`) and validation (`validation`).
- `test_support` builders for deterministic config fixtures.

## Architecture tier and allowed dependencies

**L0 domain (schema).** Allowed workspace dependencies: `jackin-core`. No presentation, no infrastructure adapters, no observability — those live above.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`schema.rs`](src/schema.rs) | config + workspace schema | — |
| [`app_config.rs`](src/app_config.rs) · [`app_config/`](src/app_config) | app-config model | [`tests.rs`](src/app_config/tests.rs) |
| [`mounts.rs`](src/mounts.rs) · [`mounts/`](src/mounts) | mounts schema | [`tests.rs`](src/mounts/tests.rs) |
| [`auth.rs`](src/auth.rs) | auth model | — |
| [`sensitive.rs`](src/sensitive.rs) | sensitive-value handling | — |
| [`resolve.rs`](src/resolve.rs) · [`resolve/`](src/resolve) | resolution | [`tests.rs`](src/resolve/tests.rs) |
| [`planner.rs`](src/planner.rs) | planning | — |
| [`persist.rs`](src/persist.rs) | persistence | — |
| [`paths.rs`](src/paths.rs) · [`paths/`](src/paths) | config paths | [`tests.rs`](src/paths/tests.rs) |
| [`migrations.rs`](src/migrations.rs) · [`migrations/`](src/migrations) | versioned migrations | [`tests.rs`](src/migrations/tests.rs) |
| [`versions.rs`](src/versions.rs) | version constants | — |
| [`validation.rs`](src/validation.rs) | validation | — |
| [`telemetry.rs`](src/telemetry.rs) · [`telemetry/`](src/telemetry) | bounded config telemetry ownership | [`tests.rs`](src/telemetry/tests.rs) |
| [`editor.rs`](src/editor.rs) · [`editor/`](src/editor) | editor config | [`tests.rs`](src/editor/tests.rs) |
| [`test_support.rs`](src/test_support.rs) | deterministic config builders | — |

## Public API

Resolved config/workspace types and the resolution + migration entry points consumed by `jackin-runtime`, `jackin-instance`, `jackin-isolation`, and the CLI. Schema changes are versioned — see `migrations` and the workspace schema-versioning rules.

## How to verify

```sh
cargo nextest run -p jackin-config
cargo clippy -p jackin-config --all-targets -- -D warnings
```
