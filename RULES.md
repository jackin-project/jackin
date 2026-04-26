# Rules

## Documentation Convention

All project rules, conventions, commands, and architecture info must live in this repo's topic-specific rule files (linked from [AGENTS.md](AGENTS.md)) — never in tool-specific config files (e.g., `CLAUDE.md`, `GEMINI.md`, `COPILOT.md`).

Tool-specific files should only contain a reference to `AGENTS.md` (e.g., `@AGENTS.md`).

This ensures instructions are shared across all AI agents regardless of which tool is used.

## Deprecations

When you deprecate an API, CLI verb/flag, config field, config value, or usage pattern — even if the old form is still wired up for backwards compatibility — record it in [DEPRECATED.md](DEPRECATED.md) in the **same commit** that introduces the deprecation.

Keeping a single ledger means we can periodically review what's safe to remove instead of rediscovering deprecations through `grep` or operator support tickets. When the deprecated thing is finally removed, delete its entry from `DEPRECATED.md` in the removal commit.

See `DEPRECATED.md` itself for the entry format.

## TUI Labels

User-facing labels in the TUI (column headers, tab names, button text, footer hints, modal titles, status badges) must use the **full word**, not an abbreviation. Operators read the TUI in passing — they cannot afford to pause and decode what `Iso`, `Cfg`, `Env`, `Auth`, `WD` etc. stand for, and the meaning of an abbreviation is rarely obvious from context.

Examples:

- `Isolation` — not `Iso`
- `Environments` — not `Env`
- `Workdir` — not `WD`
- `Read-only` — not `RO` (the `rw`/`ro` data values inside a row are fine; the *header* labelling that column should be the full word)

When a column would be uncomfortably wide, prefer wrapping or a layout
adjustment over an abbreviated label. The cost of a few extra characters
in the header is much smaller than the cost of an operator
mis-interpreting a screen.

Already-established short forms in this codebase that are NOT considered
abbreviations: `dst` (destination, used in mount paths), `src` (source,
same), `git`, `op` (1Password, an actual product name). Don't extend
this set without raising it as a design question.

## TUI Keybindings

TUI keybindings must use plain letters, numbers, `Enter`, `Esc`, `Tab`, or arrow keys. Avoid `Ctrl`/`Alt`/`Cmd`/`Shift` modifiers — they add friction, conflict with terminal and multiplexer chords (tmux, iTerm2, Ghostty), and are not discoverable in footer hints.

Where a command would otherwise collide with text input (a key inside a textarea would be typed as text), move the command to a parent context where it does not conflict — typically as a sibling row action rather than a sub-mode of the text editor.

### Contextual key absorption

When a focused row in a TUI list semantically owns a key (an arrow, `Enter`, etc.), that row absorbs the keypress — even when the row's sub-state would make the action a no-op. The keypress must NOT fall through to a sibling handler that would do something visually unrelated.

Concrete example: collapsible section headers (`▼` expanded / `▶` collapsed) own `←` and `→`. `←` collapses if expanded; if already collapsed, it's a no-op — but it never falls through to "previous tab". Same for `→`. The operator pressing arrows on a row that visually suggests directional navigation should never cause an unrelated tab change.

When designing a new TUI row type that responds to arrow keys, decide explicitly whether arrows are absorbed or fall through, and add a test for both states (active sub-state AND inactive sub-state). The default is **absorbed**.

## TUI List Modals

List-modal widgets (pickers — agent picker, 1Password picker, source
picker, etc.) follow a single canonical layout for consistency:

- **Title** — short subject of the modal (e.g., `1Password`, `Select Agent`,
  `<email> → <vault>`). Filter buffer is **never** part of the title.
- **Filter row** — first body row, persistent. Format: `Filter: <buf>`
  with placeholder dots (`░`) padding when empty, live characters when
  typing. Even pickers that don't accept filter input render this row
  empty (or omit it explicitly only if filtering is genuinely
  out-of-scope).
- **List body** — bordered area below the filter row. Rows render with
  `▸ ` prefix on the focused row, two-space prefix on unfocused. Empty
  filtered state is just blank space — no `(no items match)` placeholder.
- **Footer** — single line, separator-delimited:
  `↑↓ navigate · type filter · Enter <action> · Esc cancel` plus any
  picker-specific hints (e.g., `r refresh` for the 1Password picker).
  Use plain words for the action (`select`, `launch`, etc.) — see
  `TUI Keybindings` for the modifier-free key rule.
- **Border** — phosphor-dim single-line via `Block::default().borders
  (Borders::ALL)` matching the rest of the TUI chrome.

Reference implementation: `src/console/widgets/op_picker/render.rs`.
New picker widgets should follow this layout. If a picker needs a
visually distinct treatment, raise it as a design question first — the
default is "match the established pattern".
