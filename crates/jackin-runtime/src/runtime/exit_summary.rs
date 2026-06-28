//! Exit "still running" summary data.
//!
//! When the operator leaves one foreground session while other jackin❯
//! instances are still running, the rich boundary outro does not play: the
//! operator is still inside the Construct. We still build a compact,
//! privacy-preserving summary for diagnostics. Saved workspaces contribute
//! `workspace · role ×count`; ad-hoc folders collapse to a generic count so
//! their paths are never surfaced.

use std::collections::{BTreeMap, HashSet};

use crate::instance::InstanceIndex;

/// Build the headline + rows from the still-running instances.
///
/// Saved workspaces (named) contribute one row per role: `workspace · role
/// ×count`. Ad-hoc directories (no saved workspace) collapse to a single
/// generic `N folders` row — their paths are never surfaced. The headline
/// counts all still-running instances ("agents" = instances).
pub(super) fn summary(running_bases: &[String], index: &InstanceIndex) -> (String, Vec<String>) {
    let running: HashSet<&str> = running_bases.iter().map(String::as_str).collect();
    let mut saved: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut folders: HashSet<String> = HashSet::new();
    let total = running_bases.len();
    for entry in &index.instances {
        if !running.contains(entry.container_base.as_str()) {
            continue;
        }
        if entry.workspace_name.is_some() && !entry.workspace_label.trim().is_empty() {
            *saved
                .entry(entry.workspace_label.clone())
                .or_default()
                .entry(entry.role_key.clone())
                .or_default() += 1;
        } else {
            folders.insert(entry.workdir.clone());
        }
    }

    let mut rows = Vec::new();
    for (workspace, roles) in &saved {
        for (role, count) in roles {
            rows.push(format!("{workspace}  \u{00b7}  {role} \u{00d7}{count}"));
        }
    }
    if !folders.is_empty() {
        let n = folders.len();
        rows.push(format!("{n} folder{}", if n == 1 { "" } else { "s" }));
    }
    let headline = format!(
        "{total} agent{} still in the Construct",
        if total == 1 { "" } else { "s" }
    );
    (headline, rows)
}

#[cfg(test)]
mod tests;
