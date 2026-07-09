# AGENTS.md — jackin-launch-tui

Launch cockpit TUI — the presentation surface for `jackin load`.

## Rules (this crate)

- Render only: this crate renders progress/output events emitted by `jackin-runtime`; it does not orchestrate launch. Do not pull runtime logic in.
- Use the design system: render through `jackin-tui` components and tokens; do not construct bespoke `ratatui` widgets the design system already owns.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
