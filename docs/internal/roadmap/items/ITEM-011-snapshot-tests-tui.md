# ITEM-011: Add snapshot tests for TUI render output

**Phase:** 1  
**Risk:** low  
**Effort:** medium (1–2 days)  
**Requires confirmation:** no  
**Depends on:** none

## Summary

The TUI has 16 `#[allow(clippy::too_many_lines)]` suppressions across 11 files. Any refactor touching `render/list.rs` (1989L) or `render/editor.rs` (1666L) currently has no automated regression net. Snapshot tests using `insta` would pin the rendered output of key functions so refactors can't silently change the TUI appearance.

## Three target functions (verified by reading the files)

| Function | File | Line | What to snapshot |
|---|---|---|---|
| `render_sentinel_description_pane` | `render/list.rs` | 332 | Static "+ New workspace" panel. Takes only frame + area. Zero state. 80×10 terminal, ~10 lines of test code. |
| `render_tab_strip` | `render/editor.rs` | 269 | Tab bar with `EditorTab` active. Takes frame + area + `EditorTab` enum. 4 variants × 1 snapshot each. 80×3 terminal. |
| `render_mounts_subpanel` | `render/list.rs` | 433 | Mount list subpanel. Takes frame + area + `&[MountConfig]`. 3 cases: empty, 1 mount, 3 mounts. 60×20 terminal. |

## Testing library comparison

| Library | Stars | Approach | `jackin` fit |
|---|---|---|---|
| `insta` | ~4k | String snapshots with `cargo insta review` | Best — simple, no async needed, inline or file-based snapshots |
| `ratatui::backend::TestBackend` | built-in | Raw buffer comparison | Already used in existing tests; no new dep |
| `trycmd` | ~400 | CLI snapshot testing | Wrong layer — tests full CLI, not individual render functions |

**Recommendation:** `insta` for snapshot storage and review workflow; `ratatui::backend::TestBackend` for the buffer (already available — no extra dependency just for the backend).

## Steps

1. Add `insta` to `[dev-dependencies]` in `Cargo.toml`.
2. Write 3 test modules (one per target function) using `TestBackend` to capture the rendered buffer, then `insta::assert_snapshot!()` to pin it.
3. Run `cargo insta review` to accept the initial snapshots.
4. Add `cargo insta test` to the CI `check` job (or rely on `cargo nextest run` which already runs snapshot comparisons).

## MountConfig rename caveat

If ITEM-013 (naming pass) renames `MountConfig` → `MountSpec`, the test fixture construction changes from `MountConfig { ... }` to `MountSpec { ... }`. Write tests against the current name; rename is a mechanical find-replace.
