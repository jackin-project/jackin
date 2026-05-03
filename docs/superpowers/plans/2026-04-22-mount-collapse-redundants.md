# Mount Collapse — Redundant Descendant Removal: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-remove rule-C-covered descendant mounts on workspace write paths (`create`, `edit`), with conflict errors for readonly mismatch / child-under-existing-parent, an interactive prompt for edits, and a `workspace prune` subcommand for pre-existing violations.

**Architecture:** Pure `plan_collapse` function in `workspace.rs` (no I/O, no prompts). Library write paths (`edit_workspace`, `create_workspace`) enforce the rule-C invariant as a post-condition. CLI layer pre-plans, prompts or prints, then passes the already-collapsed mount set to the library. Runtime mount merging (`resolve_load_workspace`) stays literal.

**Tech Stack:** Rust 2024 (MSRV 1.94), `anyhow` for errors, `dialoguer` for prompts, `assert_cmd` + `predicates` for integration tests.

**Spec reference:** `docs/superpowers/specs/2026-04-22-mount-collapse-redundants-design.md`.

**Branch:** `feature/mount-collapse-redundants` (already created from `main`; spec commit `95c0737c`).

**Pre-commit checks for every code commit** (per `COMMITS.md`):
```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```
All three must pass with zero warnings and zero failures before `git commit -s`. Docs-only commits may skip.

**Commit conventions:** Conventional Commits + DCO (`git commit -s`). Sign-off trailer must match `git config user.email`.

**Fixture convention:** Generic path names only — `/tmp/proj-alpha`, `~/Projects/proj-alpha/sub-a`, etc. Never use real project or organization names in tests, commit messages, or PR descriptions.

---

## File Structure

| File | Role | Action |
|------|------|--------|
| `src/workspace.rs` | Rule-C predicate, collapse planner, pure types | **modify** — add `covers`, `CollapsePlan`, `Removal`, `CollapseError`, `plan_collapse`, tests |
| `src/config.rs` | Library write paths for `WorkspaceConfig` | **modify** — add post-condition checks in `edit_workspace` and `create_workspace`, tests |
| `src/cli.rs` | Clap command definitions | **modify** — add `--yes`, `--prune` flags to `WorkspaceCommand::Edit`; add `WorkspaceCommand::Prune` variant; tests |
| `src/lib.rs` | CLI command handlers | **modify** — pre-collapse + prompt logic in `WorkspaceCommand::Edit`; pre-collapse + stderr summary in `WorkspaceCommand::Create`; handler for `WorkspaceCommand::Prune` |
| `tests/workspace_mount_collapse.rs` | CLI integration tests | **create** — exercises prompt, `--yes`, `--prune`, non-TTY, prune subcommand |
| `docs/src/content/docs/commands/workspace.mdx` | Published command docs | **modify** — document `--yes`, `--prune`, `workspace prune` |
| `docs/src/content/docs/guides/mounts.mdx` | Published mount docs | **modify** — describe rule-C invariant for stored workspaces |

Not touched: `src/workspace.rs::resolve_load_workspace` (runtime merging stays literal), `MountConfig` struct, config file format, CLI tests unrelated to workspace.

---

## Task 1: `covers` predicate

**Files:**
- Modify: `src/workspace.rs` (add near existing mount helpers, around line 150)
- Test: `src/workspace.rs` (tests module at the bottom, starting around line 432)

- [ ] **Step 1.1: Write failing unit tests for `covers`**

Add these tests to the existing `#[cfg(test)] mod tests` block in `src/workspace.rs` (append after the last existing test). They reference a `covers` function that doesn't exist yet.

```rust
    #[test]
    fn covers_is_false_for_equal_mounts() {
        let a = MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false };
        let b = a.clone();
        assert!(!covers(&a, &b));
    }

    #[test]
    fn covers_is_true_for_exact_ancestor_with_matching_suffix() {
        let parent = MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false };
        let child = MountConfig { src: "/a/b".into(), dst: "/a/b".into(), readonly: false };
        assert!(covers(&parent, &child));
    }

    #[test]
    fn covers_is_true_for_deep_ancestor_with_matching_suffix() {
        let parent = MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false };
        let child = MountConfig { src: "/a/b/c/d".into(), dst: "/a/b/c/d".into(), readonly: false };
        assert!(covers(&parent, &child));
    }

    #[test]
    fn covers_is_true_when_src_and_dst_differ_but_offsets_match() {
        let parent = MountConfig { src: "/host/root".into(), dst: "/container/root".into(), readonly: false };
        let child = MountConfig { src: "/host/root/sub".into(), dst: "/container/root/sub".into(), readonly: false };
        assert!(covers(&parent, &child));
    }

    #[test]
    fn covers_is_false_when_src_nests_but_dst_offsets_differ() {
        let parent = MountConfig { src: "/host/root".into(), dst: "/container/a".into(), readonly: false };
        let child = MountConfig { src: "/host/root/sub".into(), dst: "/container/b/sub".into(), readonly: false };
        assert!(!covers(&parent, &child));
    }

    #[test]
    fn covers_is_false_when_src_does_not_nest() {
        let a = MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false };
        let b = MountConfig { src: "/b".into(), dst: "/b".into(), readonly: false };
        assert!(!covers(&a, &b));
    }

    #[test]
    fn covers_is_false_for_sibling_prefix_match() {
        // `/a-x` is not a child of `/a`, even though "/a-x".starts_with("/a").
        let parent = MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false };
        let child = MountConfig { src: "/a-x".into(), dst: "/a-x".into(), readonly: false };
        assert!(!covers(&parent, &child));
    }

    #[test]
    fn covers_normalizes_trailing_slashes() {
        let parent = MountConfig { src: "/a/".into(), dst: "/a/".into(), readonly: false };
        let child = MountConfig { src: "/a/b".into(), dst: "/a/b".into(), readonly: false };
        assert!(covers(&parent, &child));
    }

    #[test]
    fn covers_handles_different_readonly_flags() {
        // `covers` is purely path-based. Readonly mismatches are caught by plan_collapse.
        let parent = MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false };
        let child = MountConfig { src: "/a/b".into(), dst: "/a/b".into(), readonly: true };
        assert!(covers(&parent, &child));
    }
```

- [ ] **Step 1.2: Run the tests to confirm they fail**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
cargo nextest run --lib workspace::tests::covers
```

Expected: compilation error — `cannot find function 'covers' in this scope`.

- [ ] **Step 1.3: Implement `covers` in `src/workspace.rs`**

Add this function right after `validate_mounts` (currently ending around line 150), before the `SENSITIVE_SUFFIXES` section:

```rust
// ── Rule-C covering predicate ───────────────────────────────────────────

/// Returns true iff `parent` strictly covers `child` under rule C:
/// `parent.src` is a proper ancestor of `child.src`, AND the path suffix
/// `child.src - parent.src` equals the path suffix `child.dst - parent.dst`.
///
/// Equivalently: `child` projects the same host subtree to the same container
/// location that `parent` would already expose it at.
///
/// Identity (equal src and equal dst) returns false — that case is handled by
/// upsert-by-dst in `edit_workspace`.
///
/// The `readonly` flag is ignored here. Readonly mismatches are caught at
/// `plan_collapse` level, not in the predicate.
fn covers(parent: &MountConfig, child: &MountConfig) -> bool {
    let parent_src = parent.src.trim_end_matches('/');
    let parent_dst = parent.dst.trim_end_matches('/');
    let child_src = child.src.trim_end_matches('/');
    let child_dst = child.dst.trim_end_matches('/');

    // Identity is not covering.
    if parent_src == child_src && parent_dst == child_dst {
        return false;
    }

    // child.src must be strictly under parent.src.
    let Some(src_suffix) = child_src.strip_prefix(parent_src) else {
        return false;
    };
    if !src_suffix.starts_with('/') {
        return false;
    }

    // child.dst must be strictly under parent.dst with the same suffix.
    let Some(dst_suffix) = child_dst.strip_prefix(parent_dst) else {
        return false;
    };
    src_suffix == dst_suffix
}
```

- [ ] **Step 1.4: Run the tests and confirm they pass**

```bash
cargo nextest run --lib workspace::tests::covers
```

Expected: all 9 `covers_*` tests pass.

- [ ] **Step 1.5: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures. If `fmt --check` fails, run `cargo fmt` and retry.

- [ ] **Step 1.6: Commit**

```bash
git add src/workspace.rs
git commit -s -m "feat(workspace): add covers predicate for mount ancestry

Implements rule C: parent covers child iff parent.src is a strict
ancestor of child.src AND the src-to-child offset equals the
dst-to-child offset. Used by plan_collapse to detect redundant
descendant mounts.

Pure function — no I/O, no side effects. Readonly flags are ignored
here and checked at plan_collapse level."
```

---

## Task 2: Collapse types and `plan_collapse` happy paths

**Files:**
- Modify: `src/workspace.rs`
- Test: `src/workspace.rs` (same tests module)

- [ ] **Step 2.1: Write failing unit tests for types and happy-path planning**

Append to the same `#[cfg(test)] mod tests` block after the `covers_*` tests:

```rust
    fn mk(src: &str, dst: &str, ro: bool) -> MountConfig {
        MountConfig { src: src.into(), dst: dst.into(), readonly: ro }
    }

    #[test]
    fn plan_collapse_empty_input_returns_empty_plan() {
        let plan = plan_collapse(&[], &[]).unwrap();
        assert!(plan.kept.is_empty());
        assert!(plan.removed.is_empty());
    }

    #[test]
    fn plan_collapse_preserves_unrelated_mounts() {
        let mounts = vec![
            mk("/a", "/a", false),
            mk("/b", "/b", false),
        ];
        let plan = plan_collapse(&mounts, &[0, 1]).unwrap();
        assert_eq!(plan.kept, mounts);
        assert!(plan.removed.is_empty());
    }

    #[test]
    fn plan_collapse_collapses_single_child_under_new_parent() {
        let mounts = vec![
            mk("/a/b", "/a/b", false),  // pre-existing child (index 0)
            mk("/a", "/a", false),      // new parent (index 1)
        ];
        let plan = plan_collapse(&mounts, &[1]).unwrap();
        assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
        assert_eq!(plan.removed.len(), 1);
        assert_eq!(plan.removed[0].child, mk("/a/b", "/a/b", false));
        assert_eq!(plan.removed[0].covered_by, mk("/a", "/a", false));
    }

    #[test]
    fn plan_collapse_collapses_multiple_children_under_new_parent() {
        let mounts = vec![
            mk("/a/b", "/a/b", false),
            mk("/a/c", "/a/c", false),
            mk("/a", "/a", false),
        ];
        let plan = plan_collapse(&mounts, &[2]).unwrap();
        assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
        assert_eq!(plan.removed.len(), 2);
    }

    #[test]
    fn plan_collapse_handles_transitive_chain() {
        // A ⊃ B ⊃ C, all present in input; B and C both find A as a parent.
        let mounts = vec![
            mk("/a", "/a", false),
            mk("/a/b", "/a/b", false),
            mk("/a/b/c", "/a/b/c", false),
        ];
        let plan = plan_collapse(&mounts, &[0, 1, 2]).unwrap();
        assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
        assert_eq!(plan.removed.len(), 2);
        // Both B and C are covered by A.
        for rem in &plan.removed {
            assert_eq!(rem.covered_by, mk("/a", "/a", false));
        }
    }

    #[test]
    fn plan_collapse_flags_pre_existing_violation_as_removal_when_nothing_new() {
        // Neither index is in new_indexes — both pre-existing. Library returns
        // Ok with removal; CLI will inspect origin and decide to reject or
        // proceed with --prune.
        let mounts = vec![
            mk("/a", "/a", false),
            mk("/a/b", "/a/b", false),
        ];
        let plan = plan_collapse(&mounts, &[]).unwrap();
        assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
        assert_eq!(plan.removed.len(), 1);
    }
```

- [ ] **Step 2.2: Run the tests to confirm they fail**

```bash
cargo nextest run --lib workspace::tests::plan_collapse
```

Expected: compilation error — `plan_collapse`, `CollapsePlan`, `Removal` not found.

- [ ] **Step 2.3: Add types and happy-path implementation**

Add these public types to `src/workspace.rs`, immediately after the `covers` function (from Task 1):

```rust
/// A proposed mount-set change produced by [`plan_collapse`]. `kept` is the
/// mount list with all rule-C-redundant entries removed; `removed` describes
/// each collapse for operator-facing messaging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapsePlan {
    pub kept: Vec<MountConfig>,
    pub removed: Vec<Removal>,
}

/// Records that `child` was collapsed because it is covered by `covered_by`
/// under rule C (see [`covers`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub child: MountConfig,
    pub covered_by: MountConfig,
}

/// Conditions that prevent a silent collapse. The operator must resolve these
/// by hand before the edit can proceed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CollapseError {
    #[error(
        "mount {parent_src} ({parent_mode}) would subsume {child_src} ({child_mode}), \
         but the readonly flag differs. Match the flag or remove the child first.",
        parent_src = parent.src,
        parent_mode = if parent.readonly { "ro" } else { "rw" },
        child_src = child.src,
        child_mode = if child.readonly { "ro" } else { "rw" },
    )]
    ReadonlyMismatch {
        parent: MountConfig,
        child: MountConfig,
    },
    #[error(
        "mount {child_src} is already covered by existing mount {parent_src}. \
         Nothing to add.",
        child_src = child.src,
        parent_src = parent.src,
    )]
    ChildUnderExistingParent {
        parent: MountConfig,
        child: MountConfig,
    },
}

/// Computes a [`CollapsePlan`] for `mounts`. `new_indexes` identifies which
/// entries in `mounts` originate from the current operation (upserts for
/// `edit`, all indexes for `create`). Indexes outside `mounts.len()` are
/// ignored.
///
/// Returns `Err` on the first [`CollapseError`] encountered. On success, every
/// entry in `kept` is pairwise non-covering (see [`covers`]) — i.e., the
/// returned mount list is rule-C compliant.
pub fn plan_collapse(
    mounts: &[MountConfig],
    new_indexes: &[usize],
) -> Result<CollapsePlan, CollapseError> {
    let mut kept = Vec::new();
    let mut removed = Vec::new();

    for (i, m) in mounts.iter().enumerate() {
        let parent = mounts
            .iter()
            .enumerate()
            .find(|(j, p)| *j != i && covers(p, m))
            .map(|(j, p)| (j, p));

        match parent {
            Some((j, p)) => {
                if p.readonly != m.readonly {
                    return Err(CollapseError::ReadonlyMismatch {
                        parent: p.clone(),
                        child: m.clone(),
                    });
                }
                let child_is_new = new_indexes.contains(&i);
                let parent_is_new = new_indexes.contains(&j);
                if child_is_new && !parent_is_new {
                    return Err(CollapseError::ChildUnderExistingParent {
                        parent: p.clone(),
                        child: m.clone(),
                    });
                }
                removed.push(Removal {
                    child: m.clone(),
                    covered_by: p.clone(),
                });
            }
            None => kept.push(m.clone()),
        }
    }

    Ok(CollapsePlan { kept, removed })
}
```

- [ ] **Step 2.4: Run the happy-path tests and confirm they pass**

```bash
cargo nextest run --lib workspace::tests::plan_collapse
```

Expected: all 6 `plan_collapse_*` tests pass.

- [ ] **Step 2.5: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures.

- [ ] **Step 2.6: Commit**

```bash
git add src/workspace.rs
git commit -s -m "feat(workspace): add plan_collapse for redundant mount detection

Introduces CollapsePlan, Removal, CollapseError types and the
plan_collapse function, the pure core of the mount-collapse feature.
Given a mount list and the indexes of newly-introduced entries, it
produces a rule-C-compliant kept set plus a list of collapses.

Handles the happy paths: empty input, unrelated mounts, single/multi
child collapse under a new parent, transitive chains, and pre-existing
violations (all indexes outside new_indexes)."
```

---

## Task 3: `plan_collapse` error cases

**Files:**
- Modify: `src/workspace.rs` (tests module)

- [ ] **Step 3.1: Write failing unit tests for error cases**

Append to the tests module:

```rust
    #[test]
    fn plan_collapse_errors_on_readonly_mismatch_rw_parent_ro_child() {
        let mounts = vec![
            mk("/a/b", "/a/b", true),   // ro child
            mk("/a", "/a", false),      // rw parent (new)
        ];
        let err = plan_collapse(&mounts, &[1]).unwrap_err();
        match err {
            CollapseError::ReadonlyMismatch { parent, child } => {
                assert_eq!(parent, mk("/a", "/a", false));
                assert_eq!(child, mk("/a/b", "/a/b", true));
            }
            other => panic!("expected ReadonlyMismatch, got {other:?}"),
        }
    }

    #[test]
    fn plan_collapse_errors_on_readonly_mismatch_ro_parent_rw_child() {
        let mounts = vec![
            mk("/a/b", "/a/b", false),  // rw child
            mk("/a", "/a", true),       // ro parent (new)
        ];
        let err = plan_collapse(&mounts, &[1]).unwrap_err();
        assert!(matches!(err, CollapseError::ReadonlyMismatch { .. }));
    }

    #[test]
    fn plan_collapse_errors_on_new_child_under_existing_parent() {
        // Parent at index 0 is pre-existing. Child at index 1 is new.
        let mounts = vec![
            mk("/a", "/a", false),      // existing parent
            mk("/a/b", "/a/b", false),  // new child
        ];
        let err = plan_collapse(&mounts, &[1]).unwrap_err();
        match err {
            CollapseError::ChildUnderExistingParent { parent, child } => {
                assert_eq!(parent, mk("/a", "/a", false));
                assert_eq!(child, mk("/a/b", "/a/b", false));
            }
            other => panic!("expected ChildUnderExistingParent, got {other:?}"),
        }
    }

    #[test]
    fn plan_collapse_allows_new_child_when_new_parent_is_also_in_same_edit() {
        // Both parent and child introduced in the same edit — not a "child
        // under existing parent" case; child is just redundant and gets
        // collapsed normally.
        let mounts = vec![
            mk("/a/b", "/a/b", false),
            mk("/a", "/a", false),
        ];
        let plan = plan_collapse(&mounts, &[0, 1]).unwrap();
        assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
        assert_eq!(plan.removed.len(), 1);
    }

    #[test]
    fn plan_collapse_error_message_mentions_both_paths() {
        let mounts = vec![
            mk("/a/b", "/a/b", true),
            mk("/a", "/a", false),
        ];
        let err = plan_collapse(&mounts, &[1]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("/a"));
        assert!(msg.contains("/a/b"));
        assert!(msg.contains("readonly"));
    }
```

- [ ] **Step 3.2: Run the tests — they should already pass**

The error paths were implemented in Task 2 alongside happy paths; these tests just lock them down.

```bash
cargo nextest run --lib workspace::tests::plan_collapse
```

Expected: all tests pass, including the new ones.

- [ ] **Step 3.3: Check `thiserror` is already in dependencies**

```bash
grep -n thiserror /Users/donbeave/Projects/jackin-project/jackin/Cargo.toml
```

Expected: `thiserror = "2.0"` at line 30 — already present.

- [ ] **Step 3.4: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

- [ ] **Step 3.5: Commit**

```bash
git add src/workspace.rs
git commit -s -m "test(workspace): cover plan_collapse readonly and child-under-parent errors

Adds unit tests locking down the CollapseError branches: ReadonlyMismatch
(both directions) and ChildUnderExistingParent, plus the positive case
where a parent+child introduced in the same edit is allowed to collapse
normally. Also asserts the error Display includes both paths."
```

---

## Task 4: Property-style invariants for `plan_collapse`

**Files:**
- Modify: `src/workspace.rs` (tests module)

- [ ] **Step 4.1: Write invariant tests**

Append to the tests module:

```rust
    /// Exhaustive check: for any plan produced on a valid (no-error) input,
    /// re-planning on `plan.kept` must be a no-op.
    #[test]
    fn plan_collapse_is_idempotent() {
        let inputs: Vec<Vec<MountConfig>> = vec![
            vec![],
            vec![mk("/a", "/a", false)],
            vec![mk("/a", "/a", false), mk("/b", "/b", false)],
            vec![mk("/a", "/a", false), mk("/a/b", "/a/b", false)],
            vec![
                mk("/a", "/a", false),
                mk("/a/b", "/a/b", false),
                mk("/a/b/c", "/a/b/c", false),
                mk("/x", "/x", true),
            ],
        ];
        for input in inputs {
            let indexes: Vec<usize> = (0..input.len()).collect();
            let plan = plan_collapse(&input, &indexes).unwrap();
            let second = plan_collapse(&plan.kept, &[]).unwrap();
            assert!(
                second.removed.is_empty(),
                "plan.kept should be rule-C compliant, but re-plan removed {} entries",
                second.removed.len(),
            );
            assert_eq!(second.kept, plan.kept);
        }
    }

    #[test]
    fn plan_collapse_result_satisfies_invariant() {
        // After planning, no pair in `kept` covers another pair in `kept`.
        let mounts = vec![
            mk("/a", "/a", false),
            mk("/a/b", "/a/b", false),
            mk("/a/c/d", "/a/c/d", false),
            mk("/x/y", "/x/y", true),
            mk("/x", "/x", true),
        ];
        let indexes: Vec<usize> = (0..mounts.len()).collect();
        let plan = plan_collapse(&mounts, &indexes).unwrap();
        for (i, a) in plan.kept.iter().enumerate() {
            for (j, b) in plan.kept.iter().enumerate() {
                if i != j {
                    assert!(
                        !covers(a, b),
                        "invariant violated: {:?} covers {:?} in kept set",
                        a,
                        b,
                    );
                }
            }
        }
    }
```

- [ ] **Step 4.2: Run the tests — they should pass**

```bash
cargo nextest run --lib workspace::tests::plan_collapse
```

Expected: all tests pass. If any fail, the issue is in `plan_collapse`'s algorithm — investigate (likely an edge case in how `covers` handles normalization or how the `parent` lookup picks the first match in a chain).

- [ ] **Step 4.3: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

- [ ] **Step 4.4: Commit**

```bash
git add src/workspace.rs
git commit -s -m "test(workspace): add invariant properties for plan_collapse

Adds two safety-net tests: idempotence (re-planning on plan.kept yields
no further removals) and the kept-set invariant (no pair in kept covers
another pair). Catches regressions in either the algorithm or the covers
predicate."
```

---

## Task 5: `edit_workspace` post-condition

**Files:**
- Modify: `src/config.rs:358-430` (add post-condition before `self.workspaces.insert`)
- Test: `src/config.rs` (tests module)

- [ ] **Step 5.1: Write failing unit tests**

Locate the existing `#[cfg(test)] mod tests` block in `src/config.rs` (around line 524) and append these tests. The test uses the existing `AppConfig` builder pattern — check the nearest existing test like `edit_workspace_updates_workdir_and_mounts` for the setup style, and mirror it.

Use this shape (adapt `AppConfig::default()` / workspace construction to match existing test style in the file):

```rust
    #[test]
    fn edit_workspace_rejects_upsert_that_introduces_child_under_existing_parent() {
        use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceEdit};

        let mut config = AppConfig::default();
        config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a".into(),
                    mounts: vec![MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    }],
                    allowed_roles: vec![],
                    default_role: None,
                    last_agent: None,
                },
            )
            .unwrap();

        let err = config
            .edit_workspace(
                "test",
                WorkspaceEdit {
                    upsert_mounts: vec![MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: false,
                    }],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains("already covered"));
    }

    #[test]
    fn edit_workspace_rejects_upsert_with_readonly_mismatch_vs_existing_child() {
        use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceEdit};

        let mut config = AppConfig::default();
        config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a/b".into(),
                    mounts: vec![MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: true,
                    }],
                    allowed_roles: vec![],
                    default_role: None,
                    last_agent: None,
                },
            )
            .unwrap();

        let err = config
            .edit_workspace(
                "test",
                WorkspaceEdit {
                    upsert_mounts: vec![MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    }],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains("readonly"));
    }

    #[test]
    fn edit_workspace_accepts_pre_collapsed_upsert_that_replaces_children() {
        // CLI's job is to pre-collapse: instead of upserting just the parent
        // (which would leave children as redundants and fail post-condition),
        // CLI removes the children via remove_destinations AND upserts the
        // parent. Post-condition passes.
        use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceEdit};

        let mut config = AppConfig::default();
        config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a/b".into(),
                    mounts: vec![
                        MountConfig { src: "/a/b".into(), dst: "/a/b".into(), readonly: false },
                        MountConfig { src: "/a/c".into(), dst: "/a/c".into(), readonly: false },
                    ],
                    allowed_roles: vec![],
                    default_role: None,
                    last_agent: None,
                },
            )
            .unwrap();

        config
            .edit_workspace(
                "test",
                WorkspaceEdit {
                    upsert_mounts: vec![MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    }],
                    remove_destinations: vec!["/a/b".into(), "/a/c".into()],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();

        let ws = config.list_workspaces().iter().find(|(n, _)| *n == "test").unwrap().1;
        assert_eq!(ws.mounts.len(), 1);
        assert_eq!(ws.mounts[0].src, "/a");
    }

    #[test]
    fn edit_workspace_rejects_leaving_pre_existing_violation() {
        // Workspace starts with a pre-existing violation. An unrelated edit
        // (e.g., adding an allowed agent) should fail the post-condition until
        // the violation is cleaned up.
        use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceEdit};

        let mut config = AppConfig::default();
        // Bypass create_workspace's post-check by inserting directly into the
        // map (simulates a legacy config read from disk).
        config.workspaces.insert(
            "legacy".into(),
            WorkspaceConfig {
                workdir: "/a".into(),
                mounts: vec![
                    MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false },
                    MountConfig { src: "/a/b".into(), dst: "/a/b".into(), readonly: false },
                ],
                allowed_roles: vec![],
                default_role: None,
                last_agent: None,
            },
        );

        let err = config
            .edit_workspace(
                "legacy",
                WorkspaceEdit {
                    allowed_agents_to_add: vec!["agent-x".into()],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("redundant") || msg.contains("already covered"));
    }
```

Note: if `config.workspaces` is private, use the test-internal `AppConfig::default_with_legacy_mounts(...)` helper approach instead — add a `#[cfg(test)]` helper like:

```rust
    #[cfg(test)]
    pub(crate) fn insert_workspace_raw(&mut self, name: &str, ws: WorkspaceConfig) {
        self.workspaces.insert(name.into(), ws);
    }
```

into the `impl AppConfig` block near `list_workspaces`. Use that helper in the test instead of direct field access.

- [ ] **Step 5.2: Run the tests to confirm they fail**

```bash
cargo nextest run --lib config::tests::edit_workspace_rejects
cargo nextest run --lib config::tests::edit_workspace_accepts_pre_collapsed
```

Expected: `edit_workspace_rejects_*` tests fail (no post-check yet — edits go through). `edit_workspace_accepts_pre_collapsed_upsert_that_replaces_children` passes (current behavior). The `edit_workspace_rejects_leaving_pre_existing_violation` test fails.

- [ ] **Step 5.3: Wire post-condition into `edit_workspace`**

Open `src/config.rs` and locate `edit_workspace` (starts at line 358). Add the post-condition check immediately before the existing `validate_workspace_config(name, &workspace)?;` call at line 427:

```rust
        // Rule-C invariant: after applying this edit, the mount list must
        // contain no covering pairs. The CLI layer pre-collapses via
        // `plan_collapse`; if anything redundant remains here, the caller is
        // buggy (or the workspace has a pre-existing violation that wasn't
        // cleaned up).
        //
        // Distinguish cases by re-running plan_collapse with all indexes
        // unmarked (new_indexes = &[]): any removal means a violation is
        // present, whether pre-existing or freshly introduced.
        match crate::workspace::plan_collapse(&workspace.mounts, &[]) {
            Ok(plan) if plan.removed.is_empty() => {}
            Ok(plan) => {
                let details: Vec<String> = plan
                    .removed
                    .iter()
                    .map(|r| format!("{} covered by {}", r.child.src, r.covered_by.src))
                    .collect();
                anyhow::bail!(
                    "workspace {name:?} would contain redundant mounts after this edit:\n  - {}\n\
                     use `jackin workspace prune {name}` or pass `--prune` to clean up",
                    details.join("\n  - ")
                );
            }
            Err(e) => return Err(e.into()),
        }
```

Also ensure `CollapseError` is `Into<anyhow::Error>`. Since `CollapseError` derives `thiserror::Error`, `anyhow::Error: From<E>` for any `E: std::error::Error + Send + Sync + 'static`, so `.into()` works. No extra impl needed.

- [ ] **Step 5.4: Run the tests and confirm they pass**

```bash
cargo nextest run --lib config::tests::edit_workspace
```

Expected: all new tests plus existing `edit_workspace_*` tests pass. Any existing test that accidentally creates a redundant mount will now fail — in that case, fix the test fixture (e.g., add a `remove_destinations` entry) rather than relaxing the post-condition.

- [ ] **Step 5.5: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures.

- [ ] **Step 5.6: Commit**

```bash
git add src/config.rs
git commit -s -m "feat(config): enforce plan_collapse post-condition on edit_workspace

After applying a WorkspaceEdit, re-plans the mount list under rule C. If
any pair covers another, the edit is rejected with a targeted error
naming the offending mounts and pointing the operator to
\`jackin workspace prune\` or the --prune flag.

This makes the rule-C invariant load-bearing: no edit that passes the
library can leave redundant mounts on disk, regardless of caller. The
CLI layer pre-collapses so this check is a no-op in normal flow; it
catches non-CLI misuse and pre-existing violations that need cleanup."
```

---

## Task 6: `create_workspace` post-condition

**Files:**
- Modify: `src/config.rs:345-356` (`create_workspace`)
- Test: `src/config.rs` (tests module)

- [ ] **Step 6.1: Write failing unit tests**

Append to the config tests module:

```rust
    #[test]
    fn create_workspace_errors_on_child_under_parent_in_initial_mounts() {
        use crate::workspace::{MountConfig, WorkspaceConfig};

        let mut config = AppConfig::default();
        let err = config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a".into(),
                    mounts: vec![
                        MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false },
                        MountConfig { src: "/a/b".into(), dst: "/a/b".into(), readonly: false },
                    ],
                    allowed_roles: vec![],
                    default_role: None,
                    last_agent: None,
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains("redundant") || err.to_string().contains("already covered"));
    }

    #[test]
    fn create_workspace_errors_on_readonly_mismatch_in_initial_mounts() {
        use crate::workspace::{MountConfig, WorkspaceConfig};

        let mut config = AppConfig::default();
        let err = config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a".into(),
                    mounts: vec![
                        MountConfig { src: "/a".into(), dst: "/a".into(), readonly: false },
                        MountConfig { src: "/a/b".into(), dst: "/a/b".into(), readonly: true },
                    ],
                    allowed_roles: vec![],
                    default_role: None,
                    last_agent: None,
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains("readonly"));
    }

    #[test]
    fn create_workspace_accepts_already_collapsed_mount_set() {
        use crate::workspace::{MountConfig, WorkspaceConfig};

        let mut config = AppConfig::default();
        config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a".into(),
                    mounts: vec![MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    }],
                    allowed_roles: vec![],
                    default_role: None,
                    last_agent: None,
                },
            )
            .unwrap();
    }
```

- [ ] **Step 6.2: Run the tests to confirm they fail**

```bash
cargo nextest run --lib config::tests::create_workspace
```

Expected: the two error-case tests fail (no post-check yet). The accept-case passes.

- [ ] **Step 6.3: Add post-condition to `create_workspace`**

Open `src/config.rs` at `create_workspace` (line 345). Replace the body with:

```rust
    pub fn create_workspace(
        &mut self,
        name: &str,
        workspace: WorkspaceConfig,
    ) -> anyhow::Result<()> {
        if self.workspaces.contains_key(name) {
            anyhow::bail!("workspace {name:?} already exists; use `workspace edit`");
        }
        validate_workspace_config(name, &workspace)?;

        // Rule-C invariant: the initial mount list must be pairwise
        // non-covering. All mounts are "new" in a create.
        let all_indexes: Vec<usize> = (0..workspace.mounts.len()).collect();
        match crate::workspace::plan_collapse(&workspace.mounts, &all_indexes) {
            Ok(plan) if plan.removed.is_empty() => {}
            Ok(plan) => {
                let details: Vec<String> = plan
                    .removed
                    .iter()
                    .map(|r| format!("{} covered by {}", r.child.src, r.covered_by.src))
                    .collect();
                anyhow::bail!(
                    "workspace {name:?} initial mounts contain redundant entries:\n  - {}",
                    details.join("\n  - ")
                );
            }
            Err(e) => return Err(e.into()),
        }

        self.workspaces.insert(name.to_string(), workspace);
        Ok(())
    }
```

- [ ] **Step 6.4: Run the tests and confirm they pass**

```bash
cargo nextest run --lib config::tests::create_workspace
```

Expected: all new tests pass. Existing tests in the file may break if they construct a `WorkspaceConfig` with internal redundants — fix the fixture, not the post-check.

- [ ] **Step 6.5: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

- [ ] **Step 6.6: Commit**

```bash
git add src/config.rs
git commit -s -m "feat(config): enforce plan_collapse post-condition on create_workspace

create_workspace now re-plans the initial mount list under rule C and
rejects any input containing covering pairs (either as
CollapseError or as redundants). Symmetric with the post-condition
added to edit_workspace, making the on-disk invariant load-bearing on
both write paths."
```

---

## Task 7: `workspace create` CLI auto-collapse + stderr summary

**Files:**
- Modify: `src/lib.rs:610-653` (CLI handler for `WorkspaceCommand::Create`)

- [ ] **Step 7.1: Add auto-collapse before calling `create_workspace`**

Open `src/lib.rs` at the `WorkspaceCommand::Create` handler (line 610). Replace the block that runs from `let mount_count = all_mounts.len();` through the call to `config.create_workspace(...)` with:

```rust
                // Pre-collapse under rule C so the create_workspace
                // post-condition sees a clean mount list. Any rule-C error
                // (readonly mismatch, etc.) surfaces here before we try to
                // write.
                let all_indexes: Vec<usize> = (0..all_mounts.len()).collect();
                let plan = workspace::plan_collapse(&all_mounts, &all_indexes)?;
                if !plan.removed.is_empty() {
                    let removed_list: Vec<String> = plan
                        .removed
                        .iter()
                        .map(|r| tui::shorten_home(&r.child.src))
                        .collect();
                    // Parent paths in a single create are all the same set; pick
                    // the first for the summary headline.
                    let parent = tui::shorten_home(&plan.removed[0].covered_by.src);
                    eprintln!(
                        "collapsed {} redundant mount(s) under {parent}: {}",
                        plan.removed.len(),
                        removed_list.join(", ")
                    );
                }
                let final_mounts = plan.kept;
                let mount_count = final_mounts.len();
                config.create_workspace(
                    &name,
                    WorkspaceConfig {
                        workdir: expanded_workdir,
                        mounts: final_mounts,
                        allowed_roles,
                        default_role,
                        last_agent: None,
                    },
                )?;
```

Import `workspace` module items are already in scope via existing `use` statements in this file. If `workspace::plan_collapse` is not resolvable, ensure `workspace` is a module alias for `crate::workspace` in this file (it is — see the existing `workspace::resolve_path` call in the same handler).

- [ ] **Step 7.2: Verify existing `workspace create` unit tests still pass**

```bash
cargo nextest run --lib cli::tests::parses_workspace_create
```

Expected: all `parses_workspace_create_*` tests pass (they test argument parsing, not runtime behavior, so no change).

- [ ] **Step 7.3: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures. Integration-level coverage comes in Task 11.

- [ ] **Step 7.4: Commit**

```bash
git add src/lib.rs
git commit -s -m "feat(cli): auto-collapse redundant mounts on workspace create

The workspace create handler now calls plan_collapse on the initial
mount set before invoking config.create_workspace. When redundant
descendants are detected, they are silently dropped and a one-line
summary is printed to stderr. Readonly mismatches surface as errors.

The operator is creating fresh state, so no prompt — just information."
```

---

## Task 8: Clap flags for `workspace edit`: `--yes` and `--prune`

**Files:**
- Modify: `src/cli.rs:288-319` (`WorkspaceCommand::Edit` variant)
- Test: `src/cli.rs` (tests module around line 707)

- [ ] **Step 8.1: Write failing clap parsing tests**

Append to the `#[cfg(test)] mod tests` block in `src/cli.rs`:

```rust
    #[test]
    fn parses_workspace_edit_with_yes_flag() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "edit",
            "proj-alpha",
            "--mount",
            "/tmp/proj-alpha",
            "--yes",
        ])
        .unwrap();
        match cli.command {
            Command::Workspace {
                command: WorkspaceCommand::Edit { assume_yes, .. },
            } => assert!(assume_yes),
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn parses_workspace_edit_with_prune_flag() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "edit",
            "proj-alpha",
            "--prune",
        ])
        .unwrap();
        match cli.command {
            Command::Workspace {
                command: WorkspaceCommand::Edit { prune, .. },
            } => assert!(prune),
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn parses_workspace_edit_with_yes_short_form() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "edit",
            "proj-alpha",
            "-y",
        ])
        .unwrap();
        match cli.command {
            Command::Workspace {
                command: WorkspaceCommand::Edit { assume_yes, .. },
            } => assert!(assume_yes),
            other => panic!("unexpected command {other:?}"),
        }
    }
```

- [ ] **Step 8.2: Run the tests — they should fail (fields don't exist yet)**

```bash
cargo nextest run --lib cli::tests::parses_workspace_edit_with
```

Expected: compilation errors.

- [ ] **Step 8.3: Add flags to `WorkspaceCommand::Edit` in `src/cli.rs`**

In `src/cli.rs`, modify the `Edit { ... }` variant of `WorkspaceCommand` (starting at line 288). Add these two fields at the end of the existing field list, just before the closing brace of the variant (after `clear_default_agent`):

```rust
        /// Skip confirmation prompts for mount collapses
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        assume_yes: bool,
        /// Also remove pre-existing redundant mounts (rule-C violations) as part of this edit
        #[arg(long, default_value_t = false)]
        prune: bool,
```

Also update the after_long_help Examples block (lines 279-286) to include:
```
  jackin workspace edit my-app --mount ~/Projects/my-app --yes
  jackin workspace edit my-app --prune
```

- [ ] **Step 8.4: Update existing `Edit` destructuring in `src/lib.rs`**

Open `src/lib.rs` at the `WorkspaceCommand::Edit { ... }` match arm (starts around line 770). Add `assume_yes` and `prune` to the destructured fields:

```rust
            WorkspaceCommand::Edit {
                name,
                workdir,
                mounts,
                remove_destinations,
                no_workdir_mount,
                allowed_roles,
                remove_allowed_agents,
                default_role,
                clear_default_agent,
                assume_yes,
                prune,
            } => {
```

For this task, the two new fields are accepted but not yet used in the handler body — suppress any unused-variable lint by prefixing with `_`:

```rust
                let _assume_yes = assume_yes;
                let _prune = prune;
```

right after the destructuring. These underscores will be removed in Task 9 when the flags are actually wired up.

- [ ] **Step 8.5: Run the tests and confirm they pass**

```bash
cargo nextest run --lib cli::tests::parses_workspace_edit
```

Expected: all `parses_workspace_edit_*` tests pass, including the new three.

- [ ] **Step 8.6: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures.

- [ ] **Step 8.7: Commit**

```bash
git add src/cli.rs src/lib.rs
git commit -s -m "feat(cli): add --yes and --prune flags to workspace edit

--yes (-y) skips the mount-collapse confirmation prompt for
non-interactive use. --prune opts into cleaning up pre-existing
redundant mounts as part of the edit. Flags are parsed but not yet
wired up; behavior is implemented in the next commit."
```

---

## Task 9: `workspace edit` CLI pre-collapse + prompt + non-TTY bail

**Files:**
- Modify: `src/lib.rs:768-841` (CLI handler for `WorkspaceCommand::Edit`)

- [ ] **Step 9.1: Read the existing handler carefully**

```bash
grep -n "WorkspaceCommand::Edit" /Users/donbeave/Projects/jackin-project/jackin/src/lib.rs
```

Re-read the handler to confirm the exact structure you're about to rewrite.

- [ ] **Step 9.2: Implement pre-collapse + prompt logic**

In `src/lib.rs`, replace the body of the `WorkspaceCommand::Edit` handler (from `let upsert_mounts = mounts` through `Ok(())` at the end of the arm). Keep the existing `changes` summary collection.

The new flow:

1. Parse `upsert_mounts` from CLI input (unchanged).
2. Build the "post-upsert list" the same way `edit_workspace` will apply upserts (merge by `dst`, append if new).
3. Track `new_indexes` as the final position of each upsert in the post-upsert list.
4. Call `plan_collapse`.
5. Partition `plan.removed` into edit-driven (≥1 index in `new_indexes`) vs pre-existing (both indexes absent).
6. Reject pre-existing when `--prune` not set.
7. Prompt (or bail on non-TTY without `--yes`) when any collapse is to be performed.
8. Translate the collapse into extra `remove_destinations` so `edit_workspace`'s existing upsert-by-dst logic produces the clean set, then call `edit_workspace`.

Replace the handler body with:

```rust
                let upsert_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec_resolved(value))
                    .collect::<Result<Vec<_>>>()?;

                // Build the "post-upsert list" the same way edit_workspace will
                // apply upserts: start from existing mounts (after applying
                // remove_destinations), then merge each upsert by dst.
                let current_ws = config
                    .workspaces
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?
                    .clone();

                let mut post_upsert: Vec<workspace::MountConfig> = current_ws
                    .mounts
                    .iter()
                    .filter(|m| !remove_destinations.iter().any(|d| d == &m.dst))
                    .cloned()
                    .collect();
                let mut new_indexes: Vec<usize> = Vec::new();
                for upsert in &upsert_mounts {
                    if let Some(pos) = post_upsert.iter().position(|m| m.dst == upsert.dst) {
                        post_upsert[pos] = upsert.clone();
                        new_indexes.push(pos);
                    } else {
                        post_upsert.push(upsert.clone());
                        new_indexes.push(post_upsert.len() - 1);
                    }
                }

                // Plan the collapse.
                let plan = workspace::plan_collapse(&post_upsert, &new_indexes)?;

                // Partition removals by origin.
                let (edit_driven, pre_existing): (Vec<_>, Vec<_>) = plan
                    .removed
                    .iter()
                    .partition(|r| {
                        let child_idx = post_upsert
                            .iter()
                            .position(|m| m == &r.child)
                            .expect("child must appear in post_upsert list");
                        let parent_idx = post_upsert
                            .iter()
                            .position(|m| m == &r.covered_by)
                            .expect("parent must appear in post_upsert list");
                        new_indexes.contains(&child_idx) || new_indexes.contains(&parent_idx)
                    });

                // Reject pre-existing violations unless --prune.
                if !pre_existing.is_empty() && !prune {
                    let details: Vec<String> = pre_existing
                        .iter()
                        .map(|r| {
                            format!(
                                "{} covered by {}",
                                tui::shorten_home(&r.child.src),
                                tui::shorten_home(&r.covered_by.src),
                            )
                        })
                        .collect();
                    anyhow::bail!(
                        "workspace {name:?} already contains redundant mounts:\n  - {}\n\
                         run `jackin workspace prune {name}` to clean up, or pass --prune to this edit",
                        details.join("\n  - ")
                    );
                }

                // If there are any collapses to apply, prompt (or bail on
                // non-TTY without --yes).
                if !plan.removed.is_empty() && !assume_yes {
                    use std::io::IsTerminal;
                    if !std::io::stdin().is_terminal() {
                        anyhow::bail!(
                            "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively"
                        );
                    }

                    if !edit_driven.is_empty() {
                        eprintln!(
                            "Adding mount(s) will subsume {} existing mount(s):",
                            edit_driven.len()
                        );
                        for r in &edit_driven {
                            eprintln!("  • {}", tui::shorten_home(&r.child.src));
                        }
                    }
                    if !pre_existing.is_empty() {
                        eprintln!(
                            "Cleaning up {} pre-existing redundant mount(s):",
                            pre_existing.len()
                        );
                        for r in &pre_existing {
                            eprintln!("  • {}", tui::shorten_home(&r.child.src));
                        }
                    }
                    eprintln!("These will be removed from the workspace.");

                    let confirmed = dialoguer::Confirm::new()
                        .with_prompt("Proceed?")
                        .default(false)
                        .interact()?;
                    if !confirmed {
                        anyhow::bail!("aborted by operator");
                    }
                }

                // Translate collapse into remove_destinations so edit_workspace's
                // existing remove + upsert logic produces the clean set.
                let mut effective_removes = remove_destinations.clone();
                for r in &plan.removed {
                    if !effective_removes.contains(&r.child.dst) {
                        effective_removes.push(r.child.dst.clone());
                    }
                }

                // Collect what changed for the summary (same as before, plus
                // collapse summary).
                let mut changes: Vec<String> = Vec::new();
                if let Some(ref w) = workdir {
                    changes.push(format!("workdir → {}", tui::shorten_home(w)));
                }
                for m in &upsert_mounts {
                    if m.src == m.dst {
                        changes.push(format!("added mount {}", tui::shorten_home(&m.src)));
                    } else {
                        changes.push(format!(
                            "added mount {} → {}",
                            tui::shorten_home(&m.src),
                            tui::shorten_home(&m.dst)
                        ));
                    }
                }
                for dst in &remove_destinations {
                    changes.push(format!("removed mount {}", tui::shorten_home(dst)));
                }
                for r in &plan.removed {
                    changes.push(format!(
                        "collapsed {} under {}",
                        tui::shorten_home(&r.child.src),
                        tui::shorten_home(&r.covered_by.src)
                    ));
                }
                if no_workdir_mount {
                    changes.push("removed workdir auto-mount".to_string());
                }
                for agent in &allowed_roles {
                    changes.push(format!("allowed agent {agent}"));
                }
                for agent in &remove_allowed_agents {
                    changes.push(format!("removed agent {agent}"));
                }
                if clear_default_agent {
                    changes.push("cleared default agent".to_string());
                } else if let Some(ref agent) = default_role {
                    changes.push(format!("default agent → {agent}"));
                }

                config.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        workdir: workdir.map(|w| resolve_path(&w)),
                        upsert_mounts,
                        remove_destinations: effective_removes,
                        no_workdir_mount,
                        allowed_agents_to_add: allowed_roles,
                        allowed_agents_to_remove: remove_allowed_agents,
                        default_role: if clear_default_agent {
                            Some(None)
                        } else {
                            default_role.map(Some)
                        },
                    },
                )?;
                config.save(&paths)?;
                println!("Updated workspace {name:?}:");
                for change in &changes {
                    println!("  - {change}");
                }
                Ok(())
```

Remove the `let _assume_yes = assume_yes; let _prune = prune;` underscores from Task 8 — both are now used.

Also note: `config.workspaces` may be private. If it is, add a `pub(crate) fn get_workspace(&self, name: &str) -> Option<&WorkspaceConfig>` to `AppConfig` and use that here. Check access by running the compiler:

```bash
cargo check
```

If access errors appear, add the accessor to `src/config.rs`:

```rust
    pub fn get_workspace(&self, name: &str) -> Option<&WorkspaceConfig> {
        self.workspaces.get(name)
    }
```

and use `config.get_workspace(&name)` instead of `config.workspaces.get(&name)` in `lib.rs`.

- [ ] **Step 9.3: Run all existing workspace edit tests**

```bash
cargo nextest run --lib workspace
cargo nextest run --lib config
cargo nextest run --lib cli::tests
```

Expected: everything passes.

- [ ] **Step 9.4: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures.

- [ ] **Step 9.5: Commit**

```bash
git add src/lib.rs src/config.rs
git commit -s -m "feat(cli): prompt before collapsing redundant mounts on workspace edit

The workspace edit handler now pre-plans under rule C before writing.
Outcomes:

- CollapseError (readonly mismatch, child under existing parent) →
  propagate as anyhow error, no write.
- No collapses → proceed.
- Edit-driven collapses only → prompt via dialoguer (unless --yes);
  non-TTY stdin without --yes bails.
- Pre-existing violations present, --prune not set → reject with
  guidance toward workspace prune or --prune.
- Pre-existing violations present, --prune set → fold into the same
  prompt, categorized.

Collapses are translated into extra remove_destinations so
edit_workspace's existing remove + upsert loop produces the clean set."
```

---

## Task 10: `WorkspaceCommand::Prune` subcommand

**Files:**
- Modify: `src/cli.rs` (add `Prune` variant to `WorkspaceCommand`)
- Modify: `src/lib.rs` (add handler for `Prune`)
- Test: `src/cli.rs` (tests module)

- [ ] **Step 10.1: Write failing clap parsing test**

Append to the `src/cli.rs` tests module:

```rust
    #[test]
    fn parses_workspace_prune_command() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "prune",
            "proj-alpha",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Workspace {
                command: WorkspaceCommand::Prune { .. }
            }
        ));
    }

    #[test]
    fn parses_workspace_prune_with_yes() {
        let cli = Cli::try_parse_from([
            "jackin",
            "workspace",
            "prune",
            "proj-alpha",
            "--yes",
        ])
        .unwrap();
        match cli.command {
            Command::Workspace {
                command: WorkspaceCommand::Prune { assume_yes, .. },
            } => assert!(assume_yes),
            other => panic!("unexpected command {other:?}"),
        }
    }
```

- [ ] **Step 10.2: Run the tests — they should fail**

```bash
cargo nextest run --lib cli::tests::parses_workspace_prune
```

Expected: compilation errors.

- [ ] **Step 10.3: Add `Prune` variant to `WorkspaceCommand` in `src/cli.rs`**

Insert before the existing `Remove { ... }` variant (at line 320):

```rust
    /// Remove redundant mounts (rule-C violations) from a saved workspace
    #[command(
        before_help = BANNER,
        styles = HELP_STYLES,
        after_long_help = "\
Examples:
  jackin workspace prune my-app
  jackin workspace prune my-app --yes"
    )]
    Prune {
        /// Name of the workspace to prune
        name: String,
        /// Skip the confirmation prompt
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        assume_yes: bool,
    },
```

- [ ] **Step 10.4: Add handler in `src/lib.rs`**

In `src/lib.rs`, locate the `Command::Workspace { command } => match command { ... }` block (around line 609). Add the `Prune` arm immediately before `WorkspaceCommand::Remove`:

```rust
            WorkspaceCommand::Prune { name, assume_yes } => {
                let current_ws = config
                    .get_workspace(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?
                    .clone();

                // All existing mounts; nothing new.
                let plan = workspace::plan_collapse(&current_ws.mounts, &[])?;
                if plan.removed.is_empty() {
                    println!("Workspace {name:?} has no redundant mounts.");
                    return Ok(());
                }

                if !assume_yes {
                    use std::io::IsTerminal;
                    if !std::io::stdin().is_terminal() {
                        anyhow::bail!(
                            "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively"
                        );
                    }
                    eprintln!(
                        "Will remove {} redundant mount(s) from workspace {name:?}:",
                        plan.removed.len()
                    );
                    for r in &plan.removed {
                        eprintln!(
                            "  • {} (covered by {})",
                            tui::shorten_home(&r.child.src),
                            tui::shorten_home(&r.covered_by.src),
                        );
                    }
                    let confirmed = dialoguer::Confirm::new()
                        .with_prompt("Proceed?")
                        .default(false)
                        .interact()?;
                    if !confirmed {
                        anyhow::bail!("aborted by operator");
                    }
                }

                let remove_dsts: Vec<String> =
                    plan.removed.iter().map(|r| r.child.dst.clone()).collect();
                config.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        remove_destinations: remove_dsts,
                        ..WorkspaceEdit::default()
                    },
                )?;
                config.save(&paths)?;
                println!(
                    "Pruned {} redundant mount(s) from workspace {name:?}.",
                    plan.removed.len()
                );
                Ok(())
            }
```

If `WorkspaceEdit` doesn't currently derive `Default`, add `#[derive(Default)]` to it in `src/workspace.rs`:

```rust
#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    // ...existing fields...
}
```

Check the current definition: it already has `#[derive(Debug, Clone, Default)]` at `src/workspace.rs:25` — good, no change needed.

Also ensure `AppConfig::get_workspace` exists (added in Task 9 Step 9.2 as a contingency); if not added, add it now.

- [ ] **Step 10.5: Run the parsing tests and confirm they pass**

```bash
cargo nextest run --lib cli::tests::parses_workspace_prune
```

Expected: both tests pass.

- [ ] **Step 10.6: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures.

- [ ] **Step 10.7: Commit**

```bash
git add src/cli.rs src/lib.rs src/config.rs
git commit -s -m "feat(cli): add workspace prune subcommand

\`jackin workspace prune <name>\` computes plan_collapse on the
workspace's current mounts, prompts the operator (or requires --yes
for non-TTY), and applies the collapse via edit_workspace. Thin
wrapper over existing primitives; no new library logic."
```

---

## Task 11: CLI integration tests

**Files:**
- Create: `tests/workspace_mount_collapse.rs`

- [ ] **Step 11.1: Create the integration test file**

Write `tests/workspace_mount_collapse.rs` with the following content:

```rust
#![cfg(unix)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::{TempDir, tempdir};

/// Creates an isolated jackin environment: temp HOME with a pre-populated
/// workspace config. Returns the tempdir (keep alive for the duration of the
/// test) and the host directories used for mount sources.
struct Env {
    _temp: TempDir,
    home: std::path::PathBuf,
    proj_alpha: std::path::PathBuf,
    sub_a: std::path::PathBuf,
    sub_b: std::path::PathBuf,
}

fn setup_env() -> Env {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let proj_alpha = home.join("Projects").join("proj-alpha");
    let sub_a = proj_alpha.join("sub-a");
    let sub_b = proj_alpha.join("sub-b");
    fs::create_dir_all(&sub_a).unwrap();
    fs::create_dir_all(&sub_b).unwrap();
    Env {
        _temp: temp,
        home,
        proj_alpha,
        sub_a,
        sub_b,
    }
}

fn jackin(env: &Env) -> Command {
    let mut cmd = Command::cargo_bin("jackin").unwrap();
    cmd.env("HOME", &env.home);
    cmd
}

fn create_workspace_with_children(env: &Env, name: &str) {
    jackin(env)
        .args([
            "workspace", "create", name,
            "--workdir", env.sub_a.to_str().unwrap(),
            "--no-workdir-mount",
            "--mount", env.sub_a.to_str().unwrap(),
            "--mount", env.sub_b.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn workspace_create_auto_collapses_and_prints_summary() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace", "create", "test",
            "--workdir", env.proj_alpha.to_str().unwrap(),
            "--mount", env.sub_a.to_str().unwrap(),
            "--mount", env.sub_b.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("collapsed"))
        .stderr(predicate::str::contains("sub-a"))
        .stderr(predicate::str::contains("sub-b"));
}

#[test]
fn workspace_create_rejects_readonly_mismatch() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace", "create", "test",
            "--workdir", env.proj_alpha.to_str().unwrap(),
            "--mount", env.proj_alpha.to_str().unwrap(),
            "--mount", &format!("{}:ro", env.sub_a.to_str().unwrap()),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("readonly"));
}

#[test]
fn workspace_edit_with_yes_collapses_children_under_new_parent() {
    let env = setup_env();
    create_workspace_with_children(&env, "test");
    jackin(&env)
        .args([
            "workspace", "edit", "test",
            "--mount", env.proj_alpha.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("collapsed"));

    jackin(&env)
        .args(["workspace", "show", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("proj-alpha"))
        .stdout(predicate::str::contains("sub-a").not())
        .stdout(predicate::str::contains("sub-b").not());
}

#[test]
fn workspace_edit_fails_on_readonly_mismatch_with_clear_error() {
    let env = setup_env();
    // Build a workspace where sub-a is ro.
    jackin(&env)
        .args([
            "workspace", "create", "test",
            "--workdir", env.sub_a.to_str().unwrap(),
            "--no-workdir-mount",
            "--mount", &format!("{}:ro", env.sub_a.to_str().unwrap()),
        ])
        .assert()
        .success();

    // Adding an rw parent should fail with readonly mismatch.
    jackin(&env)
        .args([
            "workspace", "edit", "test",
            "--mount", env.proj_alpha.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("readonly"));
}

#[test]
fn workspace_edit_fails_on_child_under_existing_parent() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace", "create", "test",
            "--workdir", env.proj_alpha.to_str().unwrap(),
            "--mount", env.proj_alpha.to_str().unwrap(),
        ])
        .assert()
        .success();

    jackin(&env)
        .args([
            "workspace", "edit", "test",
            "--mount", env.sub_a.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already covered"));
}

#[test]
fn workspace_edit_non_tty_without_yes_bails() {
    let env = setup_env();
    create_workspace_with_children(&env, "test");
    // assert_cmd does not attach a TTY by default — stdin is not a terminal.
    jackin(&env)
        .args([
            "workspace", "edit", "test",
            "--mount", env.proj_alpha.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("without confirmation"));
}

#[test]
fn workspace_prune_removes_pre_existing_redundants() {
    let env = setup_env();
    // Intentionally create a workspace in a state that already contains
    // redundants by using `workspace edit` with --prune flag off-path (or by
    // skipping create post-check via the legacy config path). Simplest
    // approach: create sub-a and sub-b cleanly, then add the parent with
    // --yes so children are collapsed — that leaves no pre-existing violation.
    //
    // To seed a pre-existing violation we write config.toml directly.
    let config_path = env.home.join(".config/jackin/config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    let proj = env.proj_alpha.to_str().unwrap();
    let sub_a = env.sub_a.to_str().unwrap();
    fs::write(
        &config_path,
        format!(
            r#"
[workspaces.test]
workdir = "{proj}"

[[workspaces.test.mounts]]
src = "{proj}"
dst = "{proj}"
readonly = false

[[workspaces.test.mounts]]
src = "{sub_a}"
dst = "{sub_a}"
readonly = false
"#
        ),
    )
    .unwrap();

    jackin(&env)
        .args(["workspace", "prune", "test", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pruned"));

    jackin(&env)
        .args(["workspace", "show", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sub-a").not());
}

#[test]
fn workspace_prune_on_clean_workspace_is_noop() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace", "create", "test",
            "--workdir", env.proj_alpha.to_str().unwrap(),
            "--mount", env.proj_alpha.to_str().unwrap(),
        ])
        .assert()
        .success();

    jackin(&env)
        .args(["workspace", "prune", "test", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no redundant mounts"));
}

#[test]
fn workspace_edit_rejects_pre_existing_violation_without_prune() {
    let env = setup_env();
    // Seed pre-existing violation as in the prune test.
    let config_path = env.home.join(".config/jackin/config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    let proj = env.proj_alpha.to_str().unwrap();
    let sub_a = env.sub_a.to_str().unwrap();
    fs::write(
        &config_path,
        format!(
            r#"
[workspaces.test]
workdir = "{proj}"

[[workspaces.test.mounts]]
src = "{proj}"
dst = "{proj}"
readonly = false

[[workspaces.test.mounts]]
src = "{sub_a}"
dst = "{sub_a}"
readonly = false
"#
        ),
    )
    .unwrap();

    // Unrelated edit (adding allowed agent) should be blocked by the
    // pre-existing redundancy check.
    jackin(&env)
        .args([
            "workspace", "edit", "test",
            "--allowed-role", "some-agent",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("redundant"))
        .stderr(predicate::str::contains("prune"));
}

#[test]
fn workspace_edit_with_prune_cleans_pre_existing_violations() {
    let env = setup_env();
    let config_path = env.home.join(".config/jackin/config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    let proj = env.proj_alpha.to_str().unwrap();
    let sub_a = env.sub_a.to_str().unwrap();
    fs::write(
        &config_path,
        format!(
            r#"
[workspaces.test]
workdir = "{proj}"

[[workspaces.test.mounts]]
src = "{proj}"
dst = "{proj}"
readonly = false

[[workspaces.test.mounts]]
src = "{sub_a}"
dst = "{sub_a}"
readonly = false
"#
        ),
    )
    .unwrap();

    jackin(&env)
        .args([
            "workspace", "edit", "test",
            "--allowed-role", "some-agent",
            "--prune",
            "--yes",
        ])
        .assert()
        .success();

    jackin(&env)
        .args(["workspace", "show", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sub-a").not());
}
```

- [ ] **Step 11.2: Run the integration tests**

```bash
cargo nextest run --test workspace_mount_collapse
```

Expected: all tests pass. If any fail:
- Check stderr/stdout predicates match the exact wording used in `lib.rs` handlers.
- If config.toml seed format is wrong, run `jackin workspace create` once with valid args and copy the resulting file structure.
- If HOME env override doesn't pick up (directories crate caches or ignores HOME on some platforms), replace with `.env_clear().env("HOME", &env.home).env("PATH", std::env::var("PATH").unwrap())`.

- [ ] **Step 11.3: Run full pre-commit check**

```bash
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures.

- [ ] **Step 11.4: Commit**

```bash
git add tests/workspace_mount_collapse.rs
git commit -s -m "test: add integration tests for mount collapse CLI flows

Covers workspace create auto-collapse + summary, readonly mismatch
rejection on create, workspace edit collapse with --yes, readonly and
child-under-parent errors on edit, non-TTY bail without --yes, and the
full prune subcommand including clean workspace no-op, plus --prune on
edit for pre-existing violations."
```

---

## Task 12: Documentation updates

**Files:**
- Modify: `docs/src/content/docs/commands/workspace.mdx`
- Modify: `docs/src/content/docs/guides/mounts.mdx`

- [ ] **Step 12.1: Update `docs/src/content/docs/commands/workspace.mdx`**

Open the file and locate the `workspace edit` section. Add a new subsection describing mount collapse behavior. Follow the existing MDX style (check other subsections for `<Aside>`, `<Steps>` usage).

Add near the end of the `workspace edit` section:

```mdx
### Mount collapse

When you add a mount that is an ancestor of existing mounts (same host-to-container offset), the descendants become redundant. `jackin workspace edit` detects this and prompts before removing them:

```
Adding mount(s) will subsume 2 existing mount(s):
  • ~/Projects/proj-alpha/sub-a
  • ~/Projects/proj-alpha/sub-b
These will be removed from the workspace.
Proceed? [y/N]
```

Flags:
- `--yes` / `-y` — skip the prompt (required for non-interactive use).
- `--prune` — also clean up pre-existing redundant mounts in the workspace.

Conflict cases that are rejected with an error (not prompted):
- **Readonly mismatch.** Parent and descendant have different `:ro` flags.
- **Child under existing parent.** Adding a mount that is already covered by a pre-existing mount in the workspace.

In both cases the error text names both paths and the operator's next step.

### `workspace prune`

`jackin workspace prune <name>` removes pre-existing redundant mounts from a saved workspace. Useful when upgrading from an older jackin config or after a hand-edit that left redundants.

```
jackin workspace prune my-app        # interactive
jackin workspace prune my-app --yes  # non-interactive
```
```

- [ ] **Step 12.2: Update `docs/src/content/docs/guides/mounts.mdx`**

Add a subsection after the existing mount spec explanation:

```mdx
## Redundant-mount invariant

A saved workspace never contains two mounts where one strictly covers the other at the same container location. The rule: mount **P** covers mount **C** iff:

1. `P.src` is an ancestor of `C.src`, AND
2. the suffix from `P.src` to `C.src` equals the suffix from `P.dst` to `C.dst`.

In that case `C` projects the same host subtree to the same container path that `P` already exposes it at — making `C` strictly redundant.

jackin enforces this on write: `workspace create` auto-removes redundants (with a stderr summary), and `workspace edit` prompts before collapsing (see [mount collapse](../commands/workspace.mdx#mount-collapse)). Identity mounts (same `src` AND same `dst`) are not covered by this rule — they're handled by upsert-by-destination semantics.

Ad-hoc `-v` mounts passed on the command line at launch time are not collapsed; they stay literal.
```

- [ ] **Step 12.3: Verify docs build (optional but recommended)**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin/docs
bun install --frozen-lockfile
bun run build
```

Expected: build succeeds with no MDX syntax errors.

If bun is not installed or the `node_modules` setup is cross-OS-broken, skip the build check; the markdown is straightforward and Starlight is forgiving.

- [ ] **Step 12.4: Commit**

Docs-only commit — skip `cargo fmt/clippy/nextest`.

```bash
git add docs/src/content/docs/commands/workspace.mdx docs/src/content/docs/guides/mounts.mdx
git commit -s -m "docs: document mount collapse behavior and workspace prune

Adds a mount-collapse subsection to commands/workspace.mdx explaining
the prompt, --yes, --prune, and the two conflict error cases. Adds a
redundant-mount-invariant subsection to guides/mounts.mdx describing
rule C and when it applies."
```

---

## Task 13: Final verification, changelog, and PR

**Files:**
- Modify: `CHANGELOG.md` (if the project uses it for unreleased entries)

- [ ] **Step 13.1: Check for a changelog**

```bash
cat /Users/donbeave/Projects/jackin-project/jackin/CHANGELOG.md | head -30
```

If there is an "Unreleased" section with categories (Added, Changed, etc.), append entries; if the format is different, follow the existing pattern. If no changelog or it's auto-generated from commits, skip.

- [ ] **Step 13.2: Add changelog entries (if applicable)**

Under **Unreleased → Added**:
```
- `jackin workspace prune <name>` subcommand to remove rule-C-redundant mounts from a saved workspace.
- `--yes` / `-y` and `--prune` flags on `jackin workspace edit`.
```

Under **Unreleased → Changed**:
```
- `jackin workspace create` and `jackin workspace edit` now detect and remove descendant mounts that are strictly covered by a parent mount (rule C). `workspace create` auto-collapses with a stderr summary; `workspace edit` prompts before applying.
```

- [ ] **Step 13.3: Run the full test suite one more time**

```bash
cd /Users/donbeave/Projects/jackin-project/jackin
cargo fmt -- --check && cargo clippy && cargo nextest run
```

Expected: zero warnings, zero failures across unit + integration tests.

- [ ] **Step 13.4: Review the branch**

```bash
git log --oneline main..feature/mount-collapse-redundants
git diff --stat main..feature/mount-collapse-redundants
```

Verify:
- One commit per task, all Conventional-Commits-formatted.
- Every commit has a `Signed-off-by` trailer matching `git config user.email`.
- No real project or organization names anywhere in the diffs.

- [ ] **Step 13.5: Commit changelog (if added)**

```bash
git add CHANGELOG.md
git commit -s -m "chore: update changelog for mount collapse feature"
```

- [ ] **Step 13.6: Push and open PR — STOP HERE, DO NOT MERGE**

```bash
git push -u origin feature/mount-collapse-redundants
```

Then create a PR with a title like `feat(workspace): auto-collapse redundant descendant mounts` and a body summarizing:
- What the feature does (one paragraph).
- New flags and the new subcommand.
- Link to the spec: `docs/superpowers/specs/2026-04-22-mount-collapse-redundants-design.md`.
- Explicit note that config versioning is deferred to a separate spec.
- Test plan checklist.

**Do not merge.** Per `AGENTS.md` (line 4, "Pull Request Merging"), agents never merge a PR without explicit per-PR operator approval. Hand the PR URL back to the operator and wait.

---

## Self-Review

**1. Spec coverage**

Re-reading the spec section-by-section:

- **Problem / Example** — covered by integration tests (`workspace_edit_with_yes_collapses_children_under_new_parent`, Task 11).
- **Rule C definition** — implemented in `covers` (Task 1), locked down by 9 unit tests.
- **ReadonlyMismatch / ChildUnderExistingParent** — implemented in `plan_collapse` (Tasks 2–3), tested at library (Tasks 5–6) and CLI (Task 11) levels.
- **Data types (CollapsePlan, Removal, CollapseError, plan_collapse)** — Task 2.
- **Algorithm** — Task 2; edge cases (transitive chain, multiple siblings) explicitly tested.
- **Trigger scope: create_workspace / edit_workspace / resolve_load_workspace untouched** — Tasks 5, 6, 7, 9; `resolve_load_workspace` not in any task.
- **Write-path post-condition** — Tasks 5, 6.
- **CLI prompt / --yes / --prune / non-TTY** — Task 9; integration coverage in Task 11.
- **CLI workspace create auto-collapse + stderr** — Task 7; integration coverage in Task 11.
- **workspace prune subcommand** — Task 10; integration coverage in Task 11.
- **Pre-existing violation accommodation (reject without --prune)** — Tasks 5, 9, 11.
- **Load-path behavior (no change)** — achieved by not adding any load-time check; covered implicitly by seed-config tests in Task 11 (config with pre-existing violation loads, then is rejected only on write without `--prune`).
- **Tests listed in spec** — all unit, property-style, and integration tests are mapped to tasks.
- **Fixture convention (generic names)** — enforced in Task 11 integration tests and in all other test code.
- **Docs updates** — Task 12.

No gaps.

**2. Placeholder scan**

- No TBD / TODO / "implement later" strings.
- No "add appropriate error handling" / "handle edge cases" — every error path is explicit.
- No "similar to Task N" — code is repeated where needed.
- Every step with code shows the code.
- One contingency ("if `config.workspaces` is private, add `get_workspace`") is explicit with full code.

**3. Type consistency**

- `CollapsePlan { kept, removed }` — used consistently across Tasks 2, 5, 6, 7, 9, 10.
- `Removal { child, covered_by }` — consistently accessed via `.child` and `.covered_by`.
- `CollapseError` variants: `ReadonlyMismatch { parent, child }` and `ChildUnderExistingParent { parent, child }` — consistent field names across definition (Task 2) and tests (Task 3).
- `plan_collapse(&[MountConfig], &[usize]) -> Result<CollapsePlan, CollapseError>` — consistent signature in Tasks 2, 5, 6, 7, 9, 10.
- `WorkspaceCommand::Edit` fields `assume_yes` and `prune` — introduced in Task 8, used in Task 9.
- `WorkspaceCommand::Prune { name, assume_yes }` — consistent in Task 10.

No inconsistencies found.
