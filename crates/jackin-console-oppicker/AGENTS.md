# AGENTS.md — jackin-console-oppicker

Pure model and planning helpers for the 1Password picker.

## Hard rules (this crate)

- **Tier & dependencies:** presentation-adjacent pure model. Allowed workspace deps: `jackin-core`, `jackin-diagnostics`, `jackin-tui`. No `op`, no `tokio`, no filesystem.
- **Keep `README.md` current:** update it when structure, public API, picker state, or planning change (see `crates/AGENTS.md`).
- **Stay side-effect-free.** All `op` CLI calls and I/O live in `jackin-env`; this crate is the pure decision/planning half. Do not introduce side effects here.
- **This is the extraction template.** Future console pickers/planners follow the same pure-model-vs-adapter split; keep this crate the clean reference.

## What lives here vs elsewhere

- This crate owns: picker state machine, input handling, load/planning helpers.
- `op` side-effects live in `jackin-env`. Console wiring lives in `jackin-console`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
