# AGENTS.md — jackin-manifest

Role manifest (`jackin.role.toml`) loading, validation, and migration.

## Rules (this crate)

- Migrations are idempotent: a migrated manifest must re-validate. Unknown fields follow the documented schema policy.
- Invalid input never panics — validation returns structured errors (workspace lints also deny `unwrap`/`expect` over input).

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
