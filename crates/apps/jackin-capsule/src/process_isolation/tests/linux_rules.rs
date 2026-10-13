// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::linux::{
    FULL, READ_FILE_ONLY, Rule, add_execute_only_ancestors, drop_privileges, install_landlock,
    retained_capability_mask, rules_for, rules_for_test, support,
};

#[test]
fn landlock_rules_are_exact_for_selected_slot_and_exclude_secret_roots() {
    assert_eq!(
        size_of::<super::super::linux::RulesetAttr>(),
        size_of::<u64>()
    );
    assert_eq!(size_of::<super::super::linux::PathBeneathAttr>(), 12);
    let config = CapsuleConfig {
        instances: vec!["slot-a".to_owned()],
        agents: BTreeMap::from([("slot-a".to_owned(), "claude".to_owned())]),
        instance_home_dirs: BTreeMap::from([(
            "slot-a".to_owned(),
            "/home/agent/.claude-a".to_owned(),
        )]),
        instance_mount_paths: BTreeMap::from([(
            "slot-a".to_owned(),
            vec![
                "/home/agent/.claude-a".to_owned(),
                "/jackin/claude-a/credentials.json".to_owned(),
            ],
        )]),
        ..CapsuleConfig::default()
    };
    let rules = rules_for(
        &config,
        Some("slot-a"),
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect("construct exact Landlock rules");
    let paths = rules
        .iter()
        .map(|rule| rule.path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path == "/home/agent/.claude-a"));
    assert!(
        paths
            .iter()
            .any(|path| path == "/jackin/claude-a/credentials.json")
    );
    assert_eq!(
        rules
            .iter()
            .find(|rule| rule.path == Path::new("/jackin/claude-a/credentials.json"))
            .expect("selected auth file rule")
            .access,
        READ_FILE_ONLY
    );
    assert!(!rules.iter().any(|rule| {
        rule.path == Path::new(jackin_protocol::ACCOUNT_CREDENTIALS_DIR)
            || rule
                .path
                .starts_with(Path::new(jackin_protocol::ACCOUNT_CREDENTIALS_DIR))
            || (rule.path == Path::new("/jackin/runtime")
                && rule.access != super::super::linux::TRAVERSE)
            || (rule.path == Path::new("/jackin/default-home")
                && rule.access != super::super::linux::TRAVERSE)
            || (rule.path == Path::new("/home/agent")
                && rule.access != super::super::linux::TRAVERSE)
            || (rule.path == Path::new("/jackin/run")
                && rule.access != super::super::linux::TRAVERSE)
            || (rule.path == Path::new("/proc") && rule.access != super::super::linux::TRAVERSE)
    }));
    assert!(!paths.iter().any(|path| path == "/home/agent/.claude-b"));
    assert!(
        rules
            .iter()
            .any(|rule| rule.path == Path::new("/home/agent")
                && rule.access == super::super::linux::TRAVERSE)
    );
    assert!(
        rules
            .iter()
            .any(|rule| rule.path == Path::new("/jackin/runtime")
                && rule.access == super::super::linux::TRAVERSE)
    );
}

#[test]
fn claude_installer_share_dir_is_granted_read_only() {
    let config = CapsuleConfig {
        instances: vec!["slot-a".to_owned()],
        agents: BTreeMap::from([("slot-a".to_owned(), "claude".to_owned())]),
        instance_home_dirs: BTreeMap::from([(
            "slot-a".to_owned(),
            "/home/agent/.claude-a".to_owned(),
        )]),
        instance_mount_paths: BTreeMap::from([(
            "slot-a".to_owned(),
            vec!["/home/agent/.claude-a".to_owned()],
        )]),
        ..CapsuleConfig::default()
    };
    let rules = rules_for(
        &config,
        Some("slot-a"),
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/7"),
    )
    .expect("construct Landlock rules");
    let share = rules
        .iter()
        .find(|rule| rule.path == Path::new("/home/agent/.local/share/claude"))
        .expect("Claude installer share-dir rule");
    assert_eq!(share.access, READ_ONLY);
    assert!(!share.required);
}

#[test]
fn cwd_boundary_rejects_root_ancestors_and_noncanonical_aliases() {
    let config = CapsuleConfig::default();
    for cwd in ["/", "/home", "/jackin", "/workspace/../"] {
        let error = rules_for_test(
            &config,
            None,
            Path::new(cwd),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect_err("protected cwd must be rejected before rule construction");
        assert!(
            error.to_string().contains("protected"),
            "unexpected cwd rejection for {cwd}: {error:#}"
        );
    }
}

#[test]
fn cwd_boundary_rejects_existing_symlink_alias_to_protected_root() {
    let temp = tempfile::tempdir().expect("symlink fixture");
    let alias = temp.path().join("home-alias");
    std::os::unix::fs::symlink("/home", &alias).expect("protected-root symlink");

    let error = rules_for_test(
        &CapsuleConfig::default(),
        None,
        &alias,
        Path::new("/jackin/run/sessions/1"),
    )
    .expect_err("symlink alias to protected root must be rejected");
    assert!(error.to_string().contains("protected"));
}

#[test]
fn mount_boundaries_reject_existing_symlink_alias_to_protected_root() {
    let temp = tempfile::tempdir().expect("symlink fixture");
    let alias = temp.path().join("home-alias");
    std::os::unix::fs::symlink("/home", &alias).expect("protected-root symlink");
    let alias = alias.to_string_lossy().into_owned();

    let config = CapsuleConfig {
        workspace_mounts: vec![alias.clone()],
        ..CapsuleConfig::default()
    };
    let error = rules_for_test(
        &config,
        None,
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect_err("symlink alias dst to protected root must be rejected");
    assert!(error.to_string().contains("protected"));

    let config = CapsuleConfig {
        worktree_git_targets: vec![alias],
        ..CapsuleConfig::default()
    };
    let error = rules_for_test(
        &config,
        None,
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect_err("symlink alias git target must be rejected");
    assert!(error.to_string().contains("outside"));
}

#[test]
fn cwd_boundary_rejects_ancestor_of_any_private_mount_destination() {
    let config = CapsuleConfig {
        instance_mount_paths: BTreeMap::from([(
            "canary".to_owned(),
            vec!["/workspace/private-slot".to_owned()],
        )]),
        ..CapsuleConfig::default()
    };
    let error = rules_for_test(
        &config,
        None,
        Path::new("/workspace"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect_err("cwd ancestor of private mount must be rejected");
    assert!(error.to_string().contains("mount destination"));
}

#[test]
fn workspace_mounts_and_git_targets_gain_full_access_outside_cwd() {
    let config = CapsuleConfig {
        workspace_mounts: vec!["/workspace/other".to_owned()],
        worktree_git_targets: vec!["/jackin/host/workspace/other/.git".to_owned()],
        ..CapsuleConfig::default()
    };
    let rules = rules_for_test(
        &config,
        None,
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect("dst outside cwd and aux git target must be granted");
    for path in ["/workspace/other", "/jackin/host/workspace/other/.git"] {
        let rule = rules
            .iter()
            .find(|rule| rule.path == Path::new(path))
            .unwrap_or_else(|| panic!("missing Landlock rule for {path}"));
        assert_eq!(rule.access, FULL_WITH_UNIX, "wrong access for {path}");
        assert!(rule.required, "rule for {path} must be required");
    }
}

#[test]
fn hostile_worktree_git_targets_are_rejected() {
    for target in [
        "/jackin/run/x",
        "/home/agent/x",
        "/jackin/host",
        "/workspace/x",
        "/jackin/host/../run/x",
        "/jackin/host/a/../b",
        "relative/path",
        "",
    ] {
        let config = CapsuleConfig {
            worktree_git_targets: vec![target.to_owned()],
            ..CapsuleConfig::default()
        };
        let error = rules_for_test(
            &config,
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("outside")
                || message.contains("must not contain")
                || message.contains("absolute"),
            "unexpected rejection for {target:?}: {message}"
        );
    }
}

#[test]
fn hostile_workspace_mounts_are_rejected() {
    for dst in [
        "/",
        "/home",
        "/home/agent",
        "/home/agent/.claude",
        "/jackin",
        "/jackin/run",
        "/workspace/../jackin",
        "relative/path",
        "",
    ] {
        let config = CapsuleConfig {
            workspace_mounts: vec![dst.to_owned()],
            ..CapsuleConfig::default()
        };
        let error = rules_for_test(
            &config,
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("protected") || message.contains("absolute"),
            "unexpected rejection for {dst:?}: {message}"
        );
    }
    let config = CapsuleConfig {
        workspace_mounts: vec!["/workspace".to_owned()],
        instance_mount_paths: BTreeMap::from([(
            "canary".to_owned(),
            vec!["/workspace/private-slot".to_owned()],
        )]),
        ..CapsuleConfig::default()
    };
    let error = rules_for_test(
        &config,
        None,
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect_err("dst ancestor of private mount must be rejected");
    assert!(error.to_string().contains("mount destination"));
}
