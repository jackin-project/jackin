# Plan 006: Make agent↔detector coverage exhaustive — add grok, and stop silent-empty detection

> **Executor instructions**: Run every verification command; honor STOP conditions. Update the README row.
>
> **Drift check**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-core/src/agent.rs crates/jackin-capsule/src/agent_status/rules.rs crates/jackin-image/src/derived_image.rs crates/jackin-capsule/src/runtime_setup.rs docker/runtime/agent-status/packs`

## Status

- **Implementation status**: IN PROGRESS in PR #714. Steps 1, 3, and 4 are landed. Step 2 is partially unblocked:
  real Grok working captures from the operator's jackin❯ session now back an embedded `grok.toml` working pack.
  Grok blocked and idle rules remain open until real captures exist; do not fill them from guessed strings or
  herdr artifacts.
- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: 005 (real goldens, so a grok pack is verifiable)
- **Category**: bug (detection coverage)
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

`grok` is a first-class agent (`Agent::Grok`, `slug()="grok"`, bootable via `setup_grok`) with **zero
detection wiring** — no `grok.toml`, no hook, no reporter — so a grok pane can never show blocked and defaults
to Unknown → blank tab, permanently. This shipped because `Agent::ALL` and the detection-asset sets
(embedded packs, image-baked assets, reporter installers) are **independent parallel lists with no
exhaustiveness check**: adding `Grok` compiled and passed CI with nothing asserting every agent has a
detector. Two silent sinks compound it: a single malformed embedded pack aborts the **whole** registry
(all agents go dark), and an unmatched agent resolves to Unknown with no operator-visible signal. Root cause:
no coupling between the agent enum and the detector set; and detection failures are silent. Prefer the
structural fix (exhaustiveness + no-silent-empty) over just "add grok.toml" — that removes the condition that
lets the *next* agent ship undetectable.

## Current state

- `crates/jackin-core/src/agent.rs:32-39` — 6 agents in `ALL`; `Grok` at `:26,38,48,60,96,130,160`;
  `runtime_setup.rs:292` `"grok" => setup_grok()` (bootable).
- `docker/runtime/agent-status/packs/` — 5 files, **no grok.toml**.
- `crates/jackin-image/src/derived_image.rs:25-62` — `AGENT_STATUS_ASSETS` bakes 5 packs + 3 hooks (no grok).
- `crates/jackin-capsule/src/agent_status/rules.rs:363-375` — `load_embedded_packs` `include_str!`s 5 packs and
  uses `?` on both `toml::from_str` and `finalize` — one bad pack aborts the whole registry.
- `crates/jackin-capsule/src/daemon.rs:870-876` — a registry `Err` → `rule_registry = None` → `session.rs:986`
  yields `ScreenEvidence::default` for **all** agents; logged only via `clog!`.
- Contrast the operator-override loader `rules.rs:810-832` (`load_packs_from_dir`) which **skips-and-logs** one
  bad pack (correct). The embedded path does not.
- `runtime_setup.rs:298-300` — reporter-install failure is non-fatal (logged); `hook_installer.rs` PluginInstaller
  bails (doesn't clobber) on corrupt `plugins.json`, so opencode's plugin can silently not install; `verify`
  (`hook_installer.rs:249-251`) is substring-only.
- Reference: herdr ships a grok manifest (`herdr/src/detect/manifests/grok.toml`, blocked+working rules).

## Scope

**In scope:** a new `docker/runtime/agent-status/packs/grok.toml` (+ golden fixture), `AGENT_STATUS_ASSETS`
and `load_embedded_packs` (add grok), an exhaustiveness test over `Agent::ALL`, per-pack isolation in the
embedded loader, and loud degradation for a `None` registry / failed reporter install. **Out of scope:** the
existing packs' rule content (plan 007); the render layer (plan 001).

## Steps

### Step 1: Exhaustiveness test — `Agent::ALL` ⊆ detectors (removes the parallel-list condition)

Add a test that iterates `Agent::ALL` and asserts each slug has an embedded pack — OR is on an explicit,
reviewed `NO_SCREEN_DETECTOR` opt-out list (so "this agent intentionally has no pack" is a conscious, reviewed
decision, not an accident). Optionally a parallel assertion for reporter coverage. This test must fail today
for grok (until Step 2).

**Verify**: the test fails for grok before Step 2, passes after.

### Step 2: Add the grok pack (author from real chrome; herdr's grok signals as a guide, approach only)

Create `grok.toml` with blocked (permission/approval prompt) and working (spinner/interrupt) and idle
(prompt caret) rules, matched against a **real captured grok golden** (plan 005's harness). Add it to
`AGENT_STATUS_ASSETS` (`derived_image.rs`) and `load_embedded_packs` (`rules.rs`). Do **not** copy herdr's
TOML verbatim (AGPL) — author jackin's own from real output.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/exhaustive|packs_load/)'` → pass;
`grep -rn "grok" docker/runtime/agent-status/packs crates/jackin-image/src/derived_image.rs crates/jackin-capsule/src/agent_status/rules.rs` → present in all three.

### Step 3: Per-pack isolation in the embedded loader (no-silent-empty)

Change `load_embedded_packs` (`rules.rs:363-375`) to load each embedded pack independently — skip-and-log a
bad one (matching `load_packs_from_dir`'s behavior) so one malformed pack cannot zero the other four. And make
a fully-empty/`None` registry an **operator-visible** degradation (a startup notice / `EvidenceNote`), not just
a `clog!` line — "screen detection is off" must be loud.

**Verify**: a test that a registry built from `[good_pack, deliberately_broken_pack]` still loads the good one
(and logs the bad) → pass.

### Step 4: Make reporter-install failure loud + honest verify (DETECT-07)

Keep reporter install non-fatal (observability must not break the agent), but surface a failed opencode plugin
install as an operator-visible warning, and change `verify` (`hook_installer.rs:249-251`) from substring-match
to a parse-validate (confirm the plugin is actually registered in valid JSON), so a corrupt `plugins.json` that
happens to contain the path can't pass verify while opencode fails to parse it.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/hook_installer|reporter|verify/)'` → pass.

## Done criteria

- [x] An exhaustiveness test asserts every `Agent::ALL` slug has a pack or a reviewed opt-out
- [ ] `grok.toml` exists, is baked + embedded, and matches real grok goldens — PARTIAL: working states are backed
  by live captures; blocked and idle states remain open
- [x] One broken embedded pack no longer zeroes the registry (test proves); an empty registry is operator-visible
- [x] Reporter-install failure is loud; `verify` parse-validates rather than substring-matches
- [x] `cargo nextest run -p jackin-capsule` green; clippy clean
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- No real grok output to author the pack from — land Steps 1/3/4 and mark Step 2 `BLOCKED (needs grok capture)`;
  the exhaustiveness test then legitimately fails for grok until the pack exists, so add grok to the reviewed
  opt-out list temporarily with a `TODO(plan-006-grok)` so CI is honest about the gap.

## Maintenance notes

- After this, adding an `Agent` variant without a detector fails the exhaustiveness test — the reviewer's cue
  to add a pack or a conscious opt-out. This is the durable fix; the grok pack is one instance.
- Pairs with plan 002 (identification) — an agent must be both *identified* and *have a pack* to be detected.
