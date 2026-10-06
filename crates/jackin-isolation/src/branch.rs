// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Derive the `jackin/scratch/<selector>` branch name for isolated worktree mounts.
//!
//! Pure string derivation — no filesystem or git access. Not responsible for
//! creating or deleting the branch; that lives in `cleanup.rs` and the
//! worktree materialization path.

/// Build the scratch branch name for an isolated mount.
///
/// Selector namespace `/` is preserved; an optional `suffix` is
/// appended to the *final* selector segment with a leading `-`.
/// Current worktree materialization passes `None` because
/// `validate_isolation_layout` rejects multi-isolated-mounts on the
/// same host repo, so each container has at most one scratch branch
/// per host repo.
///
/// Examples:
/// - `branch_name("the-architect", None)` → `jackin/scratch/the-architect`
/// - `branch_name("the-architect", Some("repo-1"))` → `jackin/scratch/the-architect-repo-1`
/// - `branch_name("chainargos/the-architect", None)`
///   → `jackin/scratch/chainargos/the-architect`
pub fn branch_name(selector: &str, suffix: Option<&str>) -> String {
    suffix.map_or_else(
        || format!("jackin/scratch/{selector}"),
        |s| match selector.rsplit_once('/') {
            Some((ns, last)) => format!("jackin/scratch/{ns}/{last}-{s}"),
            None => format!("jackin/scratch/{selector}-{s}"),
        },
    )
}

#[cfg(test)]
mod tests;
