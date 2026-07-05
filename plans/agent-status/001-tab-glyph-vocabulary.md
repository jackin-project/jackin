# Plan 001: Collapse the tab status into one total glyph vocabulary covering all four states + unknown

> **Executor instructions**: Follow step by step; run every verification command and confirm the expected
> result before moving on. If a STOP condition occurs, stop and report. Update this plan's row in
> `plans/agent-status/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/tui/components/status_bar.rs crates/jackin-capsule/src/tui/components/chrome.rs crates/jackin-capsule/src/tui/model.rs`
> If any changed, compare the "Current state" excerpts against live code; on mismatch, STOP.

## Status

- **Implementation status**: DONE in PR 714 (`VisibleAgentState` is total over `AgentState`, tab glyphs cover
  blocked/working/done/idle/unknown without a catch-all, and status-bar/chrome/model tests pin the mapping)
- **Priority**: P1 (the operator's explicit ask; unblocks all state visibility)
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug (render)
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

The capsule tab can only ever paint **two** state glyphs — blocked (`●` red) and done (`○`). Working, idle,
and unknown all collapse to a blank space. So for the entire normal working→idle lifecycle the tab shows
nothing and never changes; it only changes at the two rarest moments (a blocking dialog, or finishing while
unseen). This alone reproduces the operator's "status never changes," independent of whether detection works.
The operator wants all four states on the tab icon (🔴 blocked, 🟡 working, 🔵 done, 🟢 idle), zero config.
The root cause is structural: the tab glyph is a **separate hand-maintained enum** (`TabGlyph`) from the
authority's state enum (`AgentState`), connected by lossy `match`es with no compiler-enforced totality, and
`TabGlyph` has no variant that can represent working or idle — so no data-path change can surface them; the
type system forbids it.

## Current state

Three drifting vocabularies, narrowing at each hop:

- Authority — `crates/jackin-protocol/src/control.rs` `AgentState = {Working, Blocked, Done, Idle, Unknown}`
  (5 variants; what `session.state` holds and arbitration authors).
- Intermediate — `crates/jackin-capsule/src/tui/model.rs:190-211`:
  ```rust
  pub enum VisibleAgentState { Idle, Working, Done, Blocked }   // 4 — drops Unknown
  pub fn visible_agent_state_from_protocol(state: AgentState) -> VisibleAgentState {
      match state {
          AgentState::Idle => VisibleAgentState::Idle,
          AgentState::Working => VisibleAgentState::Working,
          AgentState::Done => VisibleAgentState::Done,
          AgentState::Blocked => VisibleAgentState::Blocked,
          AgentState::Unknown => VisibleAgentState::Idle,   // <-- lossy fold
      }
  }
  ```
- Render — `crates/jackin-capsule/src/tui/components/status_bar.rs:284-322`:
  ```rust
  pub(crate) enum TabGlyph { None, Done, Blocked }   // 3 — no Working / Idle slot
  fn tab_label(tab: &Tab, states: &[(u64, VisibleAgentState)]) -> (String, TabGlyph) {
      // has_blocked -> Blocked; else has_done -> Done; else None  (working & idle -> None)
  }
  ```
- Paint — `crates/jackin-capsule/src/tui/components/chrome.rs:43-47`:
  ```rust
  let glyph_char = match cell.glyph {
      TabGlyph::None => ' ',
      TabGlyph::Done => '○',
      TabGlyph::Blocked => '●',   // overpainted red at chrome.rs:53-64
  };
  ```
- The glyph slot width is already reserved (`TAB_GLYPH_PLACEHOLDER = " X"`, `status_bar.rs:49`), so adding
  glyphs needs **no layout math change**.
- The tab is fed **live** state each frame (`daemon/compositor.rs:488-493` `snapshot_session_states` reads
  `s.state`), and repaint fires on status change (`daemon.rs:1473-1477`, `FullRedrawReason::StatusChange`).
  The wiring is alive — this plan is render-only.

Theme colors: `jackin_tui::theme::STATUS_BLOCKED_RED` exists (`chrome.rs:62`); a reserved `AMBER` exists per
the design doc. Find the palette: `grep -n "STATUS_BLOCKED_RED\|AMBER\|PHOSPHOR" crates/jackin-tui/src/theme*`.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Build | `cargo check -p jackin-capsule --all-targets` | exit 0 |
| Test | `cargo nextest run -p jackin-capsule -E 'test(/status_bar|chrome|model/)'` | all pass |
| Clippy | `cargo clippy -p jackin-capsule -- -D warnings` | exit 0 |

## Scope

**In scope:** `crates/jackin-capsule/src/tui/model.rs` (add `Unknown` to `VisibleAgentState`, fix the fold),
`crates/jackin-capsule/src/tui/components/status_bar.rs` (`TabGlyph` + `tab_label`),
`crates/jackin-capsule/src/tui/components/chrome.rs` (paint), and their `tests.rs`.
**Out of scope:** detection/arbitration (other plans); the host console's per-pane label; the roll-up logic
(keep `blocked > done > working > idle > unknown`).

## Steps

### Step 1: Make `VisibleAgentState` total over `AgentState` (add `Unknown`)

In `model.rs`, add an `Unknown` variant to `VisibleAgentState` and change the fold so
`AgentState::Unknown => VisibleAgentState::Unknown` (stop aliasing "no evidence" to idle). Every arm is now
1:1; the `match` stays exhaustive with no catch-all.

**Verify**: `cargo check -p jackin-capsule` → exit 0.

### Step 2: Replace `TabGlyph` with a total function of the state

Delete `TabGlyph` as an independent narrowing vocabulary. Either (a) render the glyph directly from
`VisibleAgentState` with a **catch-all-free** `match` (so adding a state to the authority forces a glyph
decision or the build fails), or (b) keep a `TabGlyph` enum but give it a variant per state
(`Blocked, Working, Done, Idle, Unknown`) and make the `VisibleAgentState → TabGlyph` map catch-all-free.
Prefer (a) — one fewer vocabulary. Update `tab_label` to return the per-session glyph for the tab's
worst/rolled-up state using the existing attention priority (`blocked > done > working > idle > unknown`) —
reuse whatever roll-up the authority already uses; do not invent a second priority order.

Assign each state a distinct, low-noise mark:
- Blocked → `●` **red** (`STATUS_BLOCKED_RED`) — keep the high-salience "needs you" signal.
- Done → `○` (default fg) — keep.
- Working → a muted "busy" mark (e.g. a spinner glyph or `◐`/`·`) in a dim/amber tone — **not** attention-red.
- Idle → a visible quiet mark distinct from Done and from text separators. PR #714 uses bright green `◆`; the old dim `·` was too easy to miss beside usage/status separator dots.
- Unknown → blank (` `) — "no evidence" stays visually silent, the intended non-attention state.

Keep the "attention" intent: blocked/done remain the loud pair; working/idle are present but muted so the
tab isn't noisy. (herdr shows all four — `herdr/README.md:31`.)

**Verify**: `cargo check -p jackin-capsule --all-targets` → exit 0;
`grep -n "enum TabGlyph" crates/jackin-capsule/src/tui/components/status_bar.rs` — if kept, it now has a
variant per state; `grep -n "_ =>" crates/jackin-capsule/src/tui/components/status_bar.rs` shows **no**
catch-all in the state→glyph map.

### Step 3: Paint the new glyphs

In `chrome.rs`, extend the paint `match` to cover every glyph variant with its char + color (working muted,
idle muted, blocked red as today, done as today, unknown blank). Keep the width-stable single-cell slot.

**Verify**: `cargo clippy -p jackin-capsule -- -D warnings` → exit 0.

### Step 4: Tests

Update/extend `status_bar/tests.rs`, `chrome/tests.rs`, `model/tests.rs`:
- `visible_agent_state_from_protocol(Unknown) == VisibleAgentState::Unknown` (not Idle).
- Each `AgentState` → a distinct glyph char (assert Working and Idle now produce non-blank, distinct glyphs).
- A tab whose session is Working shows the working glyph (regression for the operator symptom).
- Roll-up: a tab with one blocked + one working session shows blocked (priority preserved).

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/status_bar|chrome|model/)'` → all pass incl. new.

## Done criteria

- [x] `VisibleAgentState` has `Unknown`; the fold is 1:1 (no `Unknown→Idle`)
- [x] The state→glyph map is catch-all-free (adding an `AgentState` variant fails the build)
- [x] Working and Idle each paint a distinct, non-blank, non-red glyph; Unknown is blank
- [x] Idle uses a full-cell visible glyph (`◆`) instead of a tiny separator dot
- [x] `cargo nextest run -p jackin-capsule` green with the new assertions
- [x] `cargo clippy -p jackin-capsule -- -D warnings` exits 0
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- The excerpts don't match (someone already reworked the glyph vocabulary) — re-derive before editing.
- No muted theme color exists for working/idle and adding one touches a shared palette widely — use an
  existing dim tone (`PHOSPHOR_DIM`) rather than introducing a palette entry; if even that isn't available,
  report before adding theme tokens.

## Maintenance notes

- The whole point: after this, the render layer is a *total function* of the authority state — a reviewer
  should reject any re-introduction of a separate narrowing glyph enum with a catch-all.
- 001 makes states *visible*; if a working agent still shows nothing after this, the fault is upstream
  (detection produces Unknown — plans 002–007), not here. That separation is intentional.
- Operator preference on the exact working/idle marks (spinner vs dot, amber vs dim) is a taste call — pick
  sensible defaults; the maintainer can retune the chars/colors without touching the vocabulary structure.
