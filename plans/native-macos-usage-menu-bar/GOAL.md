# Goal: Execute the jackin❯ Desktop program (née native macOS usage menu bar) to completion

Execute the program end-to-end until all twelve plans are **DONE** (or explicitly **BLOCKED**/**STOPPED** on a named operator input), the roadmap item's checklist is honest, and the only open residual is one backed by a named STOP/BLOCKED condition. You are an executor with zero prior context: everything you need is in the files below. Do not improvise beyond them.

## Program state at planning time (2026-07-22, commit `be6fb79e`)

Do **not** re-execute DONE plans; their status rows in the program README carry re-verification evidence. Re-open one only if its drift check fails while you are touching its surfaces.

| Plans | State |
|---|---|
| 005, 006, 001, 002 | **DONE** (cache unification, Tahoe design, arm64 static PR gate, tap cask validation) |
| 003 | Engineering **DONE**, `mode=validate` green; **activation BLOCKED** on Apple secrets in GitHub environment `release-macos` (ops, operator input) |
| 004 | **BLOCKED** on 003 shipping a real notarized release + operator-merged first cask |
| 007–012 | **TODO** — the active **jackin❯ Desktop v1** track (identity rename, Rust view/FFI extensions, status-item modes, Usage window, glance popover, parity acceptance) |

## Source of truth (read all before any change, this order)

1. `plans/native-macos-usage-menu-bar/README.md` — program order, boundaries, operator decisions (including the 2026-07-22 jackin❯ Desktop identity supersession), considered-and-rejected list
2. `plans/native-macos-usage-menu-bar/007-jackin-desktop-identity-rename.md`
3. `plans/native-macos-usage-menu-bar/008-rust-view-ffi-extensions.md`
4. `plans/native-macos-usage-menu-bar/009-status-item-modes-and-settings.md`
5. `plans/native-macos-usage-menu-bar/010-usage-window.md`
6. `plans/native-macos-usage-menu-bar/011-glance-popover-and-design-contract.md`
7. `plans/native-macos-usage-menu-bar/012-v1-parity-acceptance.md`
8. `plans/native-macos-usage-menu-bar/003-notarized-release-and-cask.md` and `004-production-proof-and-roadmap-retirement.md` — the blocked activation residual you may only unblock on named operator input
9. `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx` — living roadmap status + the v1 product spec (Capsule reference screens, design contract, screen inventory S1–S6) that is the acceptance contract
10. `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx` — frozen architecture (Rust truth, Swift shell)

Context only, execute nothing from them: `001-universal-static-app.md`, `002-homebrew-cask-guardrails.md`, `005-host-global-usage-cache.md`, `006-native-tahoe-design-refresh.md` — read when a v1 plan touches their surfaces. Plans 001–004 predate the rename and carry the old identity strings; read them through the supersession note in the program README.

If any numbered-plan path above is missing on the active branch: STOP and report; do not invent plans.

## Done means

- All twelve plans status **DONE**, **or** each unfinished plan **BLOCKED**/**STOPPED** with the exact named condition from its STOP list
- Roadmap v1 checklist fully ticked except items whose plan is BLOCKED (activation residual) — honest, no premature ticks
- Roadmap overview (`docs/content/docs/roadmap/index.mdx`) consistent with the item's `**Status**` line per `docs/CLAUDE.md` discipline
- Plan 004 complete — roadmap page retired/shrunk per jackin❯ docs rules — **only after** 003 shipped a real notarized release and the operator approved + merged the first cask

## Execution order (do not reorder)

```text
005 → 006 → 001 → 002   (DONE — skip)
[007 ∥ 008] → 009 → 010 → 011 → 012 → 003 activation → 004
```

- **007** (identity rename + logomark) and **008** (Rust view/FFI extensions) are independent; default: 007 first, then 008, one PR each (one PR per session = one branch; hard repo rule). Parallel branches only if the operator asked for parallel work.
- **009** requires 007 and 008 DONE. **010** requires 007 and 008 DONE (009 preferred first — shared `PresentationStore` edits).
- **011** must not start before **010** is DONE: the popover keeps its detail cards until the Usage window can show that detail (no regressive intermediate state).
- **012** requires 007–011 all DONE. It is acceptance + docs only; a defect found there routes back to the owning plan (012's executor instructions govern).
- **003 activation**: never trigger `release.yml` with `mode=publish`. Publish is ops, gated on Apple secrets in `release-macos` **and** 007 DONE (no published artifact may carry the old identity) **and** every operator decision in the program README recorded with real values. `mode=validate` dispatches are allowed where a plan's verify step names them.
- Never start **004** until 003 shipped a real notarized release and the operator approved and merged the first cask.

## Per-plan protocol

1. Run that plan's **drift check** first. On mismatch with the plan's "Current state" excerpts: **STOP** that plan, report the exact diff, do not improvise.
2. Follow steps in order. Run every verification command. A step is done only when its **expected output is observed** and captured.
3. Honor every **STOP condition** literally: report-and-halt for that plan, not a workaround license. Continue with the next plan only if its dependencies are DONE.
4. Respect scope lists strictly: touch no file outside the plan's **In scope**. "Out of scope" lists name lookalike traps deliberately — read them.
5. **Swift is display-only** (ADR-011): no HTTP/OAuth/probes, no percentage arithmetic, no label composition, no provider mapping in Swift. Every displayed string crosses UniFFI finished. A missing string is a Rust-first change following plan 008's step pattern (Rust fn + FFI export + golden + regenerated bindings) — never a Swift workaround.
6. **Provider scope frozen**: `HostSurfaceId::ALL` (Claude, Codex, Amp, Grok Build, GLM / Z.AI, Kimi, MiniMax, OpenCode). No additions, no daemon dependency, no new IPC, no alerts/notifications/write actions — v1 is view-only.
7. **Capsule parity is the bug oracle**: if jackin❯ Desktop and the Capsule usage dialog disagree on any number or string, that is a bug by definition — fix on the Rust side or STOP; never "improve" a string in Swift.
8. Never hand-edit generated bindings (`native/Sources/JackinUsageBridge/jackin_usage_ffi.swift`, `native/Generated/`); regenerate via `cargo build -p jackin-usage-ffi --release && ./scripts/generate-usage-swift-bindings.sh` and require a deterministic diff.
9. After finishing a plan:
   - Update its status row in `plans/native-macos-usage-menu-bar/README.md`
   - Tick the matching roadmap-item checklist entries (same PR as the behavior — docs gate)
   - Sync `docs/content/docs/roadmap/index.mdx` when the item's `**Status**` changes
   - Update the `plans/README.md` top-level row when the track's aggregate state changes

## Hard repo rules (non-negotiable)

- Stay on the active feature branch. If on `main`: propose the branch name the plan suggests and **wait for operator confirm** before creating it.
- Every commit: Conventional Commits, **`git commit -s`**, **push immediately**. No force-push / rebase / amend / squash without explicit operator approval for that branch.
- Brand is always **jackin❯** in prose/docs/UI (jackin❯ Desktop); plain `jackin` / `JackinDesktop` / `jackin-desktop` only for identifiers, bundles, casks, paths. `CFBundleName` is `Jackin Desktop` (approved plaintext surface).
- Do not hard-wrap prose markdown. User-facing and contributor docs update in the **same PR** as the behavior they describe. No open-PR links in published docs.
- Never print, store, or commit secret values; credentials by name/type/location only.
- Rust: no `mod.rs`, tests in sibling `tests.rs` (see `crates/AGENTS.md`); comments state non-obvious WHY only. Prefer structural reuse of `jackin-usage` + `jackin-protocol` over parallel stacks. Clean-room vs CodexBar/OpenUsage (concept references only — never code, never provider lists).

## Verification gates (minimum; also run each plan's own command table)

- `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` (plus `-p jackin-capsule -p jackin-protocol` and any other crate the plan touches)
- `cargo clippy -p <touched crates> --all-targets -- -D warnings`
- macOS: `cd native && swift build -c release && swift test -c release`; app fixture via `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh` + the matching verify script
- Glass gate: `rg -n "#available\(macOS 26" native/Sources | grep -v GlassFallbacks.swift` → no matches
- Docs: `cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask research check`; `cd docs && bun run build` when MDX changed
- Workflow changes (plan 007, 003): `actionlint` + a green `gh workflow run release.yml --ref <branch> -f mode=validate` dispatch before merge
- Before PR-ready: `cargo xtask ci --fast` (full `cargo xtask ci` where a plan requires it)

A step is **not** done without captured verification output. Manual-matrix items (light/dark, macOS 26 glass vs 14/Reduce Transparency, VoiceOver, keyboard parity) are recorded in the PR body as each plan specifies. Run macOS commands on macOS with full Xcode; do not weaken them to pass elsewhere.

## Reporting (after each plan)

One short block:

- Plan id
- Status: `DONE` | `BLOCKED` | `STOPPED`
- Branch + commits pushed
- Verification results (commands + observed outcome)
- If BLOCKED/STOPPED: the exact STOP condition hit or operator input required

## Final report

End with:

- Status of all twelve plans
- The plan-012 parity table and design-contract checklist (or where they are recorded)
- Any operator decisions still owed
- Residual list — only items backed by a named STOP/BLOCKED condition (e.g. Apple notarization secrets); never invent bypasses
