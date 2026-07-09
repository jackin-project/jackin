# AGENTS.md — jackin-core

Universal vocabulary types shared across every jackin❯ crate — the leaf at the bottom of the workspace graph.

## Rules (this crate)

- Types, traits, constants, and pure helpers only. No I/O, no subprocess, no runtime behavior — anything that does work belongs in a higher crate.
- Keep it compile-cheap: every crate depends on this one, so do not add dependencies or heavy generics here.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
