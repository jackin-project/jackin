# AGENTS.md — jackin

The jackin❯ CLI — the operator-facing binary. Entry/glue layer that assembles the stack.

## Rules (this crate)

- Keep it thin: delegate to lower crates; do not implement domain logic in the glue layer.
- CLI-first surface: new user-visible flags/commands land on the CLI (often exclusively); the console is a deliberately simpler front for common flows. When comparing, make that relationship explicit — do not imply feature-equivalence.
- Brand is `jackin❯` in every rich-text mention; bare `jackin` only for code identifiers/paths (`RULES.md`).
- Route command output through explicit writer helpers (`print_stdout`/`print_stderr`/`unwrap`/`expect`/`panic` are denied by workspace lints).

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
