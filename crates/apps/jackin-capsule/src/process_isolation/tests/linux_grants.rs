// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::linux::{
    FULL, READ_FILE_ONLY, Rule, add_execute_only_ancestors, drop_privileges, install_landlock,
    retained_capability_mask, rules_for, rules_for_test, support,
};

#[test]
fn valid_workspace_cwd_retains_full_access_for_workspace_only() {
    let temp = tempfile::tempdir().expect("workspace fixture");
    let workspace = temp.path().join("workspace");
    let session_root = temp.path().join("session");
    fs::create_dir(&workspace).expect("workspace");
    fs::create_dir(&session_root).expect("session root");

    let rules = rules_for_test(&CapsuleConfig::default(), None, &workspace, &session_root)
        .expect("ordinary workspace cwd must remain valid");
    let workspace = workspace.canonicalize().expect("canonical workspace");
    assert!(
        rules.iter().any(|rule| {
            rule.path == workspace && rule.access == FULL_WITH_UNIX && rule.required
        })
    );
    assert!(
        !rules
            .iter()
            .any(|rule| { rule.path == Path::new("/") && rule.access & support::WRITABLE != 0 })
    );
}

#[test]
fn socket_resolution_is_only_granted_to_non_sensitive_roots_on_abi9() {
    assert_eq!(access_for_abi(FULL_WITH_UNIX, 3), FULL);
    assert_eq!(access_for_abi(FULL_WITH_UNIX, 9), FULL_WITH_UNIX);
    assert_eq!(access_for_abi(READ_ONLY_WITH_UNIX, 3), READ_ONLY);
    assert_eq!(access_for_abi(READ_ONLY_WITH_UNIX, 9), READ_ONLY_WITH_UNIX);
    assert_eq!(
        access_for_abi(super::super::linux::TRAVERSE, 9),
        super::super::linux::TRAVERSE
    );

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
        Path::new("/jackin/run/sessions/1"),
    )
    .expect("construct Landlock rules");
    for socket in [
        jackin_core::container_paths::CAPSULE_SOCKET,
        jackin_core::container_paths::HOST_SOCK,
        jackin_core::container_paths::USAGE_SOCK,
    ] {
        assert!(
            rules.iter().all(|rule| {
                rule.path != Path::new(socket) || rule.access & ACCESS_RESOLVE_UNIX != 0
            }),
            "RPC socket is missing ResolveUnix: {socket}"
        );
    }
    assert!(
        rules.iter().any(|rule| {
            rule.path == Path::new("/workspace/project") && rule.access & ACCESS_RESOLVE_UNIX != 0
        }),
        "workspace must retain local Unix-socket behavior on ABI9"
    );
}

#[test]
fn shared_state_and_tmp_are_not_agent_grants_and_private_root_is_exact() {
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
    let session_root = Path::new("/jackin/run/sessions/7");
    let rules = rules_for(
        &config,
        Some("slot-a"),
        Path::new("/workspace/project"),
        session_root,
    )
    .expect("construct Landlock rules");
    assert!(
        rules
            .iter()
            .any(|rule| { rule.path == session_root && rule.access == FULL_WITH_UNIX })
    );
    assert!(!rules.iter().any(|rule| {
        rule.path == Path::new(jackin_core::container_paths::STATE_DIR)
            && rule.access & support::WRITABLE != 0
    }));
    assert!(
        !rules
            .iter()
            .any(|rule| { rule.path == Path::new("/tmp") && rule.access & support::WRITABLE != 0 })
    );
    assert!(!rules.iter().any(|rule| {
        rule.path == Path::new("/home/agent/.config/gh") && rule.access & support::WRITABLE != 0
    }));
    assert!(
        rules.iter().any(|rule| {
            rule.path == Path::new(jackin_core::container_paths::RUN_DIR)
                && rule.access == super::super::linux::TRAVERSE
        }),
        "run directory must remain traverse-only in the filesystem policy"
    );
    assert_eq!(retained_capability_mask(), 1u32 << 1);
    assert_eq!(retained_capability_mask() & (1u32 << 3), 0);
}

#[test]
fn derived_pane_homes_and_agent_seed_fragment_are_granted() {
    // Secondary same-agent slot: suffixed home, agent default fragment.
    // The seed reads `/jackin/default-home/.codex` for every home shape,
    // so the grant must be keyed by runtime — never by home suffix.
    let config = CapsuleConfig {
        instances: vec!["cx-b-inst".to_owned()],
        agents: BTreeMap::from([("cx-b-inst".to_owned(), "codex".to_owned())]),
        instance_home_dirs: BTreeMap::from([(
            "cx-b-inst".to_owned(),
            "/home/agent/.codex-cx-b-inst".to_owned(),
        )]),
        instance_mount_paths: BTreeMap::from([(
            "cx-b-inst".to_owned(),
            vec!["/home/agent/.codex-cx-b-inst".to_owned()],
        )]),
        ..CapsuleConfig::default()
    };
    let rules = rules_for(
        &config,
        Some("cx-b-inst"),
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/4"),
    )
    .expect("construct Landlock rules for secondary slot");
    let panes = rules
        .iter()
        .find(|rule| rule.path == Path::new("/home/agent/.codex-cx-b-inst/panes"))
        .expect("derived pane-homes parent rule");
    assert_eq!(panes.access, FULL_WITH_UNIX);
    assert!(panes.required);
    let fragment = rules
        .iter()
        .find(|rule| rule.path == Path::new("/jackin/default-home/.codex"))
        .expect("agent seed-fragment rule");
    assert_eq!(fragment.access, READ_ONLY);
    assert!(!fragment.required);
    assert!(!rules.iter().any(|rule| {
        rule.path == Path::new("/jackin/default-home/.codex-cx-b-inst")
            && rule.access != super::super::linux::TRAVERSE
    }));
}

#[test]
fn xdg_parent_home_gets_pane_homes_and_config_fragments() {
    // XDG-parent home: the base mount grants cover only subpaths, so the
    // derived `{home}/panes/{seq}` tree needs its own parent grant.
    let config = CapsuleConfig {
        instances: vec!["oc-c-inst".to_owned()],
        agents: BTreeMap::from([("oc-c-inst".to_owned(), "opencode".to_owned())]),
        instance_home_dirs: BTreeMap::from([(
            "oc-c-inst".to_owned(),
            "/home/agent/.local/share".to_owned(),
        )]),
        instance_mount_paths: BTreeMap::from([(
            "oc-c-inst".to_owned(),
            vec![
                "/home/agent/.local/share/opencode".to_owned(),
                "/home/agent/.cache/opencode".to_owned(),
                "/home/agent/.config/opencode".to_owned(),
            ],
        )]),
        ..CapsuleConfig::default()
    };
    let rules = rules_for(
        &config,
        Some("oc-c-inst"),
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/5"),
    )
    .expect("construct Landlock rules for XDG-parent home");
    let panes = rules
        .iter()
        .find(|rule| rule.path == Path::new("/home/agent/.local/share/panes"))
        .expect("derived pane-homes parent rule");
    assert_eq!(panes.access, FULL_WITH_UNIX);
    assert!(panes.required);
    for fragment in [
        "/jackin/default-home/.local/share/opencode",
        "/jackin/default-home/.config/opencode",
    ] {
        let rule = rules
            .iter()
            .find(|rule| rule.path == Path::new(fragment))
            .unwrap_or_else(|| panic!("seed-fragment rule for {fragment}"));
        assert_eq!(rule.access, READ_ONLY);
        assert!(!rule.required);
    }
    // The parent home itself stays ungranted: only the panes subtree and
    // the instance's own mount subpaths are writable.
    assert!(!rules.iter().any(|rule| {
        rule.path == Path::new("/home/agent/.local/share") && rule.access & support::WRITABLE != 0
    }));
}

#[test]
fn missing_instance_home_fails_rules_closed() {
    let config = CapsuleConfig {
        instances: vec!["slot-a".to_owned()],
        instance_mount_paths: BTreeMap::from([(
            "slot-a".to_owned(),
            vec!["/home/agent/.claude-a".to_owned()],
        )]),
        ..CapsuleConfig::default()
    };
    let error = rules_for(
        &config,
        Some("slot-a"),
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect_err("instance without a home entry must fail closed");
    assert!(error.to_string().contains("no home dir"), "{error:#}");
}

#[test]
fn shell_sessions_carry_no_pane_homes_grant() {
    let rules = rules_for(
        &CapsuleConfig::default(),
        None,
        Path::new("/workspace/project"),
        Path::new("/jackin/run/sessions/1"),
    )
    .expect("construct shell Landlock rules");
    assert!(!rules.iter().any(|rule| {
        rule.path.to_string_lossy().ends_with(&format!(
            "/{}",
            jackin_core::container_paths::PANE_HOMES_DIR_NAME
        )) && rule.access & support::WRITABLE != 0
    }));
}
