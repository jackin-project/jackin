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
pub(crate) struct WorkspaceCreatePlan {
    /// The final mount list the caller should persist.
    pub final_mounts: Vec<MountConfig>,
    /// Mounts subsumed during the collapse — i.e. each `Removal`'s child
    /// was redundant against its parent in the supplied mount list.
    pub collapsed: Vec<Removal>,
}

/// Plan for `jackin workspace edit`.
pub(crate) struct WorkspaceEditPlan {
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
/// Collapses redundancies among the supplied mount list. Callers must
/// pass every mount explicitly — the planner does not auto-mount the
/// workdir.
pub(crate) fn plan_create(mounts: &[MountConfig]) -> Result<WorkspaceCreatePlan, CollapseError> {
    let all_indexes: Vec<usize> = (0..mounts.len()).collect();
    let plan = plan_collapse(mounts, &all_indexes)?;
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
pub(crate) fn plan_edit(
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
pub(crate) fn apply_isolation_overrides(
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
mod tests;
