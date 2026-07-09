# AGENTS.md — jackin-tui

Shared TUI design system: tokens, models, components, and the runtime used by jackin❯'s terminal surfaces.

## Rules (this crate)

- Backend-neutral by design: token types, component state, and render helpers stay free of a specific backend. The `runtime` traits are the dispatch point console/launch/capsule loop through — keep them operational, not type-only.
- Cross-cutting TUI behavior is documented: focusability, navigation, color, modal sizing, and hints live under `docs/content/docs/reference/tui/`; a behavior change ships the matching doc update in the same PR.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
