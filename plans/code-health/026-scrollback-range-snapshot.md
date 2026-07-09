# Plan 026: Phase 2/4 — range-scoped scrollback snapshots: stop materializing 10k rows to read a few

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat b42c97d4c..HEAD -- crates/jackin-capsule/src/tui/pane_snapshot.rs crates/jackin-capsule/src/daemon/mouse_input.rs crates/jackin-capsule/src/session.rs crates/jackin-term/src/`
> On a mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (selection/copy and link-hover correctness depend on row indexing staying consistent between the full and ranged forms)
- **Depends on**: plans/code-health/014-hot-path-bench-coverage.md (its `scrollback_snapshot` bench provides the before/after numbers — land 014 first or bring its bench along)
- **Category**: perf
- **Planned at**: commit `b42c97d4c`, 2026-07-09

## Why this matters

The first-wave audit ranked this the highest-leverage remaining perf item (PERF-scrollback-snapshot), and the roadmap names it twice: Phase 2 capsule item 4 ("Bound full scrollback snapshot allocation by adding range-based snapshot APIs for copy/selection paths") and Phase 4 item 1 (scrollback snapshotting as a measured hot path). Measured behavior: `pane_content_from_damagegrid` materializes the **entire** retained scrollback (up to the 10k-row bound) plus the live screen into owned `RowSnapshot`s — every cell an owned value — even when the caller needs a handful of rows; and it sits on a per-mouse-event path: Ctrl/Alt link-hover resolution calls `session.render_content_snapshot(cols)` per candidate motion event (`daemon/mouse_input.rs:653`), so dragging the pointer across a pane with deep scrollback allocates tens of megabytes per second of pure garbage. A range-scoped API bounds the allocation to the rows actually inspected.

## Current state

Verified at the planning commit.

- The full-materialization function, `crates/jackin-capsule/src/tui/pane_snapshot.rs:172-195`:

  ```rust
  /// Build a content-coordinate snapshot: retained scrollback rows oldest-first,
  /// followed by the current live screen. Selection copy uses this so a range can
  /// span outside the currently visible viewport.
  pub(crate) fn pane_content_from_damagegrid(
      grid: &jackin_term::DamageGrid,
      viewport_cols: u16,
  ) -> Vec<RowSnapshot> {
      let (screen_rows, screen_cols) = grid.size();
      let cols_to_draw = viewport_cols.min(screen_cols);
      let filled = grid.scrollback_len();
      let scrollback_rows = grid.scrollback_rows_at_offset(filled, filled);
      let mut snapshot =
          Vec::with_capacity(scrollback_rows.len().saturating_add(screen_rows as usize));
      for sb_row in scrollback_rows {
          snapshot.push(snapshot_damagegrid_cells(sb_row, cols_to_draw));
      }
      for row in 0..screen_rows {
          snapshot.push(snapshot_damagegrid_row(grid, row, cols_to_draw));
      }
      snapshot
  }
  ```

  Content coordinates: index 0 = oldest scrollback row; live-screen rows follow at indices `scrollback_len()..scrollback_len()+screen_rows`.
- The per-mouse-event caller, `crates/jackin-capsule/src/daemon/mouse_input.rs:645-660`: hover/click URL resolution fetches `let rows = session.render_content_snapshot(candidate.inner.cols);` then resolves a target at ONE content cell (`candidate.anchor_row`, `candidate.anchor_col`) — it needs the anchor row and (for multi-row hyperlink/text runs) a few neighbors, not the world. Find `render_content_snapshot`'s definition in `crates/jackin-capsule/src/session.rs` (grep) — it wraps `pane_content_from_damagegrid`.
- Other callers of `pane_content_from_damagegrid`/`render_content_snapshot` (inventory them: `rg -n 'pane_content_from_damagegrid|render_content_snapshot' crates/jackin-capsule/src`): expect selection-copy (legitimately needs the selected range — which is still a range, not everything) and possibly tests. Each caller is classified in Step 1.
- The term-side accessor already used: `grid.scrollback_rows_at_offset(filled, filled)` (jackin-term). Read its signature/semantics in `crates/jackin-term/src/` (grep `scrollback_rows_at_offset`) — the ranged API likely composes from it with a narrower request rather than needing new jackin-term surface. If jackin-term CAN'T serve a sub-range without copying everything, the plan extends jackin-term minimally (see Step 2 branch).
- Bench: plan 014 adds `crates/jackin-capsule/benches/scrollback_snapshot.rs` with (a) full-range and (b) narrow-range cases — the narrow-range case is this plan's target metric. If 014 has not landed, create that bench first exactly as 014 Step 3 specifies (same file name/shape) so the before/after is measured either way.
- Conventions: no panics/indexing violations (mind plan 019's future lints — use `.get()` in new code); damage/TUI invariants per `crates/jackin-capsule/AGENTS.md` and `crates/jackin-term/AGENTS.md` (read both before editing); tests sibling `tests.rs`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Capsule tests | `cargo nextest run -p jackin-capsule` | all pass |
| Term tests (if touched) | `cargo nextest run -p jackin-term` | all pass |
| Bench before/after | `cargo bench --bench scrollback_snapshot -p jackin-capsule -- --quick` | narrow-range case improves ≥10× alloc/time vs full |
| Clippy | `cargo clippy -p jackin-capsule -p jackin-term --all-targets -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-capsule/src/tui/pane_snapshot.rs` (+ its `pane_snapshot/tests.rs`): add `pane_content_range_from_damagegrid(grid, viewport_cols, content_rows: Range<usize>) -> Vec<RowSnapshot>` (name matching the existing function's style)
- `crates/jackin-capsule/src/session.rs`: a ranged sibling of `render_content_snapshot`
- `crates/jackin-capsule/src/daemon/mouse_input.rs`: the hover/click resolution path switches to the ranged call
- `crates/jackin-term/src/…` ONLY if a sub-range accessor is genuinely missing (Step 2 branch; minimal addition + tests)
- `crates/jackin-capsule/benches/scrollback_snapshot.rs` (from 014; create if absent)
- Roadmap Phase 2 capsule item 4 status; `plans/code-health/README.md` (row + strike PERF-scrollback-snapshot)

**Out of scope**:
- Changing selection-copy semantics or `RowSnapshot`'s shape
- A borrowed (zero-copy) row API — the audit's fuller vision; the ranged owned API captures most of the win at a fraction of the lifetime-plumbing risk; record borrowed rows as the follow-up
- Any daemon decomposition; any render-path change beyond the snapshot call
- `pane_content_from_damagegrid` callers that genuinely need the full range (they keep the existing function)

## Git workflow

- Branch off `main`: `perf/scrollback-range-snapshot`.
- Commits: ranged API + tests; caller switch; bench numbers in the PR body. `-s`, push each. PR to `main`; do not merge. Capsule PR → capsule smoke block mandatory, and the smoke verify list must include: link-hover open still resolves the right URL in a pane with deep scrollback, and selection copy across the scrollback boundary still yields the same text.

## Steps

### Step 1: Caller inventory and range derivation

List every caller of `pane_content_from_damagegrid` and `render_content_snapshot`. For each: what content-row range does it actually consume? Expected classification — mouse hover/click resolution: `anchor_row ± small window` (read `resolve_host_open_target_at_content_cell` to learn how many neighbor rows link-run resolution can touch; derive the window from that code, don't guess); selection copy: the selection's row range; anything else: full (keeps old API). Write the classification into the PR description draft.

**Verify**: inventory complete; window size for hover derived from code with a file:line citation.

### Step 2: The ranged API

In `pane_snapshot.rs`, add the ranged function: clamp `content_rows` to `0..scrollback_len+screen_rows`, fetch only the needed scrollback slice and/or screen rows, return `Vec<RowSnapshot>` for exactly the requested rows (caller keeps the range→index mapping: element `i` = content row `content_rows.start + i`). Reuse `snapshot_damagegrid_cells`/`snapshot_damagegrid_row`. Branch: if `scrollback_rows_at_offset(filled, filled)` is the only scrollback accessor and it cannot express "rows a..b" without materializing all — add the minimal ranged accessor to jackin-term next to it (same semantics, offset+len parameters it likely already has — read it first; its two-argument form suggests offset/len exist already, in which case NO term change is needed, just pass the right offset/len).

`pane_snapshot/tests.rs` additions: ranged result equals the corresponding slice of the full snapshot for (a) range entirely in scrollback, (b) range spanning the scrollback/live boundary, (c) range entirely in live screen, (d) out-of-bounds range clamps (empty or truncated, matching your clamp choice), (e) empty scrollback. Property: for random small grids, `ranged(r) == full()[r]` — plain loops over a few seeded cases are fine (no proptest dep yet).

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/pane_snapshot/)'` → all pass incl. ≥5 new; `cargo nextest run -p jackin-term` if touched.

### Step 3: Switch the mouse path

`session.rs`: add the ranged wrapper (`render_content_snapshot_range(cols, rows)`). `mouse_input.rs:653` region: compute the window around `candidate.anchor_row` from Step 1's derivation, call the ranged wrapper, and adjust `resolve_host_open_target_at_content_cell` inputs for the new indexing (pass the range start so absolute content coordinates keep working — do NOT re-base coordinates inside the resolver; adapt at the call site).

**Verify**: `cargo nextest run -p jackin-capsule` → all pass (existing mouse/link tests must be green unchanged — they are the characterization); clippy clean.

### Step 4: Measure + docs

Run the bench before (git stash your changes or use the base commit build) and after; record both numbers in the PR body — expect the narrow-range case to drop from O(scrollback) to O(window). Roadmap Phase 2 capsule item 4: ranged API shipped, borrowed-rows follow-up recorded. Ledger: strike PERF-scrollback-snapshot → this plan; add "borrowed row accessor" as the residual.

**Verify**: `cargo bench --bench scrollback_snapshot -p jackin-capsule -- --quick` numbers in PR body; `cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- ≥5 ranged-vs-full equivalence tests (Step 2) — the load-bearing correctness net.
- Existing mouse-input/link-hover and selection tests pass unchanged.
- Bench delta recorded (this is a perf plan; a diff without numbers is incomplete).
- Live capsule smoke (operator): hover-open + cross-boundary selection copy per the Git-workflow verify list.

## Done criteria

- [ ] Ranged snapshot API exists with the equivalence tests green
- [ ] Mouse hover/click resolution no longer calls the full-scrollback snapshot (`rg -n 'render_content_snapshot\b' crates/jackin-capsule/src/daemon/mouse_input.rs` → only the ranged form)
- [ ] Full-range callers untouched; capsule + term suites green; clippy clean
- [ ] Bench before/after in the PR body showing the narrow-range win
- [ ] Roadmap + ledger updated; `plans/code-health/README.md` row updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

Stop and report back if:

- `scrollback_rows_at_offset` cannot serve a sub-range AND extending jackin-term requires touching its damage-tracking invariants (read `crates/jackin-term/AGENTS.md` first — if the addition conflicts with a stated invariant, report).
- The hover resolver's neighbor window cannot be bounded from the code (link runs can span arbitrarily many rows) — then the ranged call needs a different strategy (e.g. expand-on-demand), which is a design change to report.
- Any existing mouse/selection test fails after the caller switch (indexing regression — do not "fix" the test).
- The equivalence property fails for boundary-spanning ranges (off-by-one in the scrollback/live seam — fix the new code, and if the FULL function turns out to have the seam bug, that is a defect-ledger entry, report it).

## Maintenance notes

- The borrowed-row (zero-copy) accessor remains the deeper win for selection-copy of huge ranges; it builds on this API's range plumbing. Recorded in the ledger.
- Plan 019's wave-2 slice lints will hit this file later; new code here should already use `.get()`-style access.
- Reviewer should scrutinize: coordinate re-basing at the mouse call site (absolute content coordinates must survive), and the window-size derivation (too small = broken multi-row link opens; the derivation citation in the PR body is the check).
