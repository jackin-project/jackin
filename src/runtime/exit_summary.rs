//! Exit "still running" summary data.
//!
//! When the operator leaves one foreground session while other jackin'
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
mod tests {
    use super::*;
    use crate::instance::{InstanceIndexEntry, InstanceStatus};

    #[allow(clippy::ref_option)]
    fn entry(
        id: &str,
        base: &str,
        workspace_name: Option<&str>,
        workspace_label: &str,
        workdir: &str,
        role: &str,
    ) -> InstanceIndexEntry {
        InstanceIndexEntry {
            instance_id: id.to_string(),
            container_base: base.to_string(),
            workspace_name: workspace_name.map(str::to_string),
            workspace_label: workspace_label.to_string(),
            workdir: workdir.to_string(),
            role_key: role.to_string(),
            agent_runtime: "claude".to_string(),
            status: InstanceStatus::Running,
            updated_at: "2026-05-25T00:00:00Z".to_string(),
        }
    }

    fn index(entries: Vec<InstanceIndexEntry>) -> InstanceIndex {
        InstanceIndex {
            version: 1,
            instances: entries,
        }
    }

    #[test]
    fn saved_workspaces_listed_by_role_with_counts() {
        let idx = index(vec![
            entry("aaa", "jk-aaa", Some("app"), "app", "/app", "the-architect"),
            entry("bbb", "jk-bbb", Some("app"), "app", "/app", "the-architect"),
            entry("ccc", "jk-ccc", Some("app"), "app", "/app", "agent-smith"),
        ]);
        let running = vec![
            "jk-aaa".to_string(),
            "jk-bbb".to_string(),
            "jk-ccc".to_string(),
        ];
        let (headline, rows) = summary(&running, &idx);
        assert!(headline.contains("3 agents"), "headline: {headline}");
        assert!(
            rows.iter().any(|r| r.contains("app")
                && r.contains("the-architect")
                && r.contains("\u{00d7}2"))
        );
        assert!(
            rows.iter()
                .any(|r| r.contains("agent-smith") && r.contains("\u{00d7}1"))
        );
    }

    #[test]
    fn private_folders_collapse_to_a_count() {
        let idx = index(vec![
            entry(
                "aaa",
                "jk-aaa",
                None,
                "",
                "/home/me/proj-a",
                "the-architect",
            ),
            entry(
                "bbb",
                "jk-bbb",
                None,
                "",
                "/home/me/proj-b",
                "the-architect",
            ),
            entry("ccc", "jk-ccc", Some("app"), "app", "/app", "the-architect"),
        ]);
        let running = vec![
            "jk-aaa".to_string(),
            "jk-bbb".to_string(),
            "jk-ccc".to_string(),
        ];
        let (headline, rows) = summary(&running, &idx);
        assert!(headline.contains("3 agents"));
        // Two distinct private folders, no paths shown.
        assert!(rows.iter().any(|r| r == "2 folders"), "rows: {rows:?}");
        assert!(
            !rows.iter().any(|r| r.contains("proj-a")),
            "paths must not leak"
        );
    }

    #[test]
    fn excludes_instances_not_in_the_running_set() {
        let idx = index(vec![
            entry("aaa", "jk-aaa", Some("app"), "app", "/app", "the-architect"),
            entry("zzz", "jk-zzz", Some("app"), "app", "/app", "the-architect"),
        ]);
        let running = vec!["jk-aaa".to_string()];
        let (headline, _) = summary(&running, &idx);
        assert!(headline.contains("1 agent "), "singular: {headline}");
    }

    #[test]
    fn headline_counts_running_bases_when_index_is_partial() {
        let idx = index(vec![entry(
            "aaa",
            "jk-aaa",
            Some("app"),
            "app",
            "/app",
            "the-architect",
        )]);
        let running = vec!["jk-aaa".to_string(), "jk-missing".to_string()];
        let (headline, rows) = summary(&running, &idx);
        assert!(headline.contains("2 agents"), "headline: {headline}");
        assert!(
            rows.iter()
                .any(|r| r.contains("app") && r.contains("the-architect")),
            "known indexed rows should still be shown: {rows:?}"
        );
    }
}
