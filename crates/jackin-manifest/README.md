# jackin-manifest

Role manifest (`jackin.role.toml`) loading, validation, and migration. Owns the contract a role repository declares and the code that turns a role repo into a validated, current-shape manifest.

## What this crate owns

- Manifest loading and parsing (`manifest`, `repo`, `repo_contract`).
- Validation against the role-manifest schema (`validate`).
- Versioned manifest migrations (`migrations`) so older role repos keep working.

## Architecture tier and allowed dependencies

**L0 domain.** Allowed workspace dependencies: `jackin-core`, `jackin-config`. No infrastructure or presentation dependencies — manifest handling stays a pure domain concern.

## Structure

- `src/manifest.rs` — manifest types and loading
- `src/repo.rs`, `src/repo_contract.rs` — role-repo contract and access
- `src/validate.rs` — schema validation
- `src/migrations.rs` — versioned migrations
- subdirs (`migrations/`, `validate/`, `manifest/`, `repo/`, `repo_contract/`) — module bodies + tests

## Public API

Validated manifest types and the load/validate/migrate entry points consumed by `jackin-runtime`, `jackin-image`, and `jackin-instance`. Migrations are idempotent and migrated manifests must re-validate.

## How to verify

```sh
cargo nextest run -p jackin-manifest
cargo clippy -p jackin-manifest --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.
