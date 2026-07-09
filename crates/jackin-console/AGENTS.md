# AGENTS.md — jackin-console

Canonical host-console product surface — console state, planning, views, effects-as-data.

## Hard rules (this crate)

- **Tier & dependencies:** L3 presentation. Allowed workspace deps: `jackin-config`, `jackin-console-oppicker`, `jackin-core`, `jackin-diagnostics`, `jackin-env`, `jackin-protocol`, `jackin-tui`. Must NOT depend on `jackin-runtime`, `jackin-launch-tui`, or `jackin-capsule` directly.
- **Keep `README.md` current:** update it when structure, public API, console state, or views change (see `crates/AGENTS.md`).
- **Effects-as-data.** The console reaches runtime/Docker through effect types it emits, not direct calls — preserve that boundary so the console stays testable and runtime-free.
- **Pure decisions stay pure.** Product decisions and planning live as pure functions; side-effect adapters are thin. Continue the `jackin-console-oppicker` split pattern for new pickers/planners.

## What lives here vs elsewhere

- This crate owns: console state machine, workspace/mount/services views, mount diff/info, effects-as-data.
- Picker *model/planning* lives in `jackin-console-oppicker`. Design-system components live in `jackin-tui`. 1Password `op` side-effects live in `jackin-env`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
