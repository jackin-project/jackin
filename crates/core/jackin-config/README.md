# jackin-config

Configuration schema, validation, migration, persistence, and workspace resolution for jackin❯.

## What this crate owns

- Config/workspace schema, auth, mounts, and sensitive-path classification.
- Resolution, planning, validation, persistence, and versioned migrations.
- Deterministic config test builders.

## Architecture tier and allowed dependencies

**L0 domain (schema).** No presentation or infrastructure adapters.

## Structure

| Module | Owns |
|---|---|
| `schema`, `app_config`, `auth` | persisted model and auth layers |
| `mounts`, `paths`, `sensitive` | mount/path parsing and classification |
| `resolve`, `planner`, `validation` | pure resolution and validation |
| `persist`, `editor` | atomic I/O and comment-preserving edits |
| `migrations`, `versions` | schema migration chain |
| `telemetry` | bounded config telemetry ownership |
| `test_support` | deterministic fixtures |

## Public API

Resolved config/workspace types plus resolution and migration entry points. Schema changes follow the repository's versioned-config rules.

## Persistence and validation guarantees

- `ConfigEditor` owns an exclusive OS lock on sibling `config.lock` through save/drop. Snapshot readers use `acquire_config_read_lock`. The persistent file never grants ownership; the OS lock does.
- `load_read_only_config_snapshot` is the host-discovery path: it parses global and split/legacy workspace files, applies supported migrations in memory, validates the resulting model, and computes a content generation without creating, migrating, or rewriting anything. It retries a torn tree and reports typed source diagnostics.
- Saves validate, stage, and sync every file before renaming. Staging failure cleans temporary files and preserves the prior tree.
- Unix writes use mode `0600`; all platforms explicitly open, write, and sync staged files. Parent directories are synced after rename.
- Mount validation rejects explicit `.`/`..`. Repo identity, isolation ancestry, and sensitive paths use normalized components; existing host paths are canonicalized by I/O-owning callers.

## How to verify

```sh
cargo nextest run -p jackin-config
cargo clippy -p jackin-config --all-targets -- -D warnings
```
