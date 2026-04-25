# Per-Mount Isolation — Implementation Spec

**Date**: 2026-04-25
**Roadmap source**: `docs/src/content/docs/reference/roadmap/per-mount-isolation.mdx`
**Status**: Approved — ready for plan
**PR strategy**: One spec, one PR (full V1 in a single change)
**TDD discipline**: Strict (failing test first per behavior)
**TUI scope**: Full integration (read-only display + interactive entry + source-drift modal)

This spec converts the roadmap proposal into an executable design. It pins
module boundaries, data shapes, validation rules, lifecycle behavior, and the
test list. Anywhere the roadmap left an implementation choice open, this spec
chooses one and explains why.

## Goal

Allow a workspace mount to declare an `isolation` mode so parallel agents
working against the same host project don't share a working tree. V1 ships
`shared` (current behavior) and `worktree` (per-agent `git worktree`); `clone`
is reserved in the enum vocabulary but rejected before persistence.

## Non-goals (V1)

- `isolation = "clone"` materialization — parses through `FromStr`, rejected at
  the CLI/TUI apply layer with a "planned but not implemented yet" message.
- `--no-hooks` flag for skipping host hooks per worktree.
- `jackin diff` / `jackin integrate` / merge-assist commands. Back-merge is
  upstream (GitHub).
- Configurable `branch_template` / `base_branch`.
- Isolation on ad-hoc `jackin load --mount` mounts.
- Isolation on global mounts (`[[mounts]]`).
- Submodule / git-LFS special handling.

## Permanently rejected (not deferred)

- **Parent-child isolated pair within one workspace.** Two `isolation = "worktree"`
  mounts whose `dst` paths nest (one strict ancestor of the other) have no safe
  on-disk layout — the inner worktree's `.git` would land inside the outer
  worktree's tree. Validation rejects this at parse and at CLI/TUI apply time.
  Sibling isolated mounts and isolated-parent-with-shared-child remain allowed.

## Module layout

New top-level module `src/isolation/`:

```
src/isolation/
  mod.rs          — MountIsolation enum, parsing, public re-exports
  branch.rs       — branch_name(selector, disambiguation_suffix?)
  materialize.rs  — MaterializedWorkspace, materialize_workspace(), git shell-outs
  state.rs        — IsolationRecord, isolation.json IO, source-drift detection
  finalize.rs     — finalize_foreground_session()
  cleanup.rs      — safe/unsafe/force cleanup helpers
```

Tests live in `#[cfg(test)] mod tests` blocks at the bottom of each file.
Git shell-outs go through the existing `CommandRunner` trait so `FakeRunner`
in `runtime/test_support.rs` can script them.

## Data model

### `MountIsolation` (in `isolation/mod.rs`)

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountIsolation {
    #[default]
    Shared,
    Worktree,
    Clone,
}

impl MountIsolation {
    pub fn is_shared(&self) -> bool { matches!(self, Self::Shared) }
}
```

`FromStr` accepts canonical lowercase only. No alias for `share`. `clone` parses
through `FromStr` (so the type round-trips) but the CLI/TUI apply layer rejects
it before persistence with the "planned but not implemented yet" error.

### `MountConfig` evolution (in `src/workspace/mod.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default, skip_serializing_if = "MountIsolation::is_shared")]
    pub isolation: MountIsolation,
}
```

`skip_serializing_if = is_shared` keeps existing TOMLs identical on round-trip.

Global mount config (`config::mounts::GlobalMountConfig`) rejects `isolation`
at parse time. Implementation: add `#[serde(deny_unknown_fields)]` to the
global mount struct (or use a strict mirror struct that doesn't carry the
field). Implementation choice deferred to plan-writing time; either approach
satisfies the rejection requirement.

### `IsolationRecord` and `isolation.json` (in `isolation/state.rs`)

Path: `<data_dir>/jackin-<container>/.jackin/isolation.json`. Format:

```json
{
  "version": 1,
  "records": [
    {
      "workspace": "jackin",
      "mount_dst": "/workspace/jackin",
      "original_src": "/Users/.../projects/jackin",
      "isolation": "worktree",
      "worktree_path": "/Users/.../.jackin/data/jackin-the-architect/isolated/workspace/jackin",
      "scratch_branch": "jackin/scratch/the-architect",
      "base_commit": "deadbeef...",
      "selector_key": "the-architect",
      "container_name": "jackin-the-architect",
      "cleanup_status": "active"
    }
  ]
}
```

`cleanup_status ∈ { active, preserved_dirty, preserved_unpushed }`. Successful
cleanup removes the record entirely; we don't keep a `cleaned` historical
state.

`version: 1` envelope leaves room to evolve the schema without forking the
file. Atomic write via temp file + rename.

### `MaterializedWorkspace` (in `isolation/materialize.rs`)

```rust
pub struct MaterializedWorkspace {
    pub workdir: String,
    pub mounts: Vec<MaterializedMount>,
}

pub struct MaterializedMount {
    pub bind_src: String,
    pub dst: String,
    pub readonly: bool,
    pub isolation: MountIsolation,
}
```

Third shape (`WorkspaceConfig` → `ResolvedWorkspace` → `MaterializedWorkspace`).
Runtime-only handoff into Docker launch; not round-trippable to TOML.

## CLI surface

### `workspace create` / `workspace edit` — `--mount-isolation`

```rust
/// Set isolation mode for a mount destination. Repeatable.
/// Format: <container-dst>=<shared|worktree|clone>
#[arg(long = "mount-isolation", value_name = "DST=TYPE", action = ArgAction::Append)]
pub mount_isolation: Vec<String>,
```

Custom `value_parser` splits on the first `=`, calls `MountIsolation::from_str`
on the value, and rejects `clone` here with the canonical "planned but not
implemented yet" error. Bad input fails before any plan logic runs.

`WorkspaceEdit` gains `mount_isolation_overrides: Vec<(String, MountIsolation)>`.
`plan_create` / `plan_edit` apply overrides by `dst` after mount upserts;
unknown destination is a hard error referencing the final mount plan.

### `workspace show` — Isolation column

Adds an `Isolation` column. Renders the canonical lowercase name for every row;
shared mounts show `shared` (not blank — exact copy-paste with TOML and CLI
input must match).

### `jackin load --force`

`LoadArgs` gains a `--force` boolean. Materialization's dirty-host check
consults this and `is_interactive()` before refusing.

### `jackin cd <container> [dst]`

New file `src/cli/cd.rs`. Resolves `<container>` via existing context helpers,
reads `isolation.json`:

- `dst` provided → exact-match against `mount_dst`; not-found is a clear error.
- `dst` omitted, exactly one record → use it.
- `dst` omitted, multiple records, TTY → `prompt_choice` picker.
- `dst` omitted, multiple records, non-TTY → fail with candidate list.
- Zero records → "no isolated mounts for container" error.

Spawn child shell from `$SHELL` (fallback `/bin/sh`) with `current_dir =
worktree_path`, env vars `JACKIN_CONTAINER`, `JACKIN_MOUNT_DST`,
`JACKIN_ORIGINAL_SRC`, `JACKIN_WORKTREE`. Wait for exit, propagate exit code.
Does not modify the parent shell.

## TUI surface (`src/console/manager/`)

### Read-only display

- `console/manager/render/editor.rs::render_editor_mount_row` adds an
  isolation badge per mount row. `worktree` rendered on the brand accent;
  `shared` rendered as literal `shared` text in the dimmed-row style
  (matches the canonical-spelling rule applied to `workspace show`; the
  string operators see in the TUI matches what they'd type in TOML).
- `console/preview.rs` mount lines append the same badge for every mount
  (including `shared`, for the same canonical-spelling reason).

### Interactive entry — `I` hotkey

Mirrors the existing `R` (readonly toggle) hotkey:
- `I` cycles the highlighted mount through `Shared → Worktree → Shared`
  (skipping `Clone` — it's reserved-but-rejected).
- New helper `cycle_isolation_for_selected_mount()` in `manager/state.rs`.
- No new widget for V1 entry; the cycle-via-hotkey matches `R` exactly.
- `manager/mount_info.rs` create/edit flows pick up `isolation` from
  `pending_mount_isolation` (default `Shared`).

### Source-drift confirmation modal

When `manager/input/save.rs` detects an edit changing `src` for a mount whose
`dst` has preserved isolation state:
- Reuses `widgets/confirm.rs` patterns.
- Three actions: `Delete preserved state and save`, `Cancel`, `Open mount details`.
- If a related container is running → show error popup ("eject first") instead.

### State

`manager/state.rs::CreateState` and the edit equivalent gain
`pending_mount_isolation: MountIsolation`. `final_mounts` materialization
passes it through to `MountConfig`.

## Materialization runtime

### Hook point

`src/runtime/launch.rs::load_agent_with` gains step **4a** between agent-state
prep and docker-launch arg construction:

```
1. Resolve workspace mounts        → ResolvedWorkspace
2. Claim final container name      → naming::claim_container_name
3. Prepare AgentState              → instance::AgentState::prepare
4a. materialize_workspace(&resolved, &agent_state, runner) → MaterializedWorkspace
4. Build docker run args from MaterializedWorkspace.mounts
5. Launch container
```

`materialize_workspace` is the only entry point. Iterates `resolved.mounts`,
passes through `Shared` mounts (`bind_src = src`), runs the per-mount
materialization pipeline for `Worktree`.

### Per-mount worktree materialization (idempotent)

```
for each isolated mount:
  1. worktree_path = <data_dir>/jackin-<container>/isolated/<dst-trim-leading-slash>
  2. scratch_branch = branch_name(selector, maybe_disambiguation_suffix)
  3. Read existing IsolationRecord for (container, dst)
       drift guard: original_src != current src → hard error
       reuse: record exists AND worktree_path is a live worktree → return
  4. preflight_worktree(mount, ctx, runner) — see Validation
  5. ensure_worktree_config_enabled(host_repo, runner) — see below
  6. base_commit = git -C src rev-parse HEAD
  7. git -C src worktree add -b <scratch_branch> <worktree_path> <base_commit>
  8. write IsolationRecord (cleanup_status = "active")
  9. return MaterializedMount { bind_src: worktree_path, dst, readonly, isolation }
```

### Mount-destination on-disk path

Verbatim `dst` with leading and trailing `/` stripped:
- dst `/workspace/jackin` → `isolated/workspace/jackin/`
- dst `/workspace/docs` → `isolated/workspace/docs/`

Implemented as a one-liner inside `materialize.rs::worktree_path_for(...)`.
No separate `slug.rs` module.

### Branch naming (`isolation/branch.rs`)

```
pub fn branch_name(selector: &str, suffix: Option<&str>) -> String
```

- Default: `jackin/scratch/<selector>` with selector namespace `/` preserved
  (e.g. `jackin/scratch/chainargos/the-architect`).
- Clone instance: `-clone-N` appended to final segment
  (`jackin/scratch/the-architect-clone-1`,
  `jackin/scratch/chainargos/the-architect-clone-1`).
- Disambiguation: only when there are multiple isolated mounts in the same
  container targeting the same host repo — append the dst-flattened-with-dashes
  to the final segment:
  - `jackin/scratch/the-architect-workspace-jackin`
  - `jackin/scratch/the-architect-workspace-jackin-v2`

Disambiguation decision lives in `materialize.rs` (it knows the count of
isolated mounts targeting each host repo); `branch.rs` just renders the
final string.

### `ensure_worktree_config_enabled`

```
- git -C <repo> config --get extensions.worktreeConfig
    if "true" → return Ok(false)  (not newly enabled)
- git -C <repo> config --get core.repositoryformatversion
    if "0" → git -C <repo> config core.repositoryformatversion 1
- git -C <repo> config extensions.worktreeConfig true
- print one-line notice
- return Ok(true)
```

One-shot per host repo per host. No per-load tracking — once enabled, future
loads skip the call and skip the notice.

### Docker bind-mount ordering

`runtime/launch.rs` sorts `MaterializedWorkspace.mounts` by `dst.len()`
ascending before emitting `--mount` flags. Parent paths are strict prefixes
of children, so length-ascending is sufficient and avoids edge cases with
lexicographic sort. Docker overlays later mounts on earlier ones, so shared
cache children land inside the isolated worktree as expected.

## Validation

Layout validation (in `workspace/mod.rs::validate_isolation_layout`, called
from `validate_workspace_config` and CLI/TUI apply paths):

| Check | Outcome |
|---|---|
| No two `Worktree` mounts where one's `dst` is a strict ancestor of the other's | Hard error naming both dsts |

Per-mount preflight (in `isolation/materialize.rs::preflight_worktree`):

| Check | Notes |
|---|---|
| `readonly == false` | Hard error: "isolated mount /workspace/jackin cannot be readonly" |
| Sensitive mount (`find_sensitive_mounts`) | Hard error citing the sensitive path |
| `src` is a git repo root | `git -C <src> rev-parse --show-toplevel` must equal `src`; rejects subdir-of-repo |
| Host not mid-rebase / merge / cherry-pick | Stat `<src>/.git/rebase-merge`, `rebase-apply`, `MERGE_HEAD`, `CHERRY_PICK_HEAD` |
| Host tree clean OR `--force` OR interactive ack | `git -C <src> status --porcelain`; ignored files don't count |

Errors include the mount destination AND the declared isolation level.

Obviously-broken cases (not a repo, unborn branch, bare repo) fall through to
git's own error messages — jackin' doesn't re-wrap.

## Lifecycle

### Shared foreground finalizer (`isolation/finalize.rs`)

`finalize_foreground_session(ctx, attach_outcome)` — called by
`runtime/launch.rs` after `load`/`launch` attach returns AND by
`runtime/attach.rs` after `hardline` attach returns.

```
match attach_outcome {
    StillRunning             => Preserved,
    StoppedNonZero | OOMKill => Preserved,
    StoppedClean             => attempt_safe_cleanup()
}
```

### `attempt_safe_cleanup` per record

```
1. status --porcelain on worktree → non-empty → preserved_dirty, mark, return
2. HEAD vs base_commit:
     equal → safe to delete
3. for-each-ref upstream:
     has upstream AND all local commits reachable from upstream → safe to delete
     has upstream AND local commits not reachable → preserved_unpushed
     no upstream AND HEAD != base → preserved_unpushed
4. safe path: git worktree remove --force, git branch -D, remove record
```

### Unsafe clean-exit interactive prompt

If safe cleanup yielded `preserved_*` AND stdin is TTY:

```
Isolated worktree for jackin-the-architect still has uncommitted changes:
  /.../.jackin/data/jackin-the-architect/isolated/workspace/jackin

What do you want to do?
  1. Return to agent to address it
  2. Preserve worktree and exit
  3. Force delete worktree and discard changes
```

- **Return to agent**: restart container, re-attach, retry safe cleanup once
  (don't loop indefinitely; if still dirty, fall through to prompt again).
- **Preserve**: leave record + worktree as-is.
- **Force delete**: invoke `cleanup::force_cleanup_isolated`.

Non-TTY: print warning shape from roadmap, return.

### `purge` (`runtime/cleanup.rs::purge_agent`)

```
1. Refuse if container is running (existing list_agent_names)
2. Stop DinD sidecar (existing)
3. Read isolation.json
4. For each record (best-effort, log on failure, do not abort):
     git -C <original_src> worktree remove --force <worktree_path>
     git -C <original_src> branch -D <scratch_branch>
     rm -rf <worktree_path>
5. Remove container (existing)
6. rm -rf <data_dir>/jackin-<container>/
```

The running-agent guard at step 1 also closes a pre-existing `shared`-mode gap
where `purge` could delete state from under a live container.

### Source-drift detection

**Runtime (`materialize.rs`):** at the start of per-mount materialization,
check `IsolationRecord.original_src != current src` → hard error citing
container, dst, old src, new src, preserved worktree path, and the three
recovery commands.

**Workspace edit (`config/workspaces.rs::apply_edit`):** before TOML save:
1. Walk `state::list_records_for_workspace(workspace_name)`.
2. For each upserted/changed mount, compare new `src` against any matching
   record's `original_src`:
   - Related container running → reject ("eject first").
   - Interactive (TTY/TUI) → confirm modal/prompt.
   - Non-interactive CLI → require `--delete-isolated-state` flag; without
     it, hard error citing affected containers.

`workspace remove` does **not** scan or prompt — it only deletes config.
Lifecycle stays with `hardline` / `cd` / `purge`.

### Eject

Unchanged. Stops the container, leaves `isolated/`, `isolation.json`, host
worktree, and scratch branch in place.

### Mount-mode flip + missing-src tolerance

- `Worktree → Shared` in TOML before next `load`: orphan `isolated/<dst>/`
  tree stays on disk; only `purge` reclaims it.
- Missing host `src`: `purge` tolerates (best-effort + `rm -rf`); `load`
  errors clearly per existing `validate_mount_paths`.

## Testing strategy

Strict TDD per behavior. Test list:

### `isolation/mod.rs`

- Parses `shared` / `worktree` / `clone` to canonical variants
- Rejects `share` (no alias)
- Rejects unknown spellings with clear error
- `Default` returns `Shared`
- Serde round-trip: absent field → `Shared`; `Shared` omitted on re-serialize
- `clone` parses but CLI/TUI apply layer rejects with canonical message
- Global mount config rejects `isolation` field at parse

### `workspace/mod.rs::validate_isolation_layout`

- Allows: 1 worktree + N shared
- Allows: sibling worktree mounts (different repos)
- Allows: sibling worktree mounts (same repo)
- Allows: isolated parent + shared child cache
- Rejects: nested worktree mounts (parent/child and grandparent/grandchild)
- Error message names both offending dsts

### `isolation/branch.rs::branch_name`

- Single isolated mount → `jackin/scratch/<selector>`
- Namespaced selector preserves `/`
- Clone instance → suffix on final segment with namespace
- Two isolated mounts targeting same repo → both get dst-flattened suffix
- Two isolated mounts targeting different repos → no suffix on either

### `isolation/state.rs`

- Empty file → empty record set, no error
- Round-trip preserves all fields and `version: 1` envelope
- `read_record(container, dst)` returns `None` when missing
- `write_record` is atomic
- `list_records_for_workspace` walks all container state dirs
- Source-drift detect returns drift error with both paths

### `isolation/materialize.rs` — preflight (parameterized)

- Sensitive mount → reject
- `readonly = true` → reject
- `src` not a repo → reject
- `src` is repo subdir → reject explicitly
- Mid-rebase / merge / cherry-pick → reject (one test per indicator)
- Dirty tree, non-interactive, no `--force` → reject
- Dirty tree, non-interactive, `--force` → proceed
- Dirty tree, interactive ack → proceed
- Dirty tree with only ignored files → proceed

### `isolation/materialize.rs` — happy path & idempotency

- First materialization: enables `extensions.worktreeConfig`, runs
  `git worktree add`, writes record
- Already-enabled `extensions.worktreeConfig` → skips enable, no notice
- Second materialization (record + worktree present) → no git ops, returns
  existing materialized mount
- Source drift on re-materialization → reject before any git ops
- `core.repositoryformatversion = 0` → bumped to 1 on first enable
- Multiple shared mounts under one isolated parent: bind-mount order
  parent-before-child (length-ascending sort)

### `isolation/finalize.rs::finalize_foreground_session`

- Container still running → all records preserved unchanged
- Container stopped non-zero → all records preserved
- Stopped clean, worktree clean, HEAD == base → record removed, branch deleted
- Stopped clean, worktree clean, HEAD != base, all reachable from upstream
  → record removed, branch deleted
- Stopped clean, worktree clean, HEAD != base, no upstream → `preserved_unpushed`
- Stopped clean, worktree dirty → `preserved_dirty`
- Multiple isolated mounts, mixed outcomes → each handled independently

### `isolation/finalize.rs` — interactive prompt

- Return to agent: re-attach called, safe cleanup retried once, no recursion
- Preserve: worktree + record left as-is
- Force delete: invokes `cleanup::force_cleanup_isolated`
- Non-TTY: prints warning shape verbatim, no prompt

### `isolation/cleanup.rs::force_cleanup_isolated`

- Removes worktree, deletes branch, removes record
- Tolerates missing host repo (best-effort)
- Tolerates already-removed worktree (idempotent)

### `runtime/cleanup.rs::purge_agent`

- Refuses on running container with clear error (closes shared-mode gap too)
- Removes all isolated worktrees and scratch branches recorded
- Tolerates missing host repo
- No prompts, no flags

### `cli/workspace.rs`

- `--mount-isolation /workspace/jackin=worktree` parses correctly
- `--mount-isolation` referencing unknown destination → hard error
- `--mount-isolation /workspace/jackin=clone` → hard error
- `workspace show` displays Isolation column with canonical names
- `workspace edit` rejects isolation change on running container
- `workspace edit` source-drift: TTY → confirm modal; non-TTY → require
  `--delete-isolated-state`

### `cli/cd.rs`

- Single isolated mount, no dst → opens shell at worktree path
- Multiple isolated mounts, dst provided, exact match → opens shell
- Multiple isolated mounts, dst missing on TTY → picker
- Multiple isolated mounts, dst missing non-TTY → fail with candidates
- No isolated mounts → clear error
- Sets `JACKIN_*` env vars

### `console/manager/`

- Mount row renders isolation badge for `worktree`
- Hotkey `I` cycles highlighted mount's isolation
- Source-drift confirm modal renders when edit detects drift
- Save flow blocks on running container with "eject first"
- `final_mounts` carries `isolation` from `pending_mount_isolation`

### Test infrastructure

`FakeRunner` already supports scripted `(prog, args) → output` triples. May
need a small extension `script_err(prog, args, exit_code, stderr)` for
git-failure tests if not already present in `runtime/test_support.rs`.

`tempfile::TempDir` per test for `isolation.json` filesystem tests.

Existing `prompt_choice` test seam in `tui/prompt.rs` covers interactive
finalizer tests.

## Docs updates (same PR)

| File | Change |
|---|---|
| `docs/src/content/docs/guides/workspaces.mdx` | New "Per-mount isolation" section |
| `docs/src/content/docs/guides/mounts.mdx` | New `isolation` field in mount syntax reference; isolated source + shared cache child pattern |
| `docs/src/content/docs/reference/configuration.mdx` | TOML schema: `MountConfig.isolation` field, enum vocabulary, default |
| `docs/src/content/docs/reference/architecture.mdx` | Materialization flow: `WorkspaceConfig` → `ResolvedWorkspace` → `MaterializedWorkspace`, `isolation.json`, foreground finalizer |
| `docs/src/content/docs/commands/workspace.mdx` | `--mount-isolation` flag for create/edit; new Isolation column in `workspace show` |
| `docs/src/content/docs/commands/load.mdx` | New `--force` flag |
| `docs/src/content/docs/commands/purge.mdx` | New running-agent guard; isolated cleanup behavior |
| `docs/src/content/docs/commands/cd.mdx` | New page; sidebar entry in `astro.config.ts` |
| `docs/src/content/docs/reference/roadmap/per-mount-isolation.mdx` | Update V1 line about "duplicate isolated mounts allowed" → narrow to "siblings allowed; parent-child rejected"; mark item Implemented |

## Affected source files

| File | Change kind |
|---|---|
| `src/workspace/mod.rs` | Add `isolation` field to `MountConfig`; new `validate_isolation_layout` helper |
| `src/workspace/mounts.rs` | Pass-through (no parser change for the field; serde handles it) |
| `src/workspace/resolve.rs` | No structural change; `ResolvedWorkspace` carries `MountConfig` with new field |
| `src/config/mod.rs` | Serialization passthrough |
| `src/config/mounts.rs` | Reject `isolation` on global mounts |
| `src/config/workspaces.rs` | Apply `mount_isolation_overrides`; source-drift detection in apply path; `workspace show` Isolation column |
| `src/cli/workspace.rs` | `--mount-isolation` flag; `value_parser` for `DST=TYPE` |
| `src/cli/agent.rs` | `--force` on `LoadArgs` |
| `src/cli/cleanup.rs` | Error messaging when target is running |
| `src/cli/cd.rs` | New file; `CdArgs` + dispatch |
| `src/cli/root.rs` and `src/cli/dispatch.rs` | Wire `Cd` variant |
| `src/app/mod.rs` | `Command::Cd` dispatch; running-agent guard for `Purge` |
| `src/runtime/launch.rs` | Insert step 4a calling `materialize_workspace`; call shared finalizer after attach |
| `src/runtime/attach.rs` | Call shared finalizer after attach (`hardline`) |
| `src/runtime/cleanup.rs` | `purge_agent` reads `isolation.json` and removes worktrees/branches |
| `src/runtime/test_support.rs` | Possibly add `script_err` to `FakeRunner` |
| `src/console/manager/state.rs` | `pending_mount_isolation` field; `cycle_isolation_for_selected_mount()` |
| `src/console/manager/mount_info.rs` | Pass `isolation` through create/edit flows |
| `src/console/manager/render/editor.rs` | Render isolation badge in mount rows |
| `src/console/manager/input/save.rs` | Source-drift confirm modal trigger |
| `src/console/preview.rs` | Append isolation badge for non-shared mounts |
| `src/isolation/mod.rs` | New |
| `src/isolation/branch.rs` | New |
| `src/isolation/materialize.rs` | New |
| `src/isolation/state.rs` | New |
| `src/isolation/finalize.rs` | New |
| `src/isolation/cleanup.rs` | New |
| `src/lib.rs` | Add `pub mod isolation;` |
| `PROJECT_STRUCTURE.md` | Add `src/isolation/` to module tree; add `cli/cd.rs`; mention new finalizer |

## Implementation checks

1. `hardline` offline lockdown: `worktree` mode is local-only (`git worktree
   add` doesn't touch network). Verify the shared finalizer never requires
   network access. Materialization re-uses upstream tracking-branch info from
   the existing host `.git/`, no fetch.

## Open implementation choices to settle in plan

These are intentionally not pinned in the spec; the plan can choose either way:

- Whether global-mount `isolation` rejection uses `deny_unknown_fields` on the
  global mount struct or a separate strict mirror struct.
- Whether `FakeRunner` gains a new method or the existing API can express
  scripted git-failure outcomes.
