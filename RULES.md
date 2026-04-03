# Rules

## Documentation Convention

All project documentation, conventions, commands, and architecture info must go in `AGENTS.md` files — never in tool-specific config files (e.g., `CLAUDE.md`, `GEMINI.md`, `COPILOT.md`).

Tool-specific files should only contain a reference to `AGENTS.md` (e.g., `@AGENTS.md`).

This ensures instructions are shared across all AI agents regardless of which tool is used.
