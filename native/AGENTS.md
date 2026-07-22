# jackin❯ Desktop (native)

Display-only Swift shell over `jackin-usage-ffi` (UniFFI). Product: **jackin❯ Desktop**
(`JackinDesktop.app`). Rust owns probes, cache, severity, and every usage number.

> **CLAUDE.md = symlink to AGENTS.md beside it** — recreate: `ln -s AGENTS.md CLAUDE.md`.

## Hard rules

- **Display-only Swift.** No HTTP/OAuth/CLI scrapes, no second provider matrix, no
  inventing percentages. Numbers and limit strings come from UniFFI / Rust only.
- **Limits only — never token price or historical usage trend.** The status item,
  glance popover, Usage window, and Settings show **subscription / quota limits
  only** (remaining or used %, dual-bucket stacks, resets, plan/status, multi-
  account switcher, provider-supplied **limit** windows). **Never** implement:
  - token unit prices or “cost of this usage” money-as-price surfaces
  - historical usage or spend **trends** (sparklines, bar charts, 30-day series)
  - aggregate-spend donuts, cost legends, ranked spend-by-model UI
  - Buy Credits or other commercial write actions
  OpenUsage/CodexBar may include those — **do not copy them**. See root
  [AGENTS.md](../AGENTS.md) and [jackin-usage/AGENTS.md](../crates/jackin-usage/AGENTS.md).
- **Liquid Glass** only in `GlassFallbacks.swift` for navigation chrome; content
  cards use standard materials (HIG).
- **Frozen host catalog only** — Claude, Codex, Amp, Grok Build, GLM/Z.AI, Kimi,
  MiniMax, OpenCode. No Cursor/Gemini/Copilot zoo.
- Build/verify/run: `mise run desktop-*` / `cargo xtask desktop` only (no shell
  assembly scripts).
