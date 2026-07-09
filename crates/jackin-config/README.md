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

- `src/schema.rs`, `src/app_config.rs`, `src/mounts.rs`, `src/auth.rs`, `src/sensitive.rs` — schema + sensitive values
- `src/resolve.rs`, `src/planner.rs`, `src/persist.rs`, `src/paths.rs` — resolution, planning, persistence
- `src/migrations.rs`, `src/versions.rs`, `src/validation.rs` — versioning + validation
- `src/test_support.rs` — deterministic config builders for tests
- subdirs (`migrations/`, `mounts/`, `resolve/`, `app_config/`, `paths/`, `fixtures/`, `editor/`) — module bodies + tests

## Public API

Resolved config/workspace types and the resolution + migration entry points consumed by `jackin-runtime`, `jackin-instance`, `jackin-isolation`, and the CLI. Schema changes are versioned — see `migrations` and the workspace schema-versioning rules.

## How to verify

```sh
cargo nextest run -p jackin-config
cargo clippy -p jackin-config --all-targets -- -D warnings
```

