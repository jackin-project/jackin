# Mount Collapse — Redundant Descendant Removal

**Status:** Proposed
**Date:** 2026-04-22
**Scope:** `jackin` crate only

## Problem

When an operator adds a parent mount to a workspace whose descendants are already mounted, the descendant mounts become strictly redundant inside the container. The CLI today keeps them as-is — the `workspace show` output carries noise, and the stored config violates the intuitive "every mount is doing work" invariant.

Example (generic names; do not use real project names in PR descriptions):

```
Mounts before:
  ~/Projects/proj-alpha/sub-a   → ~/Projects/proj-alpha/sub-a   (rw)
  ~/Projects/proj-alpha/sub-b   → ~/Projects/proj-alpha/sub-b   (rw)

Operator runs:
  jackin workspace edit proj-alpha --mount ~/Projects/proj-alpha

Desired result:
  ~/Projects/proj-alpha         → ~/Projects/proj-alpha         (rw)
```

Both child mounts are strictly covered by the new parent: same host subtree, same container location. Retaining them is pure noise.

## Scope and non-goals

**In scope**
- Detection of parent/descendant redundancy (rule C, below).
- Auto-removal on write, with operator-facing prompt / printed summary.
- Conflict handling for readonly mismatches and child-under-existing-parent edits.
- A `jackin workspace prune <name>` subcommand for cleaning pre-existing violations.
- Accommodation for configs written before this feature shipped.

**Out of scope**
- Config file versioning and migration framework. Deferred to a separate brainstorming session.
- Runtime mount merging (`resolve_load_workspace`). Ad-hoc `-v` mounts stay literal at launch.
- Changes to `MountConfig` serialization or wire format.

## Rule C — the covering predicate

Mount **P** *covers* mount **C** iff:

1. `P.src` is an ancestor of or equal to `C.src` (after tilde-expansion and `../` normalization via `normalize_path` at `src/workspace.rs:52`), AND
2. the path suffix `C.src − P.src` equals the path suffix `C.dst − P.dst`.

Equivalently: **C** projects the same host subtree to the same container location that **P** would already expose it at.

Identity (equal `src` and equal `dst`) is *not* covering. That case is already handled by `edit_workspace`'s upsert-by-`dst` behavior at `src/config.rs:395`.

Two alternative rules considered and rejected:
- "Only the `src` axis matters" — too aggressive; would silently drop deliberate host-to-different-container remappings.
- "Both axes must nest independently" — too loose; would collapse pairs that expose the same host subtree at different container locations (not actually redundant).

Rule C is the only rule where "the child is redundant" is literally true.

## Conflict cases

Two situations where redundancy is detected but cannot be silently collapsed. Both are rejected with a targeted error.

### ReadonlyMismatch

Parent covers child but `P.readonly ≠ C.readonly`. Collapsing would either broaden access (ro child becomes writable) or narrow it (rw child becomes ro). Either is a semantic change the operator should decide consciously.

Error message template:
```
mount ~/Projects/proj-alpha (rw) would subsume ~/Projects/proj-alpha/sub-a (ro),
but the readonly flag differs. Match the flag or remove the child first.
```

### ChildUnderExistingParent

An edit introduces a child mount when an existing mount already covers it. The child is strictly redundant the moment it's written, even if the readonly flag matches. Rejecting makes the no-op visible to the operator instead of hiding it.

Error message template:
```
mount ~/Projects/proj-alpha/sub-a is already covered by existing mount
~/Projects/proj-alpha. Nothing to add.
```

## Data types

New additions to `src/workspace.rs`:

```rust
pub struct CollapsePlan {
    pub kept: Vec<MountConfig>,
    pub removed: Vec<Removal>,
}

pub struct Removal {
    pub child: MountConfig,
    pub covered_by: MountConfig,
}

pub enum CollapseError {
    ReadonlyMismatch { parent: MountConfig, child: MountConfig },
    ChildUnderExistingParent { parent: MountConfig, child: MountConfig },
}

pub fn plan_collapse(
    mounts: &[MountConfig],
    new_indexes: &[usize],
) -> Result<CollapsePlan, CollapseError>;

fn covers(parent: &MountConfig, child: &MountConfig) -> bool;
```

`new_indexes` identifies which entries in `mounts` are being introduced by the current operation. For `create_workspace`, all indexes are new. For `edit_workspace`, only the upserts are new. Violations where the child is new and the covering parent is pre-existing become `ChildUnderExistingParent`. Violations where both are pre-existing become a "pre-existing redundancy" signal (handled by the CLI layer).

## Algorithm

```text
errors = []
removals = []
kept = []

for each (i, m) in mounts:
    parent = first p in mounts where covers(p, m)
    if parent:
        if parent.readonly != m.readonly:
            errors.push(ReadonlyMismatch { parent, child: m })
            continue
        if i in new_indexes and index_of(parent) not in new_indexes:
            errors.push(ChildUnderExistingParent { parent, child: m })
            continue
        removals.push(Removal { child: m, covered_by: parent })
    else:
        kept.push(m)

if errors.any(): return Err(first error)
return Ok(CollapsePlan { kept, removed: removals })
```

Edge cases handled:

- **Transitive chains.** `A ⊃ B ⊃ C` in the same input: B and C each find A as a covering parent and are removed; A is kept. No topological pass needed because rule C is transitive.
- **Multiple siblings under one parent.** Independently removed under the same parent.
- **Multiple simultaneous violations on one pair.** Implementation returns the first error encountered. Spec does not promise which.

## Trigger scope

The collapse rule runs on every write path that persists a `WorkspaceConfig`:

| Path | Behavior |
|------|----------|
| `create_workspace` (`src/config.rs:345`) | Auto-collapse + printed stderr summary. Conflict cases error. |
| `edit_workspace` (`src/config.rs:358`) | Interactive prompt before collapse (unless `--yes`). Conflict cases error. |
| `resolve_load_workspace` (`src/workspace.rs:324`) | **No change.** Ad-hoc `-v` mounts and global named mounts stay literal at launch. |

### Load-path behavior

Pre-existing violating configs (written before this feature, or hand-edited) load without error. Rule C is a *write* invariant, not a *load* invariant. This keeps old configs usable indefinitely.

### Write-path post-condition

Both `edit_workspace` and `create_workspace` re-run `plan_collapse` on their final mount set as a post-condition. If the plan contains removals or errors, the library bails — the caller did something wrong. In normal CLI flow the CLI pre-collapses and this check is a no-op, but the library-level guard prevents bad state from reaching disk regardless of caller.

## CLI behavior

### `jackin workspace edit` — interactive

1. Parse `--mount` flags into upsert `MountConfig`s.
2. Load the existing workspace. Apply upsert-by-`dst` merging first (same semantics as `edit_workspace` today at `src/config.rs:395`): for each upsert, if an existing mount shares its `dst`, replace in-place; otherwise append. Call the result the *post-upsert list*. Track which entries originated from the current edit as `new_indexes`.
3. Call `plan_collapse(post_upsert_list, new_indexes)`.
4. Partition `plan.removed` by origin:
   - **edit-driven removal** — at least one of `{child, covered_by}` is in `new_indexes`.
   - **pre-existing removal** — neither `child` nor `covered_by` is in `new_indexes` (both were already in the workspace before this edit).
5. Outcomes:
   - **`Err(CollapseError)`** → print error, exit non-zero. No prompt, no config write.
   - **`Ok(plan)` with no removals at all** → proceed to `edit_workspace` with the upserts as-is.
   - **`Ok(plan)` with only edit-driven removals** → prompt (see below) listing the edit-driven removals, then write `plan.kept` or abort.
   - **`Ok(plan)` containing any pre-existing removals, `--prune` NOT set** → reject with "workspace already contains redundant mounts: […]; run `jackin workspace prune <name>` to clean up, or pass `--prune` to this edit." Exit non-zero. (This holds even when there are no edit-driven removals in the same plan.)
   - **`Ok(plan)` containing pre-existing removals, `--prune` set** → prompt lists *both* categories in labeled sub-lists ("adding X will subsume…" + "cleaning up pre-existing redundant mounts…"), then write `plan.kept` or abort.

### Prompt

```
Adding mount ~/Projects/proj-alpha will subsume 2 existing mounts:
  • ~/Projects/proj-alpha/sub-a
  • ~/Projects/proj-alpha/sub-b
These will be removed from the workspace.
Proceed? [y/N]
```

Implemented with `dialoguer::Confirm`, default `false`. Matches the pattern already used by `confirm_sensitive_mounts` at `src/workspace.rs:194`.

### Flags

- `--yes` / `-y` — skip the prompt. Does NOT bypass `ReadonlyMismatch` / `ChildUnderExistingParent` errors.
- `--prune` — opt into cleaning up pre-existing rule-C violations as part of this edit.

### Non-TTY behavior

Non-interactive stdin without `--yes` bails with:
```
refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively
```

Mirrors `confirm_sensitive_mounts` non-TTY handling at `src/workspace.rs:202`.

### `jackin workspace create` — non-interactive

1. Build the initial `WorkspaceConfig`.
2. Call `plan_collapse` with all indexes marked new.
3. Outcomes:
   - **`CollapseError`** → print error, exit non-zero. No config written.
   - **`Ok(plan)` with no removals** → proceed.
   - **`Ok(plan)` with removals** → print one-line stderr summary, proceed:
     ```
     collapsed 2 redundant mounts under ~/Projects/proj-alpha: ~/Projects/proj-alpha/sub-a, ~/Projects/proj-alpha/sub-b
     ```

No prompt. The operator is creating fresh state and has no prior investment to protect.

### `jackin workspace prune <name>` — new subcommand

Computes the collapse plan on the workspace's current mounts. Shows the same interactive prompt as `workspace edit`. Applies or aborts. `--yes` skips the prompt. Thin wrapper over `plan_collapse` + `edit_workspace` — small amount of CLI glue.

## Touch points

| File | Change |
|------|--------|
| `src/workspace.rs` | Add `CollapsePlan`, `Removal`, `CollapseError`, `plan_collapse`, `covers` + unit tests. |
| `src/config.rs` | `create_workspace` and `edit_workspace` gain post-condition `plan_collapse` checks. |
| `src/cli.rs` | `workspace edit` subcommand: `--yes`, `--prune` flags; pre-write `plan_collapse` + prompt. `workspace create` subcommand: pre-write `plan_collapse` + stderr summary. New `workspace prune <name>` subcommand. |
| `tests/workspace_mount_collapse.rs` *(new)* | CLI integration tests covering prompt, `--yes`, `--prune`, non-TTY, prune subcommand. |
| `docs/src/content/docs/commands/workspace.mdx` | Document new flags and `prune` subcommand. |
| `docs/src/content/docs/guides/mounts.mdx` | Describe the rule-C invariant for stored workspaces. |

Not touched: `resolve_load_workspace`, `MountConfig` struct, config file format.

## Testing strategy

### Unit tests — `src/workspace.rs` (in-module `#[cfg(test)] mod tests`)

**`covers` predicate:**
- `covers_is_false_for_equal_mounts`
- `covers_is_true_for_exact_ancestor_with_matching_suffix`
- `covers_is_false_when_src_nests_but_dst_offsets_differ`
- `covers_is_false_when_src_does_not_nest`
- `covers_normalizes_trailing_slashes`
- `covers_normalizes_dotdot_in_input`
- `covers_handles_different_readonly_flags` — flags don't affect the predicate; readonly is checked at `plan_collapse` level

**`plan_collapse`:**
- `plan_collapse_empty_input_returns_empty_plan`
- `plan_collapse_preserves_unrelated_mounts`
- `plan_collapse_collapses_single_child_under_new_parent`
- `plan_collapse_collapses_multiple_children_under_new_parent`
- `plan_collapse_handles_transitive_chain`
- `plan_collapse_errors_on_readonly_mismatch_rw_parent_ro_child`
- `plan_collapse_errors_on_readonly_mismatch_ro_parent_rw_child`
- `plan_collapse_errors_on_new_child_under_existing_parent`
- `plan_collapse_flags_pre_existing_violation_as_removal_when_nothing_new`

**Property-style (in same module):**
- `plan_collapse_is_idempotent` — plan on `plan.kept` has zero removals and zero errors
- `plan_collapse_result_satisfies_invariant` — no pair in `kept` covers another pair in `kept`

### Unit tests — `src/config.rs` (in-module `#[cfg(test)] mod tests`)

- `edit_workspace_rejects_upsert_that_introduces_child_under_existing_parent`
- `edit_workspace_rejects_upsert_with_readonly_mismatch_vs_existing_parent`
- `edit_workspace_rejects_pre_existing_violation_without_prune`
- `edit_workspace_accepts_pre_existing_violation_when_pre_collapsed_by_caller` — simulates what CLI does with `--prune`
- `create_workspace_errors_on_readonly_mismatch_in_initial_mounts`
- `create_workspace_errors_on_child_under_parent_in_initial_mounts`
- `create_workspace_accepts_already_collapsed_mount_set`
- `post_condition_catches_library_misuse` — hand-construct a `WorkspaceEdit` that would leave redundants and confirm bail

### Integration tests — `tests/workspace_mount_collapse.rs` (new file)

- `workspace_edit_prompts_before_collapsing_children` — `--yes` bypass; assert resulting config has collapsed set
- `workspace_edit_fails_on_readonly_mismatch_with_clear_error` — assert error message structure
- `workspace_edit_fails_on_pre_existing_violation_without_prune`
- `workspace_edit_with_prune_cleans_pre_existing_violations`
- `workspace_edit_non_tty_without_yes_bails`
- `workspace_create_auto_collapses_and_prints_summary` — assert stderr line
- `workspace_prune_removes_redundants`
- `workspace_prune_on_clean_workspace_is_noop`

### Fixture convention

All test fixtures use generic names: `/tmp/proj-alpha`, `/tmp/proj-alpha/sub-a`, `/tmp/proj-beta`. No organization or real-project names anywhere. Same convention carries into the PR description and commit messages.

## Open questions

None at spec time. Any ambiguity is flagged in the relevant section and resolved there.

## Related

- Deferred: config versioning & migrations — see operator's separate brainstorming session.
- Adjacent pattern: sensitive-mount warnings (`docs/src/content/docs/reference/roadmap/sensitive-mount-warnings.mdx`) — reuses `dialoguer::Confirm` + non-TTY bail pattern established by `confirm_sensitive_mounts`.
