# AGENTS.md — jackin

The jackin❯ CLI — the operator-facing binary. L4 entry/glue; assembles the stack, implements no domain logic.

## Hard rules (this crate)

- **Tier & dependencies:** L4 entry/glue. Depends on the whole stack; nothing depends on this crate. Keep it thin — delegate to lower crates, do not implement domain logic here.
- **Keep `README.md` current:** update it when the CLI surface, commands, wiring, or module layout change (see `crates/AGENTS.md`).
- **CLI-first surface.** New user-visible flags/commands land on the CLI (often exclusively); the console is a deliberately simpler front for common flows. When comparing, make that relationship explicit — do not imply feature-equivalence.
- **Brand is `jackin❯`.** Every rich-text mention of the product is `jackin❯`; only code identifiers/paths use bare `jackin`. See root `RULES.md`.
- **Output through writers, not `println!`.** Route command output through explicit writer helpers (`print_stdout`/`print_stderr`/`unwrap`/`expect`/`panic` are denied by workspace lints).

## What lives here vs elsewhere

- This crate owns: CLI parsing, app wiring, console/prompt/preflight entry, role-authoring + workspace commands, Warp integration, the top-level error type.
- Domain/infra behavior lives in the lower crates it calls. The console *product surface* lives in `jackin-console`; launch *orchestration* in `jackin-runtime`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
