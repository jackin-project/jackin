# AGENTS.md — jackin-tui

Shared TUI design system: tokens, models, components, and the runtime used by jackin❯'s terminal surfaces (console, launch cockpit, capsule).

## Hard rules (this crate)

- **Tier & dependencies:** L3 presentation (design system). Allowed workspace deps: `jackin-core` (for re-exported widget stubs and helpers like `shorten_home`). Must NOT depend on infrastructure or higher-layer crates.
- **Keep `README.md` current:** update it when structure, public API, components, tokens, or keymaps change (see `crates/AGENTS.md`).
- **Backend-neutral by design.** Token types (RGB, layout, hints), component state, and render helpers must stay free of a specific backend. The `runtime` traits are the dispatch point console/launch/capsule loop through — keep them operational, not type-only.
- **Cross-cutting TUI behavior is documented.** Focusability, navigation, color, modal sizing, and hints live under `docs/content/docs/reference/tui/`; a behavior change here ships the matching doc update in the same PR.

## What lives here vs elsewhere

- This crate owns: the design-system tokens, reusable component state + `render_*` helpers, the shared TUI `runtime`, keymap, scroll, animation, ansi/url text, ownership guards.
- The reference rendering of every component (the API agents copy) lives in `jackin-tui-lookbook`. Console composition lives in `jackin-console`; launch presentation in `jackin-launch-tui`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
