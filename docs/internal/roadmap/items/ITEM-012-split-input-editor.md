# ITEM-012: Split `input/editor.rs` into `input/editor/` directory

**Phase:** 2  
**Risk:** medium  
**Effort:** medium (1–2 days)  
**Requires confirmation:** yes  
**Depends on:** ITEM-002 is helpful but not required; ITEM-011 recommended first (regression net)

## Summary

`src/console/manager/input/editor.rs` is the largest production file in the codebase at ~1141L production (2349L total, tests start at line 1142). It handles keyboard dispatch for all 4 editor tabs (General, Mounts, Agents, Secrets). PR #171 added the Secrets tab, growing this file from ~547L to 1141L production. Split into a module directory with one file per tab.

## Proposed output

```
src/console/manager/input/editor/
  mod.rs       ← re-exports, handle_editor_key dispatcher, handle_editor_modal dispatcher
  general.rs   ← General tab key handlers (~100L)
  mounts.rs    ← Mounts tab key handlers (~150L)
  agents.rs    ← toggle_agent_allowed, toggle_default_agent, open_agent_override_picker (~80L)
  secrets.rs   ← Secrets tab key handlers (~500L, the PR #171 AI-generated section)
```

## Key technical details

- `handle_editor_key` (line 22, ~250L) — main dispatcher; stays in `mod.rs`
- `handle_editor_modal` (line 618, ~276L) — modal commit handling; stays in `mod.rs`  
- Both functions dispatch to tab-specific helpers; the helpers move to per-tab files
- `EditorState` is imported from `state.rs` (or `state/types.rs` post ITEM-016) — no circular import risk
- After split: `state.rs` split (ITEM-014) before or after — independent; `pub use` re-exports handle path resolution

## Import path note

After split, `agents_block_agent_count` at line 246 calls `super::super::agent_allow::allows_all_agents`. With `editor/` being one level deeper, the path becomes `super::super::super::agent_allow::allows_all_agents`. Alternatively, use `crate::console::manager::agent_allow::allows_all_agents` for stability.

## What needs confirmation

The split boundary: should `handle_editor_modal` stay in `mod.rs` or move to a `modal.rs` sub-file? This depends on how large it is post-split and whether it's conceptually a "modal concern" or a "dispatch concern".

## Auditability gain

To audit "did the AI correctly implement the Secrets tab?", a reviewer reads only `secrets.rs` (~500L) instead of scanning 1141L of mixed-tab production code.
