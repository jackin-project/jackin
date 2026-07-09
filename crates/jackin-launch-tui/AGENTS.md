# AGENTS.md — jackin-launch-tui

Launch cockpit TUI — the presentation surface for `jackin load`.

## Hard rules (this crate)

- **Tier & dependencies:** presentation. Allowed workspace deps: `jackin-core`, `jackin-diagnostics`, `jackin-tui`, `jackin-build-meta`. No runtime or infrastructure dependencies.
- **Keep `README.md` current:** update it when structure, public API, progress views, or dialogs change (see `crates/AGENTS.md`).
- **Render only.** This crate renders progress/output events emitted by `jackin-runtime`; it does not orchestrate launch. Do not pull runtime logic in.
- **Use the design system.** Render through `jackin-tui` components and tokens; do not construct bespoke `ratatui` widgets the design system already owns.

## What lives here vs elsewhere

- This crate owns: launch progress rendering, output streaming, standalone dialog sink, launch TUI shell.
- Launch *orchestration* lives in `jackin-runtime`. Design-system components live in `jackin-tui`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
