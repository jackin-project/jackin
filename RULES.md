# Rules

## Documentation Convention

All project rules, conventions, commands, architecture info live in repo's topic-specific rule files — never in tool-specific config files (e.g., `CLAUDE.md`, `GEMINI.md`, `COPILOT.md`).

**Tool-specific config files are symlinks to sibling AGENTS.md — never a copy, never an `@import`.** Every CLAUDE.md (and future GEMINI.md / COPILOT.md) symlinks to AGENTS.md in same dir. Create with `ln -s AGENTS.md CLAUDE.md`. Find tool config that's plain-text `@AGENTS.md` include or copy? Replace with symlink. Every dir with AGENTS.md needs CLAUDE.md symlink beside it.

Symlink = one source of truth on disk: two paths resolve to same bytes, so tool file never drifts from AGENTS.md. Instructions shared across all AI agents regardless of tool.

**Never link to AGENTS.md or CLAUDE.md from another rule file.** Agent harness auto-loads these from working dir — root AGENTS.md always present, subdir's AGENTS.md loads automatically when agent reads or edits under that subtree. Cross-reference link redundant at best, misleading at worst (implies manual open). No rule file — not AGENTS.md, not repo-root topic file like PULL_REQUESTS.md, BRANCHING.md, ENGINEERING.md — may contain Markdown link or `@import` to any AGENTS.md or CLAUDE.md. Reference rule by topic, or name governing subdir in plain text (e.g. "agent-only PR extras that load under `.github/`"), but don't link file. Links between non-AGENTS topic files (PULL_REQUESTS.md ↔ BRANCHING.md, etc.) fine.

Applies to agent instruction graph — repo's rule files. Published docs site separate, human-facing: contributor pages may still point at house-rules files with Fumadocs repository-file links because docs reader not inside auto-load harness.

## Brand spelling

In rich-text prose, product and project always spelled `jackin❯`: lowercase letters followed immediately by the `❯` chevron. Plaintext-only environments may use `jackin>`. Compact logo or prompt surfaces may use `j❯`, with `j>` reserved for proven plaintext fallbacks. Never write `Jackin`, `Jackin'`, `jackin'`, or bare `jackin` for brand/product/project in normal text. Use the no-chevron spelling only for literal commands, binaries, crates, packages, env vars, config keys, file paths, labels, selectors, URLs, and code identifiers — `jackin`, `jackin-capsule`, `JACKIN_DEBUG`, `~/.jackin/`, `jackin.role.toml`. If the chevron makes possessive or sentence awkward, rewrite the sentence instead of dropping it.

## Deprecations

Deprecate API, CLI verb/flag, config field, config value, or usage pattern — even if old form still wired for backwards compat — record in [DEPRECATED.md](DEPRECATED.md) in **same commit** introducing deprecation.

Single ledger = periodic review of what's safe to remove, instead of rediscovering deprecations via `grep` or support tickets. Deprecated thing finally removed? Delete its entry from `DEPRECATED.md` in removal commit.

See `DEPRECATED.md` for entry format.

## TUI Labels

User-facing TUI labels (column headers, tab names, button text, footer hints, modal titles, status badges) use **full word**, not abbreviation. Operators read TUI in passing — can't pause to decode `Iso`, `Cfg`, `Env`, `Auth`, `WD`; meaning rarely obvious from context.

Examples:

- `Isolation` — not `Iso`
- `Environments` — not `Env`
- `Workdir` — not `WD`
- `Read-only` — not `RO` (`rw`/`ro` data values inside row fine; *header* labelling column = full word)

Column too wide? Prefer wrapping or layout adjustment over abbreviated label. Cost of few extra header chars much smaller than operator mis-reading screen.

Established short forms NOT abbreviations: `dst` (destination, mount paths), `src` (source, same), `git`, `op` (1Password, product name). Don't extend set without raising as design question.

## TUI Keybindings

TUI keybindings use plain letters, numbers, `Enter`, `Esc`, `Tab`, or arrows. Avoid `Ctrl`/`Alt`/`Cmd`/`Shift` modifiers — add friction, conflict with terminal and multiplexer chords (tmux, iTerm2, Ghostty), not discoverable in footer hints.

Command would collide with text input (key inside textarea typed as text)? Move command to parent context where no conflict — typically sibling row action, not sub-mode of text editor.

### Contextual key absorption

Focused row in TUI list semantically owns a key (arrow, `Enter`, etc.)? Row absorbs keypress — even when row's sub-state makes action a no-op. Keypress must NOT fall through to sibling handler doing something visually unrelated.

Concrete example: collapsible section headers (`▼` expanded / `▶` collapsed) own `←` and `→`. `←` collapses if expanded; if already collapsed, no-op — but never falls through to "previous tab". Same for `→`. Operator pressing arrows on row that visually suggests directional nav should never cause unrelated tab change.

Designing new TUI row type responding to arrows? Decide explicitly: arrows absorbed or fall through. Add test for both states (active sub-state AND inactive). Default **absorbed**.

## TUI List Modals

List-modal widgets (pickers — agent picker, 1Password picker, source picker, etc.) follow single canonical layout for consistency:

- **Title** — short subject of modal (e.g., `1Password`, `Select Agent`, `<email> → <vault>`). Filter buffer **never** part of title.
- **Filter row** — first body row, persistent. Format: `Filter: <buf>` with placeholder dots (`░`) padding when empty, live chars when typing. Even pickers not accepting filter input render this row empty (or omit explicitly only if filtering genuinely out-of-scope).
- **List body** — bordered area below filter row. Rows render `▸ ` prefix on focused row, two-space prefix on unfocused. Empty filtered state = blank space — no `(no items match)` placeholder.
- **Footer** — single line, separator-delimited: `↑↓ navigate · type filter · Enter <action> · Esc cancel` plus picker-specific hints (e.g., `r refresh` for 1Password picker). Use plain words for action (`select`, `launch`, etc.) — see `TUI Keybindings` for modifier-free rule.
- **Border** — phosphor-dim single-line via `Block::default().borders(Borders::ALL)` matching rest of TUI chrome.

Reference impl: `src/console/widgets/op_picker/render.rs`. New picker widgets follow this layout. Picker needs visually distinct treatment? Raise as design question first — default "match established pattern".
