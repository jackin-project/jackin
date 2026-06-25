use super::{DirtyRepo, assess_dirty, policy_is_ask};
use jackin_protocol::CapsuleConfig;

fn repo(path: &str) -> DirtyRepo {
    DirtyRepo {
        path: path.to_owned(),
        changed: 0,
        unpushed: 0,
    }
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
