# Rules

## Documentation Convention

All project rules, conventions, commands, architecture info must live in this repo's topic-specific rule files — never in tool-specific config files (e.g., `CLAUDE.md`, `GEMINI.md`, `COPILOT.md`).

**Tool-specific config files are symlinks to sibling `AGENTS.md` — never a copy, never an `@import`.** Every `CLAUDE.md` (and any future `GEMINI.md` / `COPILOT.md`) is a symbolic link pointing at `AGENTS.md` in same directory. Create one with `ln -s AGENTS.md CLAUDE.md`. If you find a tool config file that is a plain-text `@AGENTS.md` include or a copy, replace it with symlink. Every directory with an `AGENTS.md` must have a `CLAUDE.md` symlink beside it.

Symlink means exactly one source of truth on disk: two paths resolve to same bytes, so tool file can never drift from `AGENTS.md`. Ensures instructions shared across all AI agents regardless of tool used.

**Never link to an `AGENTS.md` or `CLAUDE.md` file from another rule file.** These files auto-loaded by agent harness from directory being worked in — root `AGENTS.md` always present, subdirectory's `AGENTS.md` loads automatically whenever agent reads or edits under that subtree. A cross-reference link to one is redundant at best, misleading at worst (implies file must be opened manually). No rule file — not `AGENTS.md` itself, not a repo-root topic file like `PULL_REQUESTS.md`, `BRANCHING.md`, or `ENGINEERING.md` — may contain a Markdown link or `@import` pointing at any `AGENTS.md` or `CLAUDE.md`. Reference rule by topic, or name governing subdirectory in plain text (e.g. "agent-only PR extras that load under `.github/`"), but don't link the file. Links between non-`AGENTS` topic files (`PULL_REQUESTS.md` ↔ `BRANCHING.md`, etc.) fine.

Applies to agent instruction graph — repo's rule files. Published docs site is a separate, human-facing surface: contributor pages may still point at house-rules file with `<RepoFile path="AGENTS.md" />` because a docs reader is not inside auto-load harness.

## Brand spelling

In prose, product and project always spelled `jackin'`: lowercase with trailing apostrophe. Do not write `Jackin`, `Jackin'`, or bare `jackin` when referring to brand, product, or project in normal text. Use no-apostrophe spelling only for literal commands, binaries, crates, packages, environment variables, config keys, file paths, labels, selectors, URLs, code identifiers, such as `jackin`, `jackin-capsule`, `JACKIN_DEBUG`, `~/.jackin/`, and `jackin.role.toml`. If apostrophe makes a possessive or sentence awkward, rewrite the sentence instead of dropping it.

## Deprecations

When you deprecate an API, CLI verb/flag, config field, config value, or usage pattern — even if old form still wired up for backwards compatibility — record it in [DEPRECATED.md](DEPRECATED.md) in **same commit** that introduces the deprecation.

Single ledger means we periodically review what's safe to remove instead of rediscovering deprecations through `grep` or operator support tickets. When deprecated thing finally removed, delete its entry from `DEPRECATED.md` in removal commit.

See `DEPRECATED.md` itself for entry format.

## TUI Labels

User-facing labels in TUI (column headers, tab names, button text, footer hints, modal titles, status badges) must use **full word**, not abbreviation. Operators read TUI in passing — cannot afford to pause and decode what `Iso`, `Cfg`, `Env`, `Auth`, `WD` etc. stand for, and meaning of an abbreviation rarely obvious from context.

Examples:

- `Isolation` — not `Iso`
- `Environments` — not `Env`
- `Workdir` — not `WD`
- `Read-only` — not `RO` (`rw`/`ro` data values inside a row fine; the *header* labelling that column should be full word)

When a column would be uncomfortably wide, prefer wrapping or a layout adjustment over an abbreviated label. Cost of a few extra header characters much smaller than cost of an operator mis-interpreting a screen.

Already-established short forms NOT considered abbreviations: `dst` (destination, used in mount paths), `src` (source, same), `git`, `op` (1Password, an actual product name). Don't extend this set without raising it as a design question.

## TUI Keybindings

TUI keybindings must use plain letters, numbers, `Enter`, `Esc`, `Tab`, or arrow keys. Avoid `Ctrl`/`Alt`/`Cmd`/`Shift` modifiers — they add friction, conflict with terminal and multiplexer chords (tmux, iTerm2, Ghostty), and aren't discoverable in footer hints.

Where a command would otherwise collide with text input (a key inside a textarea typed as text), move command to a parent context where it does not conflict — typically a sibling row action rather than a sub-mode of the text editor.

### Contextual key absorption

When a focused row in a TUI list semantically owns a key (an arrow, `Enter`, etc.), that row absorbs the keypress — even when row's sub-state would make the action a no-op. Keypress must NOT fall through to a sibling handler that would do something visually unrelated.

Concrete example: collapsible section headers (`▼` expanded / `▶` collapsed) own `←` and `→`. `←` collapses if expanded; if already collapsed, no-op — but never falls through to "previous tab". Same for `→`. Operator pressing arrows on a row that visually suggests directional navigation should never cause an unrelated tab change.

When designing a new TUI row type responding to arrow keys, decide explicitly whether arrows absorbed or fall through, and add a test for both states (active sub-state AND inactive sub-state). Default is **absorbed**.

## TUI List Modals

List-modal widgets (pickers — agent picker, 1Password picker, source picker, etc.) follow a single canonical layout for consistency:

- **Title** — short subject of modal (e.g., `1Password`, `Select Agent`, `<email> → <vault>`). Filter buffer **never** part of title.
- **Filter row** — first body row, persistent. Format: `Filter: <buf>` with placeholder dots (`░`) padding when empty, live characters when typing. Even pickers that don't accept filter input render this row empty (or omit it explicitly only if filtering is genuinely out-of-scope).
- **List body** — bordered area below filter row. Rows render with `▸ ` prefix on focused row, two-space prefix on unfocused. Empty filtered state is blank space — no `(no items match)` placeholder.
- **Footer** — single line, separator-delimited: `↑↓ navigate · type filter · Enter <action> · Esc cancel` plus any picker-specific hints (e.g., `r refresh` for 1Password picker). Use plain words for action (`select`, `launch`, etc.) — see `TUI Keybindings` for modifier-free key rule.
- **Border** — phosphor-dim single-line via `Block::default().borders(Borders::ALL)` matching rest of TUI chrome.

Reference implementation: `src/console/widgets/op_picker/render.rs`. New picker widgets should follow this layout. If a picker needs a visually distinct treatment, raise it as a design question first — default is "match the established pattern".
