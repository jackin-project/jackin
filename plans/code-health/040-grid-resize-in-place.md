# Plan 040: In-place terminal grid resize — reuse row storage, add same-size and height-only fast paths

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md` — unless a reviewer dispatched you and told
> you they maintain the index.
>
> **Drift check (run first)**: `git diff --stat 0971da66d..HEAD -- crates/jackin-term/src/grid.rs crates/jackin-term/src/grid/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (hot-path rewrite in the terminal model; mitigated by an equivalence-oracle test, the conformance suite, and the fuzz target — all load-bearing gates that must stay green)
- **Depends on**: plan 014 (soft — its `resize_storm` bench is this plan's before/after instrument; if 014 hasn't landed, Step 1 creates the bench to 014's exact spec so the name stays a stable budget key)
- **Category**: perf
- **Planned at**: commit `0971da66d`, 2026-07-09

## Why this matters

Roadmap Phase 4 item 5 (`docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` line 204): "Add terminal resize fast paths that reuse row storage for height-only or same-width changes and benchmark resize storms." Measured current behavior (ledger PERF-resize-clone, re-verified 2026-07-09): every `DamageGrid::set_size` — even a **same-size** call — rebuilds BOTH the primary and alternate screens from scratch: two full grid allocations, one `Mutex`-guarded O(pool) arena scan **per row**, and a deep clone of every retained cell. There is no fast path of any kind (verified: no `rows ==`/`cols ==` guard exists in the resize code). Interactive window drags deliver resize storms; each event pays 2×(rows×cols) cell clones plus 2×rows lock+scan. This plan makes resize mutate row storage in place — per-row `Vec` truncate/extend, `VecDeque` height adjust — so a same-size resize allocates nothing and other resizes allocate only genuinely new rows.

## Current state

All in `crates/jackin-term` unless noted. Crate hard rules (its `AGENTS.md` — binding): damage is recorded at mutation, never recomputed by re-read; pure Rust, no unsafe; the conformance replay harness, fuzz target, and capsule echo-back harness are load-bearing and must not be weakened.

- **The rebuild** — `src/grid/write.rs:45-59`:

```rust
pub fn resize_grid(grid: &RowStore, rows: u16, cols: u16) -> RowStore {
    let mut new = make_blank_grid(rows, cols, grid.arena.clone());
    for (r, row) in grid.iter().enumerate() {
        if r >= rows as usize { break; }
        new.wraps[r] = grid.wrap(r).unwrap_or_default();
        for (c, cell) in row.iter().enumerate() {
            if c < cols as usize { new[r][c] = cell.clone(); }
        }
    }
    new
}
```

  Its ONLY caller is `set_size`. Semantics to preserve exactly: retained cells are the top-left `rows×cols` rectangle; wrap provenance kept for retained rows; new rows/cells are blank defaults; truncated cells are dropped (including a wide cell's continuation being cut — whatever the current code produces, the equivalence oracle in Step 2 pins it).
- **The caller** — `src/grid.rs:738-762` `DamageGrid::set_size`: clamps to 1×1 minimum, sets `self.rows/cols`, `self.primary = resize_grid(&self.primary, …)`, `self.alternate = resize_grid(&self.alternate, …)` (both **unconditionally**), clamps cursor, `clear_pending_wrap()`, resets `scroll_top/scroll_bottom`, `self.dirty.resize(rows)` (all rows dirty). **Scrollback is untouched by resize** (no reflow exists anywhere); `self.scrollback` is only written by scroll-eviction (`grid.rs:1146-1179`) and `preserve_visible_rows_to_scrollback` (`grid.rs:1203-1256`).
- **Data structures** — `src/grid.rs`: `RowStore { rows: VecDeque<Vec<Cell>>, wraps: VecDeque<RowWrap>, arena: RowArena }` (`:211-216`); `Index/IndexMut` (`:390-402`); `Drop` recycles rows to the arena (`:318-322`); `RowArena = Arc<Mutex<Vec<Vec<Cell>>>>` pool, `blank_row(cols)` does a linear `.position(|row| row.len() == cols)` scan under the Mutex per requested row (`:363-373`), pool cap `MAX_RECYCLED_ROWS = 4096` (`:376`); test-only `recycled_rows()` count (`:385`). `Cell` (`src/cell.rs:77-91`): `CompactString` contents + attrs + hyperlink fields; `#[derive(Clone, Default, PartialEq, Eq)]`.
- **The funnel** (context; NOT modified): capsule `Session::resize` calls `self.shadow_grid.set_size(rows, cols)` at `crates/jackin-capsule/src/session.rs:1441`. Plan 004 operates strictly above this (frame coalescing in the daemon select loop) — clean boundary, no interaction.
- **Existing tests to keep green** (exemplars + regression net):
  - `src/grid/tests.rs:381` `wrap_provenance_survives_scrollback_view_and_resize`
  - `src/grid/tests.rs:916` `resize_to_zero_rows_keeps_grid_addressable` (the 1×1 clamp guard)
  - `tests/conformance.rs:294` `sanity_resize_smaller_then_larger` (snapshot invariants after each resize)
  - dhat harness: `tests/allocation.rs` (`#[cfg(feature = "dhat-heap")]`, asserts zero-alloc dirty-patch path) — the pattern for the new zero-alloc resize assertion.
- **Bench spec (plan 014, reuse verbatim)**: bench name `resize_storm`, file `crates/jackin-term/benches/resize_storm.rs`, criterion `harness = false` `[[bench]]` entry mirroring the existing `present_frame`/`scroll_throughput` entries in `Cargo.toml:50-56`; scenarios: (a) width+height resize of a grid preloaded with realistic content, (b) height-only resize, (c) same-size no-op resize, (d) a storm of 20 alternating resizes; setup copied from `benches/scroll_throughput.rs` (`DamageGrid::new(ROWS, COLS, SCROLLBACK)` + `grid.process(&bytes)`). Run with `cargo bench --bench resize_storm -p jackin-term -- --quick`. **Known spec correction** (report upstream in the PR body, don't silently deviate): 014's fixture note says "preloaded with a realistic scrollback (2000 rows × 200 cols)" — scrollback is NOT processed by resize; preload the *visible grid* with content instead (fill via `grid.process` of a text burst); keep scrollback present so the fixture stays realistic.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Crate tests | `cargo nextest run -p jackin-term` | all pass |
| Conformance suite | `cargo nextest run -p jackin-term -E 'binary(conformance)'` | all pass |
| Fuzz smoke (local) | `cd crates/jackin-term && cargo fuzz run --sanitizer none --target x86_64-unknown-linux-gnu damage_grid_process -- -max_total_time=60` | no crashes |
| dhat lane | `cargo nextest run -p jackin-term --features dhat-heap allocation` | passes |
| Bench | `cargo bench --bench resize_storm -p jackin-term -- --quick` | runs, prints times |
| Full lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Merge-readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (the only files you should modify/create):
- `crates/jackin-term/src/grid.rs` (`set_size` + a new `RowStore::resize` method near the other RowStore methods)
- `crates/jackin-term/src/grid/write.rs` (retire/replace `resize_grid`)
- `crates/jackin-term/src/grid/tests.rs` (equivalence oracle + fast-path tests)
- `crates/jackin-term/tests/allocation.rs` (one zero-alloc resize test)
- `crates/jackin-term/benches/resize_storm.rs` + `crates/jackin-term/Cargo.toml` `[[bench]]` entry (ONLY if plan 014 hasn't already created them — check first)
- `crates/jackin-term/README.md` (structure row if a bench file is added)

**Out of scope** (do NOT touch):
- Scrollback reflow — resize does not and will not touch `self.scrollback` in this plan (a reflow feature is a different, large design).
- `preserve_visible_rows_to_scrollback`, the scroll-eviction path, `last_preserved_block` — different triggers.
- The capsule resize funnel (`session.rs:1403-1445`, daemon coalescing) — plan 004's layer.
- `RowArena`'s scan algorithm — after this plan the arena is barely hit on resize; optimizing its `position` scan is unjustified until measured.
- The dirty-tracking semantics: `self.dirty.resize(rows)` (all rows dirty on any resize, including same-size) stays — renderers depend on a full repaint signal after SIGWINCH; do NOT "optimize" it.

## Git workflow

- Branch: current active branch if the operator designates one; otherwise propose `perf/grid-resize-in-place` and wait for confirmation.
- Conventional Commits, signed, push after each: `test(term): resize equivalence oracle + resize_storm bench`, then `perf(term): resize grid rows in place`.

## Steps

### Step 1: Bench first (the before-instrument)

If `benches/resize_storm.rs` doesn't exist, create it to the 014 spec above (with the fixture correction). Record the BEFORE numbers for all four scenarios in the commit body.

**Verify**: `cargo bench --bench resize_storm -p jackin-term -- --quick` → runs all four scenarios; numbers recorded.

### Step 2: Equivalence oracle test

In `src/grid/tests.rs`, add a table-driven oracle: a local test-only reference fn `naive_resize(grid: &RowStore, rows, cols) -> RowStore` implementing exactly the current `resize_grid` body (copy it verbatim into the test module BEFORE changing production code). Drive both paths across a dimension matrix — {grow, shrink, same} × {rows, cols} including 1×1, wide-cell truncation at the new right edge, wrap-provenance rows, and content in both primary and alternate screens — asserting full equality of cells, wraps, and dims after `set_size` vs. oracle-applied expectations. Build content via `grid.process(bytes)` (the public write path), not by poking internals.

**Verify**: `cargo nextest run -p jackin-term resize_equivalence` → passes against the CURRENT implementation (proves the oracle matches reality before anything changes).

### Step 3: `RowStore::resize` in place

Add `pub(crate) fn resize(&mut self, rows: u16, cols: u16)` on `RowStore` (in `grid.rs`, near the other RowStore methods `:274-315`):

- **Same dims**: return immediately (no storage touched).
- **Width change**: for each retained row `Vec<Cell>`: `row.truncate(cols)` or `row.resize(cols as usize, Cell::default())`. Update nothing else per row (wraps unaffected by width in the current semantics — confirm against the oracle).
- **Height shrink**: pop excess rows off the back of the `VecDeque` and `self.arena.recycle(row)` each (matching what `Drop`/`clear` does today, `grid.rs:318-322`/`:251-256`); truncate `wraps` in parallel.
- **Height grow**: push `self.arena.blank_row(cols)` rows (arena hit ONLY for these genuinely new rows) and default `RowWrap`s.

Then in `set_size` (`grid.rs:738-762`): replace the two `resize_grid` rebuilds with `self.primary.resize(rows, cols); self.alternate.resize(rows, cols);` — everything else in `set_size` (clamps, `clear_pending_wrap`, scroll-region reset, `dirty.resize`) stays byte-identical, INCLUDING on the same-size path (the fast path skips grid work, never the side effects). Delete `resize_grid` and `make_blank_grid`'s resize-only usage from `write.rs` (keep `make_blank_grid` if constructors use it; `dead_code` deny will tell you).

**Verify**: `cargo nextest run -p jackin-term` → ALL pass, including the Step 2 oracle (the oracle is the proof of behavioral identity), `wrap_provenance_survives_scrollback_view_and_resize`, `resize_to_zero_rows_keeps_grid_addressable`. `cargo nextest run -p jackin-term -E 'binary(conformance)'` → passes.

### Step 4: Reuse + zero-alloc assertions

- In `grid/tests.rs`: a reuse test — build a grid, capture `arena.recycled_rows()` (test-only hook, `grid.rs:385`), do a same-size resize and a width-only resize, assert the arena pool count did not grow-and-drain (i.e. no full-grid round-trip through the pool; exact assertion shape: same-size → `recycled_rows()` unchanged; width-only → unchanged too, since rows mutate in place).
- In `tests/allocation.rs` (dhat, model on `focused_process_dirty_patch_path_allocates_zero_after_warmup`): warm up, snapshot `HeapStats`, run one **same-size** `set_size`, assert `total_blocks`/`total_bytes` deltas are 0.

**Verify**: `cargo nextest run -p jackin-term --features dhat-heap allocation` → passes; `cargo nextest run -p jackin-term resize` → passes.

### Step 5: Fuzz + after-bench

Run the local fuzz smoke (command above; the differential target exercises `DamageGrid::process` including post-construction states — resize isn't fuzz-driven, but grid invariants are). Re-run the bench; record AFTER numbers. Expected shape: scenario (c) same-size → near-zero (was 2 full rebuilds); (b) height-only → large win; (a) full resize → wins from per-row in-place mutation; (d) storm → compounding win. Any scenario slower than BEFORE → STOP condition 4.

**Verify**: fuzz 60s no crashes; bench table BEFORE/AFTER in the commit/PR body.

### Step 6: Full gates

**Verify**: `cargo fmt && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo nextest run -p jackin-term -p jackin-capsule && cargo xtask ci --fast` → all exit 0 (capsule included: its `Session::resize` path consumes `set_size`).

## Test plan

- New: the equivalence-oracle matrix test (Step 2 — written and green BEFORE the rewrite; this is the characterization), the arena-reuse test, the dhat zero-alloc same-size test, the `resize_storm` bench (4 scenarios).
- Kept green: wrap-provenance test, 1×1-clamp test, conformance `sanity_resize_smaller_then_larger`, full conformance replay suite, fuzz smoke, capsule daemon/session suites.
- Pattern exemplars: `grid/tests.rs` existing resize tests; `tests/allocation.rs` for the dhat shape; `benches/scroll_throughput.rs` for bench setup.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -n "fn resize_grid" crates/jackin-term/src/grid/write.rs` → empty; `grep -n "fn resize" crates/jackin-term/src/grid.rs` shows `RowStore::resize`
- [ ] `cargo nextest run -p jackin-term -p jackin-capsule` exits 0 (incl. conformance binary)
- [ ] `cargo nextest run -p jackin-term --features dhat-heap allocation` exits 0 with the new same-size zero-alloc test present
- [ ] `cargo bench --bench resize_storm -p jackin-term -- --quick` runs; BEFORE/AFTER table in the PR body; no scenario regressed
- [ ] Local fuzz smoke 60s: no crashes
- [ ] `cargo xtask ci --fast` exits 0
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

1. The `resize_grid`/`set_size` excerpts don't match (drift — someone may have started this).
2. The Step 2 oracle test FAILS against current production code — your oracle transcription is wrong OR current behavior has a quirk the excerpts miss; reconcile before any rewrite.
3. Wide-cell truncation semantics differ between oracle and in-place paths in a way that makes a conformance snapshot change — a snapshot diff on this plan is by definition a behavior change: report, do not re-bless snapshots (crate rule: gates must not be weakened to make a change pass).
4. Any bench scenario is measurably SLOWER after (beyond noise, `--quick` twice to confirm) — the in-place approach has a pathological case; report with numbers.
5. `RowStore`'s structure changed from `VecDeque<Vec<Cell>>` (e.g. plan 026's range-snapshot work landed something structural) — re-verify the approach against the new layout before proceeding.

## Maintenance notes

- Plan 026 (range-scoped scrollback snapshots) touches adjacent code (snapshot/copy paths) but not `set_size` — disjoint; land in either order.
- The `resize_storm` bench name is a stable budget key for plan 017's perf-budget ratchet; never rename it.
- Recorded follow-ups (NOT this plan): scrollback reflow on width change (product decision), arena scan algorithm (unjustified post-fix), `preserve_visible_rows_to_scrollback` double-clone (ledger, low-leverage).
- Reviewer scrutiny: the same-size path must still run the `set_size` side effects (cursor clamp, scroll-region reset, pending-wrap clear, all-dirty) — skipping those changes SIGWINCH-visible behavior; and `wraps`/`rows` VecDeques must stay length-locked in every path.
- Report the 014 fixture correction (scrollback-preload doesn't stress resize) as a one-line note on plan 014's status row when updating the index.
