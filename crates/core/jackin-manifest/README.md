# jackin-manifest

Role manifest (`jackin.role.toml`) loading, validation, and migration. Owns the contract a role repository declares and the code that turns a role repo into a validated, current-shape manifest.

## What this crate owns

- Manifest loading and parsing (`manifest`, `repo`, `repo_contract`).
- Validation against the role-manifest schema (`validate`).
- Versioned manifest migrations (`migrations`) so older role repos keep working.

## Architecture tier and allowed dependencies

**L0 domain.** Allowed workspace dependencies: `jackin-core`, `jackin-config`. No infrastructure or presentation dependencies — manifest handling stays a pure domain concern.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`manifest.rs`](src/manifest.rs) · [`manifest/`](src/manifest) | manifest types + loading | [`tests.rs`](src/manifest/tests.rs) |
| [`repo.rs`](src/repo.rs) · [`repo/`](src/repo) | role-repo contract + access | [`tests.rs`](src/repo/tests.rs) |
| [`repo_contract.rs`](src/repo_contract.rs) · [`repo_contract/`](src/repo_contract) | repo contract | [`tests.rs`](src/repo_contract/tests.rs) |
| [`validate.rs`](src/validate.rs) · [`validate/`](src/validate) | schema validation | [`tests.rs`](src/validate/tests.rs) |
| [`migrations.rs`](src/migrations.rs) · [`migrations/`](src/migrations) | versioned migrations | [`tests.rs`](src/migrations/tests.rs) |

## Public API

Validated manifest types and the load/validate/migrate entry points consumed by `jackin-runtime`, `jackin-image`, and `jackin-instance`. Migrations are idempotent and migrated manifests must re-validate.

## How to verify

```sh
cargo nextest run -p jackin-manifest
cargo clippy -p jackin-manifest --all-targets -- -D warnings
```

