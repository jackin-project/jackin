# Plan 014: Fix OSC 8 hyperlink identity — id reuse must not repoint earlier cells

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-term/src/grid.rs crates/jackin-term/src/grid/ crates/jackin-term/src/cell.rs`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P1 (correctness bug — mis-navigation on click)
- **Effort**: M
- **Risk**: LOW-MED
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Characterization item 9, verified live as a real bug: hyperlink tokens are interned by OSC 8 `id` only, and the token→URI map is overwritten on reuse. When terminal output reuses an `id=` (or the empty id — which ALL anonymous hyperlinks share) with a different URI, every earlier cell carrying that token silently repoints to the new URI. Hover/click on an old link then navigates to the wrong target — on untrusted model output that is a mis-navigation hazard, not just a rendering nit. The inline `Cell.hyperlink` field stores a per-cell URI copy and is correct; the token-map path used for hover/click disagrees with it.

## Current state

- Token interning by id only, `crates/jackin-term/src/grid.rs:974-988`:

```rust
fn alloc_hyperlink_token(&mut self, id: &str) -> u32 {
    if let Some(&token) = self.osc8_id_to_token.get(id) { return token; }
    if self.osc8_id_to_token.len() >= OSC8_HYPERLINK_CAP { self.clear_hyperlink_maps(); }
    let token = self.next_hyperlink_token;
    ...
    self.osc8_id_to_token.insert(id.to_owned(), token);
    token
}
```

- Overwrite on reuse, `grid.rs:1597-1603` (OSC 8 handler): `let token = self.alloc_hyperlink_token(&id); self.active_hyperlink_token = token; … self.hyperlink_targets.insert(token, uri.clone());` — empty id defaults via `unwrap_or("")` at `:1585-1588`, so all anonymous links share one token.
- Hover resolution through the map: `hyperlink_target_at_content_row` (`grid.rs:1023-1031`).
- Behavior to preserve: `OSC8_HYPERLINK_CAP` bounding (`grid.rs:236,978,1599` — cap triggers `clear_hyperlink_maps`), clear-on-reset (`grid.rs:947-948` region; test `reset_modes_clears_hyperlink_maps` at `grid/tests.rs:1105`), cell-metadata-not-passthrough (`grid/tests.rs:662`), boundedness (`grid/tests.rs:1096`).
- Prior related fix recorded in `DEFECT_LEDGER.md` (unbounded map growth) — this identity bug is distinct and unrecorded.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Crate tests | `cargo nextest run -p jackin-term` | pass |
| Grid module | `cargo nextest run -p jackin-term -E 'test(/grid::tests/)'` | pass |
| Lint | `cargo clippy -p jackin-term --all-targets -- -D warnings` | exit 0 |
| Fuzz smoke (optional) | existing `damage_grid_process` fuzz target still builds | builds |

## Scope

**In scope**: `crates/jackin-term/src/grid.rs` (token allocation + OSC 8 handler + hover lookup), `crates/jackin-term/src/grid/tests.rs`, `DEFECT_LEDGER.md` (new row: symptom, root cause, characterization test, gate), `crates/jackin-term/README.md` only if the module contract text mentions hyperlink identity.

**Out of scope**: renderer/frame OSC 8 re-emission (`Cell.hyperlink` inline path — already correct); protocol changes; scrollback storage format beyond what token semantics require.

## Git workflow

Branch `fix/osc8-hyperlink-identity`; Conventional Commits (`fix(term): …`); `git commit -s`; push per commit.

## Steps

### Step 1: Characterization tests first (currently failing)

Add to `grid/tests.rs`:
1. `osc8_id_reuse_with_new_uri_keeps_earlier_cells` — emit `OSC 8;id=x;https://a` + text, then `OSC 8;id=x;https://b` + text; assert hover target of the FIRST cell range is still `https://a`, second is `https://b`.
2. `osc8_empty_id_updates_do_not_repoint` — same with no id.
3. `osc8_same_id_same_uri_shares_token` — dedupe still works (same id+uri twice → map stays bounded, both ranges resolve to the one URI).

Run them; all three must FAIL against current code (proves they capture the bug).

**Verify**: `cargo nextest run -p jackin-term -E 'test(/grid::tests/)'` → exactly the three new tests fail.

### Step 2: Key identity by (id, uri); immutable targets

Change interning: `osc8_id_to_token: HashMap<(String, String), u32>` keyed by `(id, uri)` — reusing an id with a different URI allocates a fresh token, so `hyperlink_targets` entries become immutable once written (never overwritten). Empty id: key `("", uri)` still dedupes identical anonymous targets while distinct URIs get distinct tokens. Preserve the cap semantics: cap check counts the interning map as before; `clear_hyperlink_maps` untouched. Keep `active_hyperlink` (`Hyperlink { id, uri }`) exactly as-is for the renderer path.

**Verify**: step-1 tests pass; existing `hyperlink_id_map_stays_bounded`, `reset_modes_clears_hyperlink_maps`, `osc8_hyperlink_is_cell_metadata_not_passthrough` still pass unmodified (if `hyperlink_id_map_stays_bounded` asserts an exact map size that legitimately changes under (id,uri) keying, adjust ONLY with a comment explaining the new counting unit).

### Step 3: Ledger + sweep

Add the `DEFECT_LEDGER.md` row (symptom: earlier links repoint on id reuse; root cause: token keyed by id with mutable target map; characterization: the three new tests; gate: none practical beyond tests — say so per ledger convention). Run the full crate suite and clippy.

**Verify**: `cargo nextest run -p jackin-term` → all pass; `cargo clippy -p jackin-term --all-targets -- -D warnings` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

The three characterization tests in step 1 (written failing-first), plus the untouched existing OSC 8 suite. Model assertions on `osc8_hyperlink_is_cell_metadata_not_passthrough` (`grid/tests.rs:662`) for how to feed OSC bytes and read hover targets.

## Done criteria

- [ ] Three new regression tests pass; wrote-failing-first confirmed in the PR description
- [ ] Existing OSC 8 cap/reset/metadata tests pass
- [ ] `hyperlink_targets` entries never overwritten (no `insert` on an existing token with a different URI — assert in test 3 or via debug_assert)
- [ ] `DEFECT_LEDGER.md` row added
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Scrollback rows store tokens whose lifetime outlives `clear_hyperlink_maps` in a way that (id,uri) keying makes WORSE (dangling tokens after cap-clear already exist as behavior — do not "fix" cap semantics here; if the interaction is more entangled than described, report).
- The fix requires touching frame/protocol encoding of hyperlinks.
- Perf-sensitive: if the (String,String) key measurably regresses the parser hot path per existing benches (`cargo bench -p jackin-term` scroll/present benches), report numbers before optimizing with interned strings.

## Maintenance notes

- Width-change reflow and scrollback interactions with hyperlink tokens are adjacent roadmap work (performance item on `preserve_visible_rows_to_scrollback`); whoever touches those paths must keep the three regression tests green.
- The cap-clear still orphans tokens in old cells by design (pre-existing); a future plan may address cap semantics — deliberately out of scope here.
