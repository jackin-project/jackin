# Coverage Ledger — jackin-desktop

Item: roadmap/jackin-desktop/README.md (working tree on `3e6376d`; item file
uncommitted at ingest), ingested 2026-07-24.
Override: none (item READY 2026-07-24).

## Screens

| ID | Screen / state | Item anchor | Spec | Plans | Status |
|----|----------------|-------------|------|-------|--------|
| S1 | Status bar item — default (icon + glance % left: weekly except Amp Free daily, monochrome) | §Screens/"macOS status bar item" | spec/status-bar.md | 005 | covered |
| S2 | Status bar — stale/error (dimmed last-known; never disappears) | §Screens/status bar States | spec/status-bar.md | 005 | covered |
| S3 | Status bar — never-fetched ("–") | §Screens/status bar States | spec/status-bar.md | 005 | covered |
| S4 | Status item right-click context menu (Open Usage Window, Refresh, Quit) | §Decisions "Usage window entry", §Screens interactions | spec/status-bar.md | 007 | covered |
| S5 | Popover — Overview tab (compact per-provider rows) | §Screens/"Popover" schematic left | spec/popover.md | 006 | covered |
| S6 | Popover — provider tab (chips, header, windows, credits, Refresh footer) | §Screens/"Popover" schematic right + Kept list | spec/popover.md | 006 | covered |
| S7 | Popover — loading (last-known stays; no blank flash) | §Screens/popover States | spec/popover.md | 006 | covered |
| S8 | Popover — stale (freshness dims; last-good renders) | §Screens/popover States | spec/popover.md | 006 | covered |
| S9 | Popover — error (per-provider Rust error line) | §Screens/popover States | spec/popover.md | 006 | covered |
| S10 | Popover — empty ("no agent credentials found") | §Screens/popover States | spec/popover.md | 005, 006 | covered |
| S11 | Usage window — provider card (glass sidebar + content pane) | §Screens/"Usage window" | spec/usage-window.md | 008 | covered |
| S12 | Usage window — Overview + stale/error/empty states | §Screens/"Usage window" States | spec/usage-window.md | 008 | covered |

## Capabilities

| ID | Capability | Item anchor | Spec | Plans | Status |
|----|-----------|-------------|------|-------|--------|
| F1 | Glance availability % per enabled provider in menu bar (selected account; weekly except Amp Free daily) | §Intent, §Decisions D4/D8/D15 | spec/status-bar.md | 005 | covered |
| F2 | Rust core; Swift display-only rendering of Rust strings | §Intent, §Must not N1 | spec/architecture.md | 001–009 | covered |
| F3 | Auto-detect enabled providers (no Settings) | §Decisions D12/D6 | spec/providers.md | 005 | covered |
| F4 | Multi-account per provider; selected account persists and drives glance | §Screens Kept, §Decisions D8 | spec/popover.md | 006 | covered |
| F5 | Run-out producer Variant A in Rust via `pace_label` composite | §Decisions D10 | spec/providers.md | 004 | covered |
| F6 | Claude macOS Keychain read with normalized default/custom isolation; preflight before shared coordination; denial cannot resurrect/share cache; every Desktop bridge access off-main | §Decisions D9 | spec/providers.md | 002 | covered |
| F7 | Grok resolved server tier; heuristic retired; zero/signed-safe current nested headline/prepaid fields; on-demand only behind a positive enabled cap | §Decisions D11, §Data gaps | spec/providers.md | 003 | covered |
| F8 | Manual Refresh (⌘R) forces a Rust fetch; automatic refresh keeps the ≥60s floor | §Screens interactions, §Flows W4 | spec/popover.md | 006 | covered |
| F9 | Distribution: notarized ZIP + Homebrew cask + install proof (headless) | §Capabilities scope | spec/distribution.md | 010, 011 | covered |
| F10 | Design/UX refresh: CodexBar-style popover/status bar under Capsule design supremacy | §Capabilities scope, §Decisions D7/D16 | spec/popover.md, spec/architecture.md | 006, 009 | covered |
| F11 | Provider-core correctness fixes (Codex account decode tags; MiniMax documented host; z.ai `level` plan field) | §Research link (coverage gaps) | spec/providers.md | 001 | covered |
| F12 | Current Amp Free daily `displayText`, Daily slot, individual/workspace balances | §Decisions D15; research ch. 11 | spec/providers.md | 001 | covered |
| F13 | Usage window renders full Capsule-parity provider card | §Decisions D5 | spec/usage-window.md | 008 | covered |
| F14 | Amp paid-plan/monthly `displayText` (Megawatt/Gigawatt/linked subscriptions) | §Decisions D15; research ch. 11 open unknowns | — | — | deferred (authenticated paid-account capture required) |

## Flows

| ID | Flow | Screens touched | Spec | Plans | Status |
|----|------|-----------------|------|-------|--------|
| W1 | Glance (left-click → provider tab → dismiss) | S1, S6 | spec/popover.md | 007 (binds 006 seam) | covered |
| W2 | Detail (right-click menu / header click → Usage window) | S4, S6, S11 | spec/usage-window.md | 007, 008 | covered |
| W3 | Account switch (chip → selection persists → bar/Overview follow) | S6, S5, S1 | spec/popover.md | 006 | covered |
| W4 | Refresh (⌘R → forced Rust fetch → per-provider failure isolation; automatic cadence remains floored) | S6, S4 | spec/popover.md | 006 | covered |
| W5 | First launch / no credentials → auto-detect pickup w/o restart | S1, S10 | spec/providers.md | 002, 005 | covered |

## Must-not anchors

| ID | Statement | Reason | Registry |
|----|-----------|--------|----------|
| N1 | No logic beyond display in Swift | Rust owns implementation (item Intent) | spec/README.md |
| N2 | No action buttons in popover (Refresh only; header click = navigation) | glance surface (D2/D3) | spec/README.md |
| N3 | Limits only — never token prices, spend/trend/history surfaces | repo hard rule | spec/README.md |

## Quality bar

| ID | Statement anchor | Spec scenario(s) | Status |
|----|------------------|------------------|--------|
| B1 | Swift renders Rust strings verbatim (arch test gate) | spec/architecture.md scenarios | covered |
| B2 | Liquid Glass chrome-only + macOS 14/15/Reduce-Transparency fallbacks | spec/architecture.md "Supported macOS uses glass only for chrome"; "Older macOS and accessibility fallback" | covered |
| B3 | Usage-window number == Capsule dialog number (parity) | spec/usage-window.md | covered |
| B4 | Limits-only audit (no price/trend string anywhere) | spec/architecture.md "Forbidden reference content is absent"; "Provider quota bounds remain allowed" | covered |
| B5 | Error never overwrites last-good; stale dimmed with age | spec/popover.md, spec/status-bar.md | covered |
| B6 | CI green; artifact notarized, stapled, Gatekeeper-accepted | spec/distribution.md | covered |

## Decisions (constraints)

| ID | Decision | Dated | Constrains |
|----|----------|-------|-----------|
| D1 | v1 code = reference baseline; refactor + extend | 2026-07-24 | all slicing (no rewrite-from-scratch) |
| D2 | Popover availability-only, no action buttons | 2026-07-24 | spec/popover.md, N2 |
| D3 | Popover footer = Refresh only | 2026-07-24 | spec/popover.md |
| D4 | Status bar = all enabled providers, one glance %: weekly for six, Amp Free daily for Amp | 2026-07-24 | spec/status-bar.md |
| D5 | Usage window keeps Capsule parity | 2026-07-24 | spec/usage-window.md |
| D6 | No Settings surface for now | 2026-07-24 | scope |
| D7 | Capsule design supremacy; mismatches escalate to operator | 2026-07-24 | all UI plans (STOP condition) |
| D8 | Selected account drives status bar % + Overview row | 2026-07-24 | spec/popover.md, spec/status-bar.md |
| D9 | Claude Keychain service/account/cache follows normalized effective config dir; consent precedes shared coordination; denial is terminal/local-only; headless fallback remains; all Desktop bridge access serializes off-main | 2026-07-24 | plan 002 |
| D10 | Run-out = Variant A via pace_label composite | 2026-07-24 | plan 004 |
| D11 | Grok plan label from server fields | 2026-07-24 | plan 003 |
| D12 | Enabled providers auto-detected | 2026-07-24 | plan 005 |
| D13 | Window entry: right-click menu + popover header click | 2026-07-24 | plan 007 |
| D14 | Plans home = plans/jackin-desktop; fold 003/004; reconcile 013; retire old program | 2026-07-24 | manifest, plan 011 |
| D15 | Amp Free daily reparse now; paid-plan/monthly parsing after capture | 2026-07-24 | F12, F14 |
| D16 | CodexBar display implementation stays clean-room; operator-requested source check may corroborate a provider wire contract without copied code | 2026-07-24 | all UI plans; research ch. 11 |

## External references & integrations

| ID | Reference | Kind | Research topics |
|----|-----------|------|-----------------|
| R1 | `crates/jackin-usage/` (probes, HostUsageRuntime, snapshot store) | in-repo crate | agent-usage-provider-apis ch.01 |
| R2 | `crates/jackin-usage-ffi/` (UniFFI DTO facade) | in-repo crate | agent-usage-provider-apis ch.01 |
| R3 | `native/` Swift package (v1 baseline: StatusItemLabel, PopoverRoot, UsageWindow, GlassFallbacks) | in-repo | — (repo read during plan writing) |
| R4 | 7 provider usage APIs (endpoints, auth, fields) | external APIs | agent-usage-provider-apis ch.02–06, 08–10 |
| R5 | CodexBar / OpenUsage | clean-room display refs | agent-usage-provider-apis ch.10 (allowed materials only) |
| R6 | `plans/native-macos-usage-menu-bar/` + docs roadmap page + ADR-011 | prior program | — (reconciled by plan 011) |
| R7 | `jackin usage` CLI + capsule usage dialog (parity source) | in-repo | — |
| R8 | Build/verify/test commands for native + crates | verification tooling | jackin-desktop-verification-tooling |

## Assumptions

| ID | Assumption | Why safe | Falsified by | Status |
|----|------------|----------|---------------|--------|
| A1 | Provider windows are fixed-slot; `resets_at` anchors Variant A (`window_start = resets_at − window_seconds`) | research ch.09 Q3 / ch.10 Q4 (Claude weekly HIGH, Grok HIGH, Codex client-observable convention) | live window observed rolling / usage non-zero at start breaking projections materially | holds |
| A2 | One credential parser covers Claude Keychain JSON and file JSON (same `claudeAiOauth` shape) | research ch.09 Q1 (issue #9403 payload = file shape) | Keychain payload diverging from file schema | holds |
| A3 | z.ai Bearer header keeps working for the operator's key | jackin❯ ships Bearer today; 12 public Bearer-only clients, no failure reports (ch.10 Q2) | 401 on live probe → switch to raw-key + fallback | holds |
| A4 | Existing v1 gates (arch test, glass fallbacks, desktop verify) remain the enforcement points for B1/B2 | native/README.md + CI job "Native usage menu bar" | gate removed/renamed in CI | holds |
| A5 | Current Amp Free `displayText` is percentage-based Daily and has no exact reset timestamp | research ch. 11 live transcript + merged regression fixture | current authenticated output lacks the daily line or supplies structured reset metadata | holds |

## Research questions

| ID | Question | Research topic | Status |
|----|----------|----------------|--------|
| Q1 | Amp `displayText` under Megawatt/Gigawatt/linked-subscription accounts | agent-usage-provider-apis ch. 11 (queued operator probe) | open — blocks F14 only; Amp Free daily + workspace balance lines are resolved |
| Q2 | Operator-gated probe list (Spark live `limit_name`, z.ai header two-form probe, Kimi `/usages` schema/UA, Grok web protobuf, MiniMax plan-title fields, Claude routines key) | agent-usage-provider-apis §Open unknowns | open — none blocks a covered plan; each listed plan carries a STOP/fallback |

## Planning conflicts

| ID | Affected coverage | Conflict | Resolution required | Status |
|----|-------------------|----------|---------------------|--------|
| PC1 | S1–S3, S10 bar half, F1, F3; plans 001/005 and dependants | Original D4 required weekly for Amp despite no weekly source | Resolved by operator: Amp uses its proven Amp Free Daily percentage; paid-only responses without that line show unavailable dash while retaining detail balances | resolved 2026-07-24 |
