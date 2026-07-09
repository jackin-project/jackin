# AGENTS.md — jackin-manifest

Role manifest (`jackin.role.toml`) loading, validation, and migration.

## Hard rules (this crate)

- **Tier & dependencies:** L0 domain. Allowed workspace deps: `jackin-core`, `jackin-config`. No infrastructure or presentation dependencies.
- **Keep `README.md` current:** update it when structure, public API, module layout, or responsibilities change (see `crates/AGENTS.md`).
- **Migrations are idempotent.** A migrated manifest must re-validate; never mutate in a way that needs a second pass. Unknown fields follow the documented schema policy.
- **Invalid input never panics.** Validation returns structured errors; do not `unwrap`/`expect` over manifest input (enforced by workspace lints anyway).

## What lives here vs elsewhere

- This crate owns: the role-manifest contract, loading, validation, migration.
- The role-repo author surface schema is documented under Role Authoring docs (role authors write `jackin.role.toml`). Image/derived-image concerns live in `jackin-image`. Instance lifecycle lives in `jackin-instance`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
