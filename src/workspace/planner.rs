//! Pure planning for workspace create and edit operations.
//!
//! Takes parsed-and-resolved inputs and returns a plan describing what would
//! change. Does no I/O, no printing, and does not touch config persistence —
//! callers are responsible for prompting, reporting, and calling
//! `AppConfig::create_workspace` / `edit_workspace` with the plan's outputs.
//!
//! The split-of-responsibility: the planner decides *what* the mount list
//! would look like and *which removals were caused by this edit versus were
//! already redundant*; the CLI decides *whether to ask*, *what to print*, and
//! *whether to treat pre-existing redundancy as a hard error under `--prune`*.

use crate::workspace::MountConfig;
use crate::workspace::WorkspaceConfig;
use crate::workspace::mounts::covers;

/// Plan for `jackin workspace create`.
pub struct WorkspaceCreatePlan {
    /// The final, collapsed mount list the caller should persist.
    pub final_mounts: Vec<MountConfig>,
    /// Mounts subsumed during the collapse. All are edit-driven for create.
    pub collapsed: Vec<Removal>,
}

/// Plan for `jackin workspace edit`.
pub struct WorkspaceEditPlan {
    /// Destinations to remove in the persisted edit. Stacks the user-supplied
    /// `remove_destinations` with every collapsed child, so
    /// `edit_workspace`'s existing remove-then-upsert logic produces the
    /// collapsed set.
    pub effective_removals: Vec<String>,
    /// Redundancies newly created by this edit (the user added a mount that
    /// subsumes an existing one).
    pub edit_driven_collapses: Vec<Removal>,
    /// Redundancies that were already present before this edit.
    ///
    /// Callers typically gate persistence on a `--prune` flag: reject when
    /// this list is non-empty unless the operator opted in to cleanup.
    pub pre_existing_collapses: Vec<Removal>,
}

/// Plan a `workspace create`.
///
/// Auto-inserts a workdir-mount at position 0 unless `no_workdir_mount` is
/// set or the workdir destination is already mounted. Then runs the collapse
/// with every mount marked as new (since everything is "new" for a create).
pub fn plan_create(
    workdir: &str,
    mounts: Vec<MountConfig>,
    no_workdir_mount: bool,
) -> Result<WorkspaceCreatePlan, CollapseError> {
    let mut all_mounts = mounts;
    if !no_workdir_mount {
        let already_mounted = all_mounts.iter().any(|m| m.dst == workdir);
        if !already_mounted {
            all_mounts.insert(
                0,
                MountConfig {
                    src: workdir.to_string(),
                    dst: workdir.to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            );
        }
    }
    let all_indexes: Vec<usize> = (0..all_mounts.len()).collect();
    let plan = plan_collapse(&all_mounts, &all_indexes)?;
    Ok(WorkspaceCreatePlan {
        final_mounts: plan.kept,
        collapsed: plan.removed,
    })
}

/// Plan a `workspace edit`.
///
/// Mirrors `AppConfig::edit_workspace`'s mount-list construction: filter the
/// current mounts by `remove_destinations`, optionally drop the workdir
/// auto-mount, then merge each upsert by dst (replacing on match, appending
/// otherwise). Tracks which indexes were touched by the edit so that
/// `plan_collapse`'s removals can be classified as `edit_driven_collapses`
/// (touching a new/updated mount) or `pre_existing_collapses` (entirely
/// between pre-existing mounts).
pub fn plan_edit(
    current: &WorkspaceConfig,
    upserts: &[MountConfig],
    remove_destinations: &[String],
    no_workdir_mount: bool,
) -> Result<WorkspaceEditPlan, CollapseError> {
    let mut post_upsert: Vec<MountConfig> = current
        .mounts
        .iter()
        .filter(|m| !remove_destinations.iter().any(|d| d == &m.dst))
        .cloned()
        .collect();
    if no_workdir_mount {
        let workdir = &current.workdir;
        post_upsert.retain(|m| !(m.src == *workdir && m.dst == *workdir));
    }
    let mut new_indexes: Vec<usize> = Vec::new();
    for upsert in upserts {
        if let Some(pos) = post_upsert.iter().position(|m| m.dst == upsert.dst) {
            post_upsert[pos] = upsert.clone();
            new_indexes.push(pos);
        } else {
            post_upsert.push(upsert.clone());
            new_indexes.push(post_upsert.len() - 1);
        }
    }

    let plan = plan_collapse(&post_upsert, &new_indexes)?;

    let (edit_driven, pre_existing): (Vec<_>, Vec<_>) = plan.removed.iter().partition(|r| {
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
    let edit_driven: Vec<Removal> = edit_driven.into_iter().cloned().collect();
    let pre_existing: Vec<Removal> = pre_existing.into_iter().cloned().collect();

    let mut effective_removals = remove_destinations.to_vec();
    for r in &plan.removed {
        if !effective_removals.contains(&r.child.dst) {
            effective_removals.push(r.child.dst.clone());
        }
    }

    Ok(WorkspaceEditPlan {
        effective_removals,
        edit_driven_collapses: edit_driven,
        pre_existing_collapses: pre_existing,
    })
}

/// Apply per-destination isolation overrides to the supplied mount list.
///
/// Each `(dst, mode)` pair must reference an existing destination in
/// `mounts`; an unknown destination is a hard error so the operator can fix
/// the typo instead of silently picking up the default. The plan must be the
/// final, post-collapse mount list because the destinations the operator
/// supplied on the CLI must match what gets persisted.
pub fn apply_isolation_overrides(
    mounts: &mut [crate::workspace::MountConfig],
    overrides: &[(String, crate::isolation::MountIsolation)],
) -> anyhow::Result<()> {
    for (dst, mode) in overrides {
        let target = mounts.iter_mut().find(|m| m.dst == *dst).ok_or_else(|| {
            anyhow::anyhow!(
                "--mount-isolation references unknown destination `{dst}`; \
                 it must match a mount in the final plan"
            )
        })?;
        target.isolation = *mode;
    }
    Ok(())
}

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
#[non_exhaustive]
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

/// Computes a [`CollapsePlan`] for `mounts`.
///
/// `new_indexes` identifies which entries in `mounts` originate from the
/// current operation (upserts for `edit`, all indexes for `create`). Indexes
/// outside `mounts.len()` are ignored.
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
            .find(|(j, p)| *j != i && covers(p, m));

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

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.to_string(),
            dst: dst.to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }
    }

    fn workspace(workdir: &str, mounts: Vec<MountConfig>) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: workdir.to_string(),
            mounts,
            ..Default::default()
        }
    }

    #[test]
    fn plan_create_inserts_workdir_auto_mount_at_front() {
        let plan = plan_create("/work", vec![mount("/data", "/data")], false).unwrap();

        assert_eq!(plan.final_mounts.len(), 2);
        assert_eq!(plan.final_mounts[0].dst, "/work");
        assert_eq!(plan.final_mounts[0].src, "/work");
        assert_eq!(plan.final_mounts[1].dst, "/data");
        assert!(plan.collapsed.is_empty());
    }

    #[test]
    fn plan_create_skips_auto_mount_when_workdir_already_mounted() {
        let plan = plan_create("/work", vec![mount("/custom-src", "/work")], false).unwrap();

        assert_eq!(plan.final_mounts.len(), 1);
        assert_eq!(plan.final_mounts[0].src, "/custom-src");
        assert_eq!(plan.final_mounts[0].dst, "/work");
        assert!(plan.collapsed.is_empty());
    }

    #[test]
    fn plan_create_no_workdir_mount_suppresses_auto_mount() {
        let plan = plan_create("/work", vec![mount("/data", "/data")], true).unwrap();

        assert_eq!(plan.final_mounts.len(), 1);
        assert_eq!(plan.final_mounts[0].dst, "/data");
        assert!(plan.collapsed.is_empty());
    }

    #[test]
    fn plan_create_collapses_redundant_children_under_parent() {
        // /work auto-inserts, then /work/sub is a child of /work — gets collapsed.
        let plan = plan_create("/work", vec![mount("/work/sub", "/work/sub")], false).unwrap();

        assert_eq!(plan.final_mounts.len(), 1);
        assert_eq!(plan.final_mounts[0].dst, "/work");
        assert_eq!(plan.collapsed.len(), 1);
        assert_eq!(plan.collapsed[0].child.dst, "/work/sub");
        assert_eq!(plan.collapsed[0].covered_by.dst, "/work");
    }

    #[test]
    fn plan_edit_classifies_new_parent_as_edit_driven() {
        // Existing: /work/sub. Add: /work (new parent). /work/sub gets
        // subsumed by the new parent — edit-driven.
        let current = workspace("/work", vec![mount("/work/sub", "/work/sub")]);
        let plan = plan_edit(&current, &[mount("/work", "/work")], &[], false).unwrap();

        assert_eq!(plan.edit_driven_collapses.len(), 1);
        assert_eq!(plan.pre_existing_collapses.len(), 0);
        assert_eq!(plan.edit_driven_collapses[0].child.dst, "/work/sub");
        assert!(plan.effective_removals.contains(&"/work/sub".to_string()));
    }

    #[test]
    fn plan_edit_classifies_untouched_redundancy_as_pre_existing() {
        // Existing config already has /work and /work/sub (redundant).
        // Edit does nothing related to those; collapse reports pre-existing.
        let current = workspace(
            "/work",
            vec![mount("/work", "/work"), mount("/work/sub", "/work/sub")],
        );
        let plan = plan_edit(&current, &[], &[], false).unwrap();

        assert_eq!(plan.edit_driven_collapses.len(), 0);
        assert_eq!(plan.pre_existing_collapses.len(), 1);
        assert_eq!(plan.pre_existing_collapses[0].child.dst, "/work/sub");
    }

    #[test]
    fn plan_edit_applies_remove_destinations_before_upsert() {
        // Remove /work/sub, then add /work. If removes happened *after*
        // upserts, /work would subsume /work/sub and show up as an
        // edit-driven collapse. Because removes come first, nothing collapses.
        let current = workspace("/work", vec![mount("/work/sub", "/work/sub")]);
        let plan = plan_edit(
            &current,
            &[mount("/work", "/work")],
            &["/work/sub".to_string()],
            false,
        )
        .unwrap();

        assert!(
            plan.edit_driven_collapses.is_empty(),
            "no collapse expected: /work/sub was removed before /work was added"
        );
        assert!(plan.pre_existing_collapses.is_empty());
        assert_eq!(plan.effective_removals, vec!["/work/sub".to_string()]);
    }

    #[test]
    fn plan_edit_no_workdir_mount_drops_workdir_auto_mount() {
        // Workdir auto-mount is present; also add /work/sub as an upsert.
        // If no_workdir_mount correctly drops /work before collapse, /work/sub
        // has no parent and no collapse fires. If /work survived, /work/sub
        // would collapse under it as edit-driven.
        let current = workspace(
            "/work",
            vec![mount("/work", "/work"), mount("/data", "/data")],
        );
        let plan = plan_edit(&current, &[mount("/work/sub", "/work/sub")], &[], true).unwrap();

        assert!(
            plan.edit_driven_collapses.is_empty(),
            "no_workdir_mount must drop /work before collapse considers it"
        );
        assert!(plan.pre_existing_collapses.is_empty());
    }

    #[test]
    fn plan_edit_effective_removals_stack_on_user_removals() {
        // User asks to remove /extra. Then an edit adds /work that subsumes
        // /work/sub. Both removals should appear in effective_removals.
        let current = workspace(
            "/work",
            vec![mount("/work/sub", "/work/sub"), mount("/extra", "/extra")],
        );
        let plan = plan_edit(
            &current,
            &[mount("/work", "/work")],
            &["/extra".to_string()],
            false,
        )
        .unwrap();

        assert!(plan.effective_removals.contains(&"/extra".to_string()));
        assert!(plan.effective_removals.contains(&"/work/sub".to_string()));
    }

    #[test]
    fn covers_is_false_for_equal_mounts() {
        let a = MountConfig {
            src: "/a".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let b = a.clone();
        assert!(!covers(&a, &b));
    }

    #[test]
    fn covers_is_true_for_exact_ancestor_with_matching_suffix() {
        let parent = MountConfig {
            src: "/a".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let child = MountConfig {
            src: "/a/b".into(),
            dst: "/a/b".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        assert!(covers(&parent, &child));
    }

    #[test]
    fn covers_is_true_for_deep_ancestor_with_matching_suffix() {
        let parent = MountConfig {
            src: "/a".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let child = MountConfig {
            src: "/a/b/c/d".into(),
            dst: "/a/b/c/d".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        assert!(covers(&parent, &child));
    }

    #[test]
    fn covers_is_true_when_src_and_dst_differ_but_offsets_match() {
        let parent = MountConfig {
            src: "/host/root".into(),
            dst: "/container/root".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let child = MountConfig {
            src: "/host/root/sub".into(),
            dst: "/container/root/sub".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        assert!(covers(&parent, &child));
    }

    #[test]
    fn covers_is_false_when_src_nests_but_dst_offsets_differ() {
        let parent = MountConfig {
            src: "/host/root".into(),
            dst: "/container/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let child = MountConfig {
            src: "/host/root/sub".into(),
            dst: "/container/b/sub".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        assert!(!covers(&parent, &child));
    }

    #[test]
    fn covers_is_false_when_src_does_not_nest() {
        let a = MountConfig {
            src: "/a".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let b = MountConfig {
            src: "/b".into(),
            dst: "/b".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        assert!(!covers(&a, &b));
    }

    #[test]
    fn covers_is_false_for_sibling_prefix_match() {
        // `/a-x` is not a child of `/a`, even though "/a-x".starts_with("/a").
        let parent = MountConfig {
            src: "/a".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let child = MountConfig {
            src: "/a-x".into(),
            dst: "/a-x".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        assert!(!covers(&parent, &child));
    }

    #[test]
    fn covers_normalizes_trailing_slashes() {
        let parent = MountConfig {
            src: "/a/".into(),
            dst: "/a/".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let child = MountConfig {
            src: "/a/b".into(),
            dst: "/a/b".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        assert!(covers(&parent, &child));
    }

    #[test]
    fn covers_handles_different_readonly_flags() {
        // `covers` is purely path-based. Readonly mismatches are caught by plan_collapse.
        let parent = MountConfig {
            src: "/a".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let child = MountConfig {
            src: "/a/b".into(),
            dst: "/a/b".into(),
            readonly: true,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        assert!(covers(&parent, &child));
    }

    fn mk(src: &str, dst: &str, ro: bool) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: ro,
            isolation: crate::isolation::MountIsolation::Shared,
        }
    }

    #[test]
    fn plan_collapse_empty_input_returns_empty_plan() {
        let plan = plan_collapse(&[], &[]).unwrap();
        assert!(plan.kept.is_empty());
        assert!(plan.removed.is_empty());
    }

    #[test]
    fn plan_collapse_preserves_unrelated_mounts() {
        let mounts = vec![mk("/a", "/a", false), mk("/b", "/b", false)];
        let plan = plan_collapse(&mounts, &[0, 1]).unwrap();
        assert_eq!(plan.kept, mounts);
        assert!(plan.removed.is_empty());
    }

    #[test]
    fn plan_collapse_collapses_single_child_under_new_parent() {
        let mounts = vec![
            mk("/a/b", "/a/b", false), // pre-existing child (index 0)
            mk("/a", "/a", false),     // new parent (index 1)
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
        let mounts = vec![mk("/a", "/a", false), mk("/a/b", "/a/b", false)];
        let plan = plan_collapse(&mounts, &[]).unwrap();
        assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
        assert_eq!(plan.removed.len(), 1);
    }

    #[test]
    fn plan_collapse_errors_on_readonly_mismatch_rw_parent_ro_child() {
        let mounts = vec![
            mk("/a/b", "/a/b", true), // ro child
            mk("/a", "/a", false),    // rw parent (new)
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
            mk("/a/b", "/a/b", false), // rw child
            mk("/a", "/a", true),      // ro parent (new)
        ];
        let err = plan_collapse(&mounts, &[1]).unwrap_err();
        assert!(matches!(err, CollapseError::ReadonlyMismatch { .. }));
    }

    #[test]
    fn plan_collapse_errors_on_new_child_under_existing_parent() {
        // Parent at index 0 is pre-existing. Child at index 1 is new.
        let mounts = vec![
            mk("/a", "/a", false),     // existing parent
            mk("/a/b", "/a/b", false), // new child
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
        let mounts = vec![mk("/a/b", "/a/b", false), mk("/a", "/a", false)];
        let plan = plan_collapse(&mounts, &[0, 1]).unwrap();
        assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
        assert_eq!(plan.removed.len(), 1);
    }

    #[test]
    fn plan_collapse_error_message_mentions_both_paths() {
        let mounts = vec![mk("/a/b", "/a/b", true), mk("/a", "/a", false)];
        let err = plan_collapse(&mounts, &[1]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("/a"));
        assert!(msg.contains("/a/b"));
        assert!(msg.contains("readonly"));
    }

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
    fn apply_isolation_overrides_updates_matching_dst() {
        let mut mounts = vec![
            crate::workspace::MountConfig {
                src: "/tmp/a".into(),
                dst: "/workspace/x".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            crate::workspace::MountConfig {
                src: "/tmp/b".into(),
                dst: "/workspace/y".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        ];
        apply_isolation_overrides(
            &mut mounts,
            &[(
                "/workspace/y".into(),
                crate::isolation::MountIsolation::Worktree,
            )],
        )
        .unwrap();
        assert_eq!(
            mounts[1].isolation,
            crate::isolation::MountIsolation::Worktree
        );
        assert_eq!(
            mounts[0].isolation,
            crate::isolation::MountIsolation::Shared
        );
    }

    #[test]
    fn apply_isolation_overrides_unknown_dst_errors() {
        let mut mounts = vec![crate::workspace::MountConfig {
            src: "/tmp/a".into(),
            dst: "/workspace/x".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }];
        let err = apply_isolation_overrides(
            &mut mounts,
            &[("/nope".into(), crate::isolation::MountIsolation::Worktree)],
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown destination `/nope`"));
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
                        "invariant violated: {a:?} covers {b:?} in kept set",
                    );
                }
            }
        }
    }
}
