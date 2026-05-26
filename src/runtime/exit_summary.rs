//! Exit "still running" summary.
//!
//! When the operator leaves the foreground session and other jackin' instances
//! are still running, this shows a brief centered white block — styled like the
//! intro phrase screens, with the brand pill at the bottom. It lists which
//! saved workspaces still have running instances (workspace · role ×count) and
//! a generic count of ad-hoc folders (their paths are never shown). Non-rich
//! terminals fall back to a plain line.

use std::collections::{BTreeMap, HashSet};

use crate::instance::InstanceIndex;
use crate::paths::JackinPaths;
use crate::runtime::LoadOptions;

/// Build the headline + rows from the still-running instances.
///
/// Saved workspaces (named) contribute one row per role: `workspace · role
/// ×count`. Ad-hoc directories (no saved workspace) collapse to a single
/// generic `N folders` row — their paths are never surfaced. The headline
/// counts all still-running instances ("agents" = instances).
fn summary(running_bases: &[String], index: &InstanceIndex) -> (String, Vec<String>) {
    let running: HashSet<&str> = running_bases.iter().map(String::as_str).collect();
    let mut saved: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut folders: HashSet<String> = HashSet::new();
    let mut total = 0usize;
    for entry in &index.instances {
        if !running.contains(entry.container_base.as_str()) {
            continue;
        }
        total += 1;
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
    // If the index is missing entries, fall back to the raw running count so
    // the headline is never an undercount.
    if total == 0 {
        total = running_bases.len();
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

/// Show the exit summary: a centered white block with the brand pill on a rich
/// terminal, or a plain line otherwise.
pub fn show(paths: &JackinPaths, running_bases: &[String], opts: &LoadOptions) {
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap_or(InstanceIndex {
        version: 0,
        instances: Vec::new(),
    });
    let (headline, rows) = summary(running_bases, &index);

    if opts.no_rain || opts.no_tui || !super::progress::rich_terminal_supported() {
        eprintln!("{headline}");
        for row in &rows {
            eprintln!("  {row}");
        }
        return;
    }
    crate::tui::outro_summary(&headline, &rows);
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
        let running = vec!["jk-aaa".to_string(), "jk-bbb".to_string(), "jk-ccc".to_string()];
        let (headline, rows) = summary(&running, &idx);
        assert!(headline.contains("3 agents"), "headline: {headline}");
        assert!(rows.iter().any(|r| r.contains("app") && r.contains("the-architect") && r.contains("\u{00d7}2")));
        assert!(rows.iter().any(|r| r.contains("agent-smith") && r.contains("\u{00d7}1")));
    }

    #[test]
    fn private_folders_collapse_to_a_count() {
        let idx = index(vec![
            entry("aaa", "jk-aaa", None, "", "/home/me/proj-a", "the-architect"),
            entry("bbb", "jk-bbb", None, "", "/home/me/proj-b", "the-architect"),
            entry("ccc", "jk-ccc", Some("app"), "app", "/app", "the-architect"),
        ]);
        let running = vec!["jk-aaa".to_string(), "jk-bbb".to_string(), "jk-ccc".to_string()];
        let (headline, rows) = summary(&running, &idx);
        assert!(headline.contains("3 agents"));
        // Two distinct private folders, no paths shown.
        assert!(rows.iter().any(|r| r == "2 folders"), "rows: {rows:?}");
        assert!(!rows.iter().any(|r| r.contains("proj-a")), "paths must not leak");
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
}
