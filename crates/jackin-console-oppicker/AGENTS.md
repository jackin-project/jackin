# AGENTS.md — jackin-console-oppicker

Pure model and planning helpers for the 1Password picker.

## Rules (this crate)

- Stay side-effect-free: all `op` CLI calls and I/O live in `jackin-env`; this crate is the pure decision/planning half.
- This crate is the extraction template — future console pickers/planners follow the same pure-model-vs-adapter split; keep it the clean reference.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
