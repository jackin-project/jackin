// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn instance_bindings_carry_roots_only_for_selected_instances() {
    let roots = jackin_config::XdgRoots {
        data: "/selected/data".into(),
        config: "/selected/config".into(),
        cache: "/selected/cache".into(),
    };
    let mut config = AppConfig::default();
    for account_id in ["selected", "unselected"] {
        config.accounts.insert(
            account_id.into(),
            AccountConfig {
                enabled: true,
                name: account_id.into(),
                provider: AiProvider::Amp,
                credential: AccountCredential::Profile {
                    agent: Agent::Amp,
                    directory: format!("/{account_id}/amp").into(),
                    xdg_roots: Some(roots.clone()),
                    source_selector: None,
                },
            },
        );
    }

    let mut selected = instance("amp-selected", Agent::Amp, "selected");
    selected.xdg_roots = Some(roots.clone());
    let bindings = instance_auth_bindings(&config, &[selected]).unwrap();

    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].account_id, "selected");
    assert_eq!(bindings[0].xdg_roots, Some(roots));
    assert!(
        !bindings
            .iter()
            .any(|binding| binding.account_id == "unselected")
    );
}

#[test]
fn instance_dirs_come_from_slots_and_fail_closed() {
    use jackin_instance::ProvisionedInstanceAuth;
    let slot = |suffix: Option<&str>, home_rel: &str, store_rel: &str| ProvisionedInstanceAuth {
        agent: Agent::Claude,
        account_id: "work".into(),
        mode: AuthForwardMode::Sync,
        home_dir: None,
        credential_paths: Vec::new(),
        forward_auth: true,
        slot_suffix: suffix.map(str::to_owned),
        container_home_rel: home_rel.into(),
        container_store_rel: store_rel.into(),
        folder_target: format!("/home/agent/{home_rel}"),
        cache_source_dir: None,
        container_cache_rel: None,
    };
    let slots = std::collections::BTreeMap::from([
        ("claude-work".to_owned(), slot(None, ".claude", "claude")),
        (
            "claude-personal".to_owned(),
            slot(
                Some("claude-personal"),
                ".claude-claude-personal",
                "claude-claude-personal",
            ),
        ),
    ]);
    let instances = vec![
        instance("claude-work", Agent::Claude, "work"),
        instance("claude-personal", Agent::Claude, "personal"),
    ];
    let mut config = jackin_protocol::CapsuleConfig::default();
    apply_instance_dirs(&mut config, &instances, &slots).unwrap();
    assert_eq!(
        config.home_for_instance("claude-work"),
        Some("/home/agent/.claude")
    );
    assert_eq!(
        config.home_for_instance("claude-personal"),
        Some("/home/agent/.claude-claude-personal")
    );
    assert_eq!(
        config.forwarded_for_instance("claude-work"),
        Some("/jackin/claude")
    );
    assert_eq!(
        config.forwarded_for_instance("claude-personal"),
        Some("/jackin/claude-claude-personal")
    );
    assert_eq!(
        config.credential_file_for_instance("claude-work"),
        Some("/jackin/account-credentials/acct-636c617564652d776f726b.json")
    );
    assert_eq!(
        config.identity_for_instance("claude-work").unwrap().uid,
        2_000
    );
    assert_eq!(
        config.identity_for_instance("claude-personal").unwrap().uid,
        2_001
    );
    assert_eq!(config.shell_identity.unwrap().uid, 2_002);
    assert!(
        config
            .mount_paths_for_instance("claude-work")
            .iter()
            .all(|path| !path.starts_with(jackin_protocol::ACCOUNT_CREDENTIALS_DIR))
    );

    let mut config = jackin_protocol::CapsuleConfig::default();
    let missing = vec![instance("ghost", Agent::Claude, "work")];
    apply_instance_dirs(&mut config, &missing, &slots).unwrap_err();
}

#[test]
fn amp_instance_dir_exports_the_durable_data_parent() {
    use jackin_instance::ProvisionedInstanceAuth;

    let slot = ProvisionedInstanceAuth {
        agent: Agent::Amp,
        account_id: "amp".into(),
        mode: AuthForwardMode::Sync,
        home_dir: None,
        credential_paths: Vec::new(),
        forward_auth: true,
        slot_suffix: None,
        container_home_rel: ".local/share/amp".into(),
        container_store_rel: "amp".into(),
        folder_target: "/home/agent/.local/share".into(),
        cache_source_dir: Some("/tmp/amp-cache".into()),
        container_cache_rel: Some(".cache/amp".into()),
    };
    let slots = std::collections::BTreeMap::from([("amp".to_owned(), slot)]);
    let instances = vec![instance("amp", Agent::Amp, "amp")];
    let mut config = jackin_protocol::CapsuleConfig::default();

    apply_instance_dirs(&mut config, &instances, &slots).unwrap();

    assert_eq!(
        config.home_for_instance("amp"),
        Some("/home/agent/.local/share")
    );
    assert_eq!(config.forwarded_for_instance("amp"), Some("/jackin/amp"));
    assert_eq!(
        config.cache_for_instance("amp"),
        Some("/home/agent/.cache/amp")
    );
    assert!(
        config
            .mount_paths_for_instance("amp")
            .contains(&"/home/agent/.cache/amp".to_owned())
    );
}

#[test]
fn openrouter_account_pin_lands_byte_exact_in_capsule_models() {
    const PIN: &str = "openrouter/anthropic/claude-sonnet-4";
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        "version = \"v1alpha5\"\ndockerfile = \"Dockerfile\"\nagents = [\"opencode\"]\n\n[opencode]\nmodel = \"opencode/big-pickle\"\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(temp.path()).unwrap();

    let mut config = AppConfig::default();
    config.accounts.insert(
        "or-model".into(),
        api_key_account(AiProvider::OpenRouter, Some(PIN)),
    );
    let mut pinned = instance("oc-plain", Agent::Opencode, "or-model");
    pinned.model = Some(PIN.to_owned());

    let models =
        resolved_instance_models(&config, &manifest, &[pinned], Agent::Opencode, None).unwrap();
    assert_eq!(
        models.get("oc-plain").map(String::as_str),
        Some(PIN),
        "the account pin must replace the role default without rewriting"
    );
}
