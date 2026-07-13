// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure workspace planning: create, edit, and mount-set collapse.
//!
//! No I/O. Callers apply the plan by calling `AppConfig::create_workspace` /
//! `edit_workspace` with the plan's outputs.

use crate::ConfigError;
use jackin_core::MountIsolation;

use crate::{MountConfig, WorkspaceConfig};

/// Returns `true` iff `parent` strictly covers `child` under rule C.
fn covers(parent: &MountConfig, child: &MountConfig) -> bool {
    let parent_src = parent.src.trim_end_matches('/');
    let parent_dst = parent.dst.trim_end_matches('/');
    let child_src = child.src.trim_end_matches('/');
    let child_dst = child.dst.trim_end_matches('/');
    if parent_src == child_src && parent_dst == child_dst {
        return false;
    }
    let Some(src_suffix) = child_src.strip_prefix(parent_src) else {
        return false;
    };
    if !src_suffix.starts_with('/') {
        return false;
    }
    let Some(dst_suffix) = child_dst.strip_prefix(parent_dst) else {
        return false;
    };
    src_suffix == dst_suffix
}

/// Plan for `jackin workspace create`.
#[derive(Debug)]
pub struct WorkspaceCreatePlan {
    /// Mounts remaining after rule-C collapse.
    pub final_mounts: Vec<MountConfig>,
    /// Mounts removed because a parent already covers them.
    pub collapsed: Vec<Removal>,
}

/// Plan for `jackin workspace edit`.
#[derive(Debug)]
pub struct WorkspaceEditPlan {
    /// Destinations to remove (explicit removals plus collapsed children).
    pub effective_removals: Vec<String>,
    /// Collapses involving at least one mount touched by this edit.
    pub edit_driven_collapses: Vec<Removal>,
    /// Collapses among mounts that already existed before the edit.
    pub pre_existing_collapses: Vec<Removal>,
}

/// Plan a `workspace create`.
pub fn plan_create(mounts: &[MountConfig]) -> Result<WorkspaceCreatePlan, CollapseError> {
    let all_indexes: Vec<usize> = (0..mounts.len()).collect();
    let plan = plan_collapse(mounts, &all_indexes)?;
    Ok(WorkspaceCreatePlan {
        final_mounts: plan.kept,
        collapsed: plan.removed,
    })
}

/// Plan a `workspace edit`.
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
            if let Some(slot) = post_upsert.get_mut(pos) {
                *slot = upsert.clone();
                new_indexes.push(pos);
            }
        } else {
            post_upsert.push(upsert.clone());
            new_indexes.push(post_upsert.len() - 1);
        }
    }

    let plan = plan_collapse(&post_upsert, &new_indexes)?;

    let mut edit_driven = Vec::new();
    let mut pre_existing = Vec::new();
    for removal in &plan.removed {
        let child_idx = post_upsert.iter().position(|m| m == &removal.child).ok_or(
            CollapseError::PlannerInvariant {
                message: "child must appear in post_upsert list",
            },
        )?;
        let parent_idx = post_upsert
            .iter()
            .position(|m| m == &removal.covered_by)
            .ok_or(CollapseError::PlannerInvariant {
                message: "parent must appear in post_upsert list",
            })?;
        if new_indexes.contains(&child_idx) || new_indexes.contains(&parent_idx) {
            edit_driven.push(removal.clone());
        } else {
            pre_existing.push(removal.clone());
        }
    }

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
/// # Errors
/// Returns an error if any override destination is not in the mount list.
pub fn apply_isolation_overrides(
    mounts: &mut [MountConfig],
    overrides: &[(String, MountIsolation)],
) -> anyhow::Result<()> {
    for (dst, mode) in overrides {
        let target = mounts.iter_mut().find(|m| m.dst == *dst).ok_or_else(|| {
            anyhow::Error::from(ConfigError::msg(format!(
                "--mount-isolation references unknown destination `{dst}`; \
                 it must match a mount in the final plan"
            )))
        })?;
        target.isolation = *mode;
    }
    Ok(())
}

/// A proposed mount-set change produced by [`plan_collapse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapsePlan {
    /// Mounts that are not covered by any other mount.
    pub kept: Vec<MountConfig>,
    /// Covered mounts proposed for removal.
    pub removed: Vec<Removal>,
}

/// Records that `child` was collapsed because it is covered by `covered_by`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// Mount that is strictly covered and can be dropped.
    pub child: MountConfig,
    /// Parent mount that already exposes the child's subtree.
    pub covered_by: MountConfig,
}

/// Conditions that prevent a silent collapse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CollapseError {
    /// Parent covers child but `readonly` flags disagree.
    #[error(
        "mount {parent_src} ({parent_mode}) would subsume {child_src} ({child_mode}), \
         but the readonly flag differs. Match the flag or remove the child first.",
        parent_src = parent.src,
        parent_mode = if parent.readonly { "ro" } else { "rw" },
        child_src = child.src,
        child_mode = if child.readonly { "ro" } else { "rw" },
    )]
    ReadonlyMismatch {
        /// Covering parent mount.
        parent: MountConfig,
        /// Covered child mount.
        child: MountConfig,
    },
    /// New child is already covered by an existing parent (nothing to add).
    #[error(
        "mount {child_src} is already covered by existing mount {parent_src}. \
         Nothing to add.",
        child_src = child.src,
        parent_src = parent.src,
    )]
    ChildUnderExistingParent {
        /// Existing parent that covers the new child.
        parent: MountConfig,
        /// New child that cannot be added.
        child: MountConfig,
    },
    /// Internal planner bookkeeping failure.
    #[error("workspace mount planner invariant failed: {message}")]
    PlannerInvariant {
        /// Static description of the broken invariant.
        message: &'static str,
    },
}

/// Compute a [`CollapsePlan`] for `mounts`.
///
/// # Errors
/// Returns an error if mounts have readonly flag mismatches or child-under-existing-parent.
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
