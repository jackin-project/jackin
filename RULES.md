# Rules

## Documentation Convention

All project rules, conventions, commands, and architecture info must live in this repo's topic-specific rule files (linked from [AGENTS.md](AGENTS.md)) — never in tool-specific config files (e.g., `CLAUDE.md`, `GEMINI.md`, `COPILOT.md`).

Tool-specific files should only contain a reference to `AGENTS.md` (e.g., `@AGENTS.md`).

This ensures instructions are shared across all AI agents regardless of which tool is used.

## Deprecations

When you deprecate an API, CLI verb/flag, config field, config value, or usage pattern — even if the old form is still wired up for backwards compatibility — record it in [DEPRECATED.md](DEPRECATED.md) in the **same commit** that introduces the deprecation.

Keeping a single ledger means we can periodically review what's safe to remove instead of rediscovering deprecations through `grep` or operator support tickets. When the deprecated thing is finally removed, delete its entry from `DEPRECATED.md` in the removal commit.

See `DEPRECATED.md` itself for the entry format.

## TUI Keybindings

TUI keybindings must use plain letters, numbers, `Enter`, `Esc`, `Tab`, or arrow keys. Avoid `Ctrl`/`Alt`/`Cmd`/`Shift` modifiers — they add friction, conflict with terminal and multiplexer chords (tmux, iTerm2, Ghostty), and are not discoverable in footer hints.

Where a command would otherwise collide with text input (a key inside a textarea would be typed as text), move the command to a parent context where it does not conflict — typically as a sibling row action rather than a sub-mode of the text editor.
