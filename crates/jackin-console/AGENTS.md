# AGENTS.md — jackin-console

Canonical host-console product surface — console state, planning, views, effects-as-data.

## Rules (this crate)

- Effects-as-data: the console reaches runtime/Docker through effect types it emits, not direct calls. Preserve that boundary so the console stays testable and runtime-free.
- Pure decisions stay pure: product decisions and planning are pure functions; side-effect adapters are thin. Follow the `jackin-console-oppicker` split pattern for new pickers/planners.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
