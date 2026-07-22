# Plan 012: jackin❯ Desktop v1 parity acceptance and roadmap closure

> **Executor instructions**: This is an acceptance-and-documentation plan — expect zero product-code changes. If acceptance uncovers a defect, do not fix it inline: file it against the owning plan (007–011), set that plan's row back to IN PROGRESS with a one-line reason, and stop this plan at Step 4. When done, update this plan's row in `plans/native-macos-usage-menu-bar/README.md`.
>
> **Drift check (run first)**: confirm plans 007–011 rows are DONE in `plans/native-macos-usage-menu-bar/README.md`; any not-DONE row → STOP (prerequisite).

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: Plans 007–011 all DONE
- **Category**: docs / direction
- **Planned at**: commit `be6fb79e`, 2026-07-22

## Why this matters

The roadmap defines done for v1 as two invariants holding simultaneously: **Capsule parity** (Desktop shows the same DTO strings the Capsule usage dialog shows — "If jackin❯ Desktop and the Capsule ever disagree on a number, that is a bug by definition") and the **v1 design contract** ("A jackin❯ Desktop screen is done when it reads like the native references while matching the Capsule strings exactly"). Plans 007–011 each verified their slice; this plan runs the cross-cutting acceptance once, on one build, and then makes the docs tell the truth: roadmap checklist closed, overview synced, operator guide describing the shipped v1 — per the repo's roadmap-freshness and docs-as-source-of-truth PR gates.

## Current state

- Acceptance sources (all in `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`): the Capsule reference screens (provider cards, Overview tab, distinctive rows), the "Conventions jackin❯ Desktop mirrors" list, the "Native design reference" contract, and the S1–S6 screen inventory.
- Checklist state expected at entry (same file, "Implementation checklist"): all v1 items ticked by their owning plans except any final sweep; the **Activation** item (plan 004, Apple secrets) remains legitimately unchecked — it is ops residual, not v1 UI work.
- Docs pages in play: operator guide `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx` (updated incrementally by 007–011), roadmap page + `docs/content/docs/roadmap/index.mdx` overview, ADR-011 (architecture unchanged by v1 — verify wording still true), program `plans/native-macos-usage-menu-bar/README.md` + `GOAL.md`.
- Roadmap-overview discipline (docs/CLAUDE.md): the roadmap page's `**Status**` line and the overview section placement must agree after this plan.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Build the app | `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh` | exit 0 |
| Verify bundle | same env, `./scripts/verify-usage-menu-bar-app.sh native/dist/JackinDesktop.app` | exit 0 |
| Rust + Swift gates | `cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-capsule --locked && (cd native && swift test -c release)` | all pass |
| Capsule comparison source | `jackin usage` (host CLI) and the Capsule usage dialog in a live session | reference strings |
| Docs audits | `cd docs && bun run build && cd .. && cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask research check` | exit 0 |
| Merge readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope:** `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`, `docs/content/docs/roadmap/index.mdx`, `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx` (final-pass wording only), `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx` (only if acceptance shows a stated fact drifted), `plans/native-macos-usage-menu-bar/README.md`, `plans/native-macos-usage-menu-bar/GOAL.md`.

**Out of scope:** all product code (`native/`, `crates/`, `scripts/`, `.github/`) — defects route back to their owning plan; roadmap-page deletion/retirement (Plan 004 owns final retirement after the notarized release + first cask merge); any new feature requests surfaced during acceptance (record under "Deferred" on the roadmap page instead).

## Git workflow

- Active feature branch; from `main` propose `docs/desktop-v1-acceptance` and wait for confirmation.
- Signed Conventional Commits, push after every commit. Suggested: `docs(roadmap): jackin❯ Desktop v1 acceptance + status closure`.

## Steps

### Step 1: Parity audit — Desktop vs Capsule, one build

On one assembled build with live credentials, side-by-side against the Capsule usage dialog (or `jackin usage` output where the dialog is impractical), walk every enabled provider and record a table (PR body): provider · field group (identity / bucket labels / percent / reset / pace / money / auth / status words / overview row) · Desktop string · Capsule string · match?. Cover minimum: one OAuth surface (Claude or Codex), one key-only surface (Z.AI or Kimi), one money window, one degraded state (unplug network → stale), the Overview strip vs Capsule Overview tab. Every row must byte-match (modulo window-relative "Updated Xm ago" timing skew — note, not a failure).

**Verify**: table complete; zero mismatches (a mismatch is a defect → per executor instructions, route to the owning plan and stop at Step 4).

### Step 2: Screen-inventory + design-contract walkthrough

Check each S1–S6 screen against its roadmap sketch (all status-item variants incl. depleted + privacy collapse; popover strip + footer; window sidebar/card; S4 states; S5 empty; S6 settings) and the design-contract bullets (floating panel, inset cards, section headers, metric-row anatomy, honest empty rows, pinned footer, typography, sidebar severity accents, menu-row footer, estimate caption). Record a checklist with pass / pass-with-noted-limitation / fail. Known acceptable limitations recorded by earlier plans (e.g. `MenuBarExtra` chrome constraints from Plan 011 Step 3) count as pass-with-note.

**Verify**: checklist complete; zero fails.

### Step 3: Close the roadmap truthfully

On the roadmap page: tick the remaining v1 checklist items this sweep proves (native design pass; S1–S6); update the top `**Status**` line to state v1 UI complete with the activation residual (Apple secrets → plan 004) as the only open item; move the "Open work" section-4 entries to Shipped phrasing (keep the page — retirement is Plan 004's). Sync `docs/content/docs/roadmap/index.mdx` per the status→section table in `docs/CLAUDE.md`. Final-pass the operator guide for coherence (007–011 edited it piecewise — one read-through, fix seams only). Confirm ADR-011 statements still hold (architecture frozen; no v1 change should have touched it — if one did, that is a finding, not an edit to make silently).

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` → exit 0; `cd docs && bun run build` → exit 0; roadmap file vs overview placement agree.

### Step 4: Close the program records

Update `plans/native-macos-usage-menu-bar/README.md`: status table rows 007–012, and note the program residual = plans 003 activation + 004 (ops, Apple secrets). Update `GOAL.md`'s source-of-truth list and execution order to include 007–012 (it currently names six plans). If acceptance stopped early on a defect, record exactly which plan reopened and why.

**Verify**: `cargo xtask ci --fast` → exit 0; README/GOAL consistent with reality.

## Test plan

This plan *is* the test: Step 1 parity table + Step 2 contract checklist, both recorded in the PR body. No new automated tests (the string goldens live in Rust from Plan 008; the architecture guards in Swift from 009–011).

## Done criteria

- [ ] Parity table recorded, zero mismatches (or defect routed + this plan stopped honestly).
- [ ] S1–S6 + design-contract checklist recorded, zero fails.
- [ ] Roadmap checklist/status/overview updated and consistent; docs audits pass.
- [ ] Program README + GOAL updated for the twelve-plan program.
- [ ] `cargo xtask ci --fast` exit 0.

## STOP conditions

- Any 007–011 row not DONE.
- A parity mismatch or contract fail (route to owning plan; stop after Step 4's record-keeping).
- Acceptance requires credentials/providers unavailable on the test machine for **all** of a required category (e.g. no money window anywhere) — record the coverage gap explicitly rather than skipping silently; the operator decides whether the gap blocks.

## Maintenance notes

- Plan 004 (production proof + roadmap retirement) remains the program's final gate after Apple secrets exist; it should reuse Step 1's parity table as its runtime-proof baseline.
- Future Desktop surfaces (daemon live-focus, attention prompts — roadmap "Beyond the status bar") start as new roadmap items + new plan files; this program's numbering ends at 012.
