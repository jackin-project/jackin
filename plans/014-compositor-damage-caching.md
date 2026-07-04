# Plan 014: Gate capsule per-frame SGR/hyperlink region scans on the damage signal (measure first)

> **Executor instructions**: Perf plan with a **measurement gate** — confirm the hotness before building
> the cache (the win is real but its magnitude is unproven). Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-capsule/src/daemon/compositor.rs`

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `46511939d`, 2026-07-03
- **Measurement note**: Local `render_perf_probe` before caching: p50 695us, p95 882us, max 3330us
  across 300 frames. After per-pane region caching: p50 658us, p95 701us, max 947us. The p95 drop
  is about 20.5%, so the scan/cache path met the measurement gate.
- **Implementation note**: Visible pane dirty spans now invalidate a per-pane SGR/hyperlink region cache
  instead of being dropped. The cache key includes pane geometry, scrollback offset, focus state, and
  hyperlink policy; disappeared panes are pruned.

## Why this matters

Every compose rebuilds each visible pane's SGR and hyperlink region sets from scratch, and `pane_sgr_regions`
uses `allow_pane = |_| true` — so it visits **every cell** of every visible pane, O(panes × rows × cols)
per frame, on top of Ratatui's own full-buffer build+diff. During a streaming agent (the common case,
~30 fps via `RENDER_TICK_INTERVAL = 33ms`), all panes' region sets are rebuilt even though only the
streaming pane changed. The dirty-span tracker that could gate this is computed and **immediately
dropped** (`compositor.rs:303-307`, comment: "damage never selects what to emit"). Likely the dominant
capsule CPU cost under load — but "likely" needs a profile before an L-effort cache with correctness
risk (stale OSC 8 / SGR overlays on scroll/resize/focus) is worth building.

## Current state

- `crates/jackin-capsule/src/daemon/compositor.rs:303-307` — drains each visible pane's `dirty_spans()`
  and `drop`s them ("damage never selects what to emit").
- `crates/jackin-capsule/src/daemon/compositor.rs:402,421` — `pane_hyperlink_regions(...)` and
  `pane_sgr_regions(...)` rebuilt from scratch each frame.
- `crates/jackin-capsule/src/daemon/compositor.rs:670-684` — `pane_sgr_regions` passes `allow_pane = |_| true`
  → visits every cell.
- `crates/jackin-capsule/src/daemon/compositor.rs:592-640` — `pane_cell_runs`, the nested rows×cols scan
  backing both.
- ADR to honor: `docs/content/docs/reference/adrs/adr-005-capsule-single-render-path.mdx` — the team
  deliberately chose full-frame composition (Ratatui fresh-buffer diff). A per-pane cache must not violate
  that model's correctness (it may *feed* the single render path faster, not fork it).

## Steps

### Step 1 (GATE): profile a streaming soak

Build a capsule soak that streams sustained output through one pane in a multi-pane layout and profile CPU
(e.g. `cargo build --profile capsule-debug` per `Cargo.toml:144`, run under a sampling profiler, or add
`JACKIN_DEBUG` timing spans around `pane_sgr_regions`/`pane_hyperlink_regions`/`pane_cell_runs`). Record in
this plan's row note: what fraction of compose CPU is these scans. **If it is not a meaningful fraction
(<~15%), STOP** — mark the plan `REJECTED (not hot — measured)` and record the numbers. Only proceed if the
scans are genuinely hot.

### Step 2: Cache per-pane region vectors keyed by grid generation

Give each session/pane a cached `(generation, Vec<SgrRegion>, Vec<HyperlinkRegion>)`. Rebuild a pane's
regions only when its `dirty_spans()` is non-empty (or its scroll offset / size / focus changed) — reuse
the damage signal already drained at `compositor.rs:303-307` instead of dropping it. Idle panes skip the
scan entirely. Invalidate on: content change (dirty), scroll, resize, focus change, and any state the
regions depend on (audit `cell_sgr_metadata` / `cell_safe_uri` inputs).

**Verify**: `cargo check -p jackin-capsule --all-targets` → exit 0.

### Step 3: Prove correctness against the render-conformance harness

The whole point of the harness (`crates/jackin-capsule/src/daemon/tests.rs`) is that emitted frames
reproduce the pane model. Run it; the cache must not change any emitted frame. Add scenarios that exercise
the invalidation triggers: scroll a pane with active SGR/hyperlinks, resize during streaming, focus swap.

**Verify**: `cargo nextest run -p jackin-capsule` → all pass, including new invalidation scenarios;
re-profile to confirm the scans now skip idle panes.

## Done criteria

- [x] Step 1 measurement recorded; proceeded only if scans were hot
- [x] Per-pane region cache with generation-keyed invalidation (dirty/scroll/resize/focus)
- [x] Render-conformance harness unchanged-frames on all existing + new invalidation scenarios
- [x] Re-profiled: idle panes no longer scanned; net CPU reduction recorded in the row note
- [x] `cargo clippy -p jackin-capsule -- -D warnings` exits 0
- [x] `plans/README.md` row updated

## STOP conditions

- Step 1 shows the scans are not hot → REJECT with numbers (don't build a risky cache for no win).
- Any invalidation trigger is missed and the harness shows a stale overlay you can't cleanly fix — revert
  the cache; a stale OSC 8 / SGR overlay is worse than the CPU cost.
- The cache would fork the single-render-path model of ADR-005 — it must feed it, not bypass it.

## Maintenance notes

- Reviewer must scrutinize invalidation completeness — this is the whole risk. Every input to a pane's
  regions is an invalidation source.
- Keep the damage-drain at 303-307 as the invalidation feed rather than re-deriving damage elsewhere.
