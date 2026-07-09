# AGENTS.md — jackin-dev

The `jackin-dev` PR-verification + contributor tooling binary.

## Rules (this crate)

- Stay self-contained: this tool runs against a fresh checkout before the workspace is necessarily built — do not add assumptions about a built workspace, and keep it free of jackin❯ runtime-crate dependencies.
- `--version` must work offline — keep version reporting cheap and dependency-light.
- Coordinate CLI changes with `.github/AGENTS.md` and the PR-verification docs.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
