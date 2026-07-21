# Goal: Execute native macOS agent-usage menu bar program to completion

Execute the **Native macOS agent-usage menu bar** program end-to-end until every plan is DONE (or explicitly BLOCKED on a named operator input).

## Source of truth (read all before any change, this order)

1. `plans/native-macos-usage-menu-bar/README.md` — program order, boundaries, operator decisions, considered-and-rejected
2. `plans/native-macos-usage-menu-bar/005-host-global-usage-cache.md`
3. `plans/native-macos-usage-menu-bar/006-native-tahoe-design-refresh.md`
4. `plans/native-macos-usage-menu-bar/001-universal-static-app.md`
5. `plans/native-macos-usage-menu-bar/002-homebrew-cask-guardrails.md`
6. `plans/native-macos-usage-menu-bar/003-notarized-release-and-cask.md`
7. `plans/native-macos-usage-menu-bar/004-production-proof-and-roadmap-retirement.md`
8. `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx` — living roadmap status
9. `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx` — frozen architecture

If any path is missing on the active branch: STOP and report; do not invent plans.

## Done means

- All six plans status **DONE**, **or** each unfinished plan **BLOCKED** with the exact operator input required
- Roadmap item open-work checklist fully checked (or residual list only, honest)
- Plan 004 complete: roadmap page retired / shrunk per jackin docs rules **after** 003 shipped a real notarized release and the operator approved + merged the first cask

## Execution order (do not reorder)

```text
005 → 006 → 001 → 002 → 003 → 004
```

- After **006**, **001** and **002** may run in parallel.
- Never start **003** until **001** and **002** are DONE **and** every operator decision in the program README is recorded with real values.
- Never start **004** until **003** shipped a real notarized release and the operator approved and merged the first cask.

## Per-plan protocol

1. Run that plan’s **drift check** first. On mismatch with the plan’s “Current state” excerpts: **STOP** that plan, report the exact diff, do not improvise.
2. Follow steps in order. Run every verification command. A step is done only when its **expected output is observed**.
3. Honor every **STOP** literally: report-and-halt for that plan, not a workaround license. Continue with the next unblocked plan if dependencies allow.
4. Respect scope lists strictly: touch no file outside the plan’s **In scope**.
5. **Swift is display-only**: no HTTP/OAuth/probes/percent arithmetic in Swift.
6. **Provider scope frozen**: `Agent::ALL` + Z.AI/GLM + MiniMax only. No Cursor/Gemini/Copilot zoo. No daemon requirement for v1. No new IPC unless a plan explicitly requires it.
7. After finishing a plan:
   - Update status row in `plans/native-macos-usage-menu-bar/README.md`
   - Tick the matching roadmap-item checklist entry
   - Sync `docs/content/docs/roadmap/index.mdx` per docs discipline in `docs/CLAUDE.md` / root `AGENTS.md`

## Hard repo rules (non-negotiable)

- Stay on the active feature branch. If on `main`: propose a branch name and **wait for operator confirm** before creating it.
- Every commit: Conventional Commits, **`git commit -s`**, **push immediately**. No force-push / rebase / amend / squash without explicit operator approval for that branch.
- Brand is always **jackin❯** in prose/docs/UI; bare `jackin` only for identifiers, paths, commands, crates.
- Do not hard-wrap prose markdown. User-facing + contributor docs update in the **same PR** as the behavior they describe.
- Never print, store, or commit secret values; reference credentials by name/type/location only.
- Prefer structural reuse of `jackin-usage` + `jackin-protocol` over parallel stacks. Clean-room vs CodexBar (visual reference only).

## Verification gates (minimum; also run each plan’s own command table)

- `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` (plus `-p jackin-runtime` for plan **005**)
- `cargo clippy -p jackin-usage -p jackin-usage-ffi --all-targets -- -D warnings` (and other packages the plan touches)
- macOS plans: `cd native && swift build -c release && swift test -c release`; app via `./scripts/build-usage-menu-bar-app.sh`
- Docs: `cargo xtask docs repo-links` && `cargo xtask roadmap audit` && `cargo xtask research check` (when the plan touches research/roadmap)
- Before PR-ready: `cargo xtask ci --fast` (full `cargo xtask ci` where a plan requires it)
- Before any “all gates green” claim: `cargo xtask lint --strict` when the tree has new crates/docs/registration surfaces

A step is **not** done without captured verification output.

## Reporting (after each plan)

One short block:

- Plan id
- Status: `DONE` | `BLOCKED` | `STOPPED`
- Commits pushed
- Verification results (commands + observed outcome)
- If BLOCKED/STOPPED: exact missing operator input or failed condition

Never claim a step done without its verification output.

## Final report

End with:

- Status of all six plans
- Any operator decisions still owed
- Residual (e.g. Apple notarization secrets) only if a plan’s STOP/BLOCKED condition is met — never invent bypasses
