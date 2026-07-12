//! Tests for `exit_summary`.
use super::*;
use crate::instance::{InstanceIndexEntry, InstanceStatus};

#[allow(clippy::ref_option, reason = "documented residual allow; prefer expect when site is lint-true")]
fn entry(
    id: &str,
    base: &str,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role: &str,
) -> InstanceIndexEntry {
    InstanceIndexEntry {
        instance_id: id.to_owned(),
        container_base: base.to_owned(),
        workspace_name: workspace_name.map(str::to_owned),
        workspace_label: workspace_label.to_owned(),
        workdir: workdir.to_owned(),
        role_key: role.to_owned(),
        agent_runtime: "claude".to_owned(),
        status: InstanceStatus::Running,
        updated_at: "2026-05-25T00:00:00Z".to_owned(),
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
        "jk-aaa".to_owned(),
        "jk-bbb".to_owned(),
        "jk-ccc".to_owned(),
    ];
    let (headline, rows) = summary(&running, &idx);
    assert!(headline.contains("3 agents"), "headline: {headline}");
    assert!(
        rows.iter()
            .any(|r| r.contains("app") && r.contains("the-architect") && r.contains("\u{00d7}2"))
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
        "jk-aaa".to_owned(),
        "jk-bbb".to_owned(),
        "jk-ccc".to_owned(),
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
    let running = vec!["jk-aaa".to_owned()];
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
    let running = vec!["jk-aaa".to_owned(), "jk-missing".to_owned()];
    let (headline, rows) = summary(&running, &idx);
    assert!(headline.contains("2 agents"), "headline: {headline}");
    assert!(
        rows.iter()
            .any(|r| r.contains("app") && r.contains("the-architect")),
        "known indexed rows should still be shown: {rows:?}"
    );
}
