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

use crate::workspace::{CollapseError, MountConfig, Removal, WorkspaceConfig, plan_collapse};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.to_string(),
            dst: dst.to_string(),
            readonly: false,
        }
    }

    fn workspace(workdir: &str, mounts: Vec<MountConfig>) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: workdir.to_string(),
            mounts,
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
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
}
