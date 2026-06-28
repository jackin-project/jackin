use super::{
    DirtyRepo, ExitDecision, assess_dirty, decide_exit, exit_action_json, policy_is_ask,
    write_exit_action_to,
};
use jackin_protocol::{CapsuleConfig, ExitAction};

fn repo(path: &str) -> DirtyRepo {
    DirtyRepo {
        path: path.to_owned(),
        changed: Vec::new(),
        unpushed: 0,
    }
}

fn changed_file(status: char, path: &str) -> jackin_core::worktree_dirty::ChangedFile {
    jackin_core::worktree_dirty::ChangedFile {
        status,
        path: path.to_owned(),
    }
}

#[test]
fn summary_line_formats_changed_and_unpushed() {
    let r = DirtyRepo {
        path: "/jackin/work/jackin".to_owned(),
        changed: vec![changed_file('M', "a.rs"), changed_file('?', "b.md")],
        unpushed: 1,
    };
    assert_eq!(r.summary_line(), "jackin   2 changed · 1 unpushed");
}

#[test]
fn summary_line_omits_zero_counts() {
    let only_unpushed = DirtyRepo {
        path: "/work/holla".to_owned(),
        changed: Vec::new(),
        unpushed: 3,
    };
    assert_eq!(only_unpushed.summary_line(), "holla   3 unpushed");
    let only_changed = DirtyRepo {
        path: "/work/holla".to_owned(),
        changed: vec![changed_file('M', "a")],
        unpushed: 0,
    };
    assert_eq!(only_changed.summary_line(), "holla   1 changed");
}

#[test]
fn changed_file_carries_status_and_path() {
    let r = DirtyRepo {
        path: "/work/x".to_owned(),
        changed: vec![changed_file('M', "src/a.rs"), changed_file('?', "n.md")],
        unpushed: 0,
    };
    assert_eq!(r.changed[0].status, 'M');
    assert_eq!(r.changed[0].path, "src/a.rs");
    assert_eq!(r.changed[1].status, '?');
    assert_eq!(r.changed[1].path, "n.md");
}

#[test]
fn label_is_final_path_component() {
    assert_eq!(repo("/jackin/work/jackin").label(), "jackin");
    assert_eq!(repo("/jackin/work/holla-apt/").label(), "holla-apt");
    assert_eq!(repo("jackin").label(), "jackin");
}

#[test]
fn policy_ask_is_default_when_unset() {
    let config = CapsuleConfig::default();
    assert!(policy_is_ask(&config));
}

#[test]
fn policy_ask_explicit() {
    let config = CapsuleConfig {
        dirty_exit_policy: Some("ask".to_owned()),
        ..CapsuleConfig::default()
    };
    assert!(policy_is_ask(&config));
}

#[test]
fn policy_keep_and_discard_are_not_ask() {
    for policy in ["keep", "discard"] {
        let config = CapsuleConfig {
            dirty_exit_policy: Some(policy.to_owned()),
            ..CapsuleConfig::default()
        };
        assert!(!policy_is_ask(&config), "{policy} must not be ask");
    }
}

#[tokio::test]
async fn assess_dirty_empty_when_no_isolated_worktrees() {
    let config = CapsuleConfig::default();
    assert!(assess_dirty(&config).await.is_empty());
}

#[tokio::test]
async fn decide_exit_drains_for_keep_and_discard_policies() {
    // Even with isolated worktrees present, keep/discard skip the modal.
    for policy in ["keep", "discard"] {
        let config = CapsuleConfig {
            dirty_exit_policy: Some(policy.to_owned()),
            isolated_worktrees: vec!["/jackin/work/jackin".to_owned()],
            ..CapsuleConfig::default()
        };
        assert_eq!(decide_exit(&config).await, ExitDecision::Drain, "{policy}");
    }
}

#[tokio::test]
async fn decide_exit_drains_when_ask_but_no_dirty_worktrees() {
    let config = CapsuleConfig {
        dirty_exit_policy: Some("ask".to_owned()),
        isolated_worktrees: Vec::new(),
        ..CapsuleConfig::default()
    };
    assert_eq!(decide_exit(&config).await, ExitDecision::Drain);
}

#[test]
fn exit_action_json_matches_serde_snake_case() {
    assert_eq!(exit_action_json(ExitAction::Keep), "\"keep\"");
    assert_eq!(exit_action_json(ExitAction::Discard), "\"discard\"");
}

#[test]
fn write_exit_action_writes_expected_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exit-action.json");
    write_exit_action_to(&path, ExitAction::Discard).expect("write");
    let contents = std::fs::read_to_string(&path).expect("read");
    assert_eq!(contents, "\"discard\"");
}
