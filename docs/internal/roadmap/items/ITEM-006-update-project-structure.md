# ITEM-006: Update PROJECT_STRUCTURE.md with PR #171 additions

**Phase:** 1  
**Risk:** low  
**Effort:** small (< 1 hour)  
**Requires confirmation:** no  
**Depends on:** none

## Summary

`PROJECT_STRUCTURE.md` line 53 is confirmed stale (iteration 31). The `console/` entry still lists the pre-PR#171 widget set and omits 4 new widgets plus the full manager/ sub-structure. This makes AI agents and contributors unable to find new subsystems via the primary navigation document.

## Specific gaps (verified by reading PROJECT_STRUCTURE.md line 53)

Currently lists in widgets/: `file_browser`, `text_input`, `confirm`, `confirm_save`, `error_popup`, `mount_dst_choice`, `workdir_pick`, `github_picker`, `save_discard`, `panel_rain`

**Missing:**
- `op_picker/` (1712L, 1Password picker state machine)
- `agent_picker.rs` (436L, agent disambiguation modal)
- `scope_picker.rs` (201L, workspace-vs-agent choice)
- `source_picker.rs` (244L, plain-or-1Password choice)

**Also stale:** the manager/ description doesn't reflect the split into `input/`, `render/`, and named subfiles.

## Steps

1. Open `PROJECT_STRUCTURE.md` line 53.
2. Update the `widgets/` list to include the 4 new items.
3. Update the `manager/` description to reflect its actual structure: `state.rs`, `input/` (mouse.rs, editor.rs, save.rs, list.rs, prelude.rs, mod.rs), `render/` (editor.rs, list.rs, mod.rs), `create.rs`, `mount_info.rs`, `agent_allow.rs`, `github_mounts.rs`.
4. Add `op_cache.rs` entry (session-scoped op metadata cache, missing entirely).

## Caveats

None — purely documentation, no code change.
