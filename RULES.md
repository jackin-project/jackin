# Rules

## Documentation Convention

All project rules, conventions, commands, and architecture info must live in this repo's topic-specific rule files (linked from [AGENTS.md](AGENTS.md)) — never in tool-specific config files (e.g., `CLAUDE.md`, `GEMINI.md`, `COPILOT.md`).

Tool-specific files should only contain a reference to `AGENTS.md` (e.g., `@AGENTS.md`).

This ensures instructions are shared across all AI agents regardless of which tool is used.
