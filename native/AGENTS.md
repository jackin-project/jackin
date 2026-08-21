# jackin❯ desktop (native)

Display-only Swift shell over `jackin-usage-ffi` (boltffi). Product: **jackin❯ desktop**
(`JackinDesktop.app`). Rust owns probes, cache, severity, and every usage number.

> **CLAUDE.md = symlink to AGENTS.md beside it** — recreate: `ln -s AGENTS.md CLAUDE.md`.

## Platform lanes

- **Minimum deployment target: macOS 26.0.** No compatibility branch, no
  pre-26 availability lane.
- **Shipping lane: Xcode 26.6, macOS 26.5 SDK, Swift 6 mode with complete
  strict concurrency and warnings as errors.** This is the only lane that
  produces release artifacts.
- **Forward-validation lane: Xcode 27 beta / macOS 27 SDK — nonblocking and
  scheduled, never the shipping lane.** The runner lane is not yet available;
  the dated exception is recorded in `README.md` (owner: Release Engineering).
- **Post-26.0 API discipline:** guard every post-26.0 symbol with
  `if #available(macOS 27, *)`, ship a decided native fallback beside it, and
  name the minimum-target bump that removes the guard.
  `UIDesignRequiresCompatibility` is never a strategy — an architecture test
  rejects it.

## Hard rules

- **Display-only Swift.** No HTTP/OAuth/CLI scrapes, no second provider matrix, no
  config/workspace discovery, credential/path resolution, account deduplication, or
  inventing percentages. Numbers, identities, provenance, diagnostics, and limit
  strings come from boltffi / Rust only. Production passes no host paths to the bridge.
- **Broker client only.** Refresh sends intent once and renders Rust generation/phase.
  Swift never schedules provider work, owns a retry deadline, or treats task
  cancellation as single-flight authority. Active manual/background requests join;
  broker failure preserves last-good data and fails closed.
- **Limits only — never token price or historical usage trend.** The status item,
  glance popover, Usage window, and Settings show **subscription / quota limits
  only** (remaining or used %, dual-bucket stacks, resets, plan/status, multi-
  account switcher, provider-supplied **limit** windows). **Never** implement:
  - token unit prices or “cost of this usage” money-as-price surfaces
  - historical usage or spend **trends** (sparklines, bar charts, 30-day series)
  - aggregate-spend donuts, cost legends, ranked spend-by-model UI
  - Buy Credits or other commercial write actions
  OpenUsage/CodexBar may include those — **do not copy them**. See root
  product limits-only rules and the `jackin-usage` crate agent rules.
- **System-owned Liquid Glass only.** `NSPopover`, `NavigationSplitView`, toolbar, sidebar, controls, and menus own material. No explicit glass, custom material, custom blur, content glass, or fallback visual lane.
- **Frozen desktop provider contract only** — Codex, Claude, Amp, Grok Build, GLM/Z.AI, Kimi, and MiniMax in Rust order. OpenCode belongs to the wider host universe but is intentionally excluded from jackin❯ desktop.
- Build/verify/run: `mise run desktop-*` / `cargo xtask desktop` only (no shell
  assembly scripts).
- **Test display parity:** after Desktop UI changes run `mise run desktop-test`
  (or `cargo xtask desktop test`). That drives host nextest + pure Swift harnesses
  (`StatusItemChipHarness`, `DesktopArchitectureLint`, `DesktopParityMatrixHarness`)
  proving multi-provider remaining % strips, dual-bucket, depleted countdown, and
  displayability of Rust-supplied Desktop catalog fixtures without inventing token
  prices or trends.
  Full Xcode CI runs `cargo xtask desktop test-swift` — counted proof (nonzero
  XCTest + Swift Testing totals, zero failures); never bare `swift test`.
