// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn opencode_binding_uses_selected_xdg_data_and_cache_roots() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    let data = temp.path().join("selected-opencode/data");
    let config = temp.path().join("selected-opencode/config");
    let cache = temp.path().join("selected-opencode/cache");
    std::fs::create_dir_all(data.join("opencode")).unwrap();
    std::fs::create_dir_all(config.join("opencode")).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("opencode/auth.json"),
        r#"{"opencode-go":{"type":"api","key":"fixture-key"}}"#,
    )
    .unwrap();

    let mut binding = InstanceAuthBinding::new(
        "selected",
        jackin_core::Agent::Opencode,
        AuthForwardMode::Sync,
        None,
    );
    binding.xdg_roots = Some(jackin_config::XdgRoots {
        data,
        config,
        cache: cache.clone(),
    });
    let (state, _) = RoleState::prepare_for_bindings(
        &paths,
        "jk-selected-opencode",
        &manifest,
        &[binding],
        &GithubAuthContext::default(),
        temp.path().join("host-home").as_path(),
        jackin_core::Agent::Opencode,
    )
    .unwrap();

    let slot = state
        .auth
        .slots
        .get("selected@opencode")
        .expect("selected OpenCode slot missing");
    assert_eq!(slot.cache_source_dir.as_deref(), Some(cache.as_path()));
    assert_eq!(slot.container_cache_rel.as_deref(), Some(".cache/opencode"));
    let staged: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(state.root.join("opencode/auth.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        staged
            .pointer("/opencode-go/type")
            .and_then(|value| value.as_str()),
        Some("api")
    );
    assert_eq!(
        staged
            .pointer("/opencode-go/key")
            .and_then(|value| value.as_str()),
        Some("fixture-key")
    );
}

#[cfg(unix)]
#[test]
fn xdg_cache_overlap_rejects_parent_traversal_and_symlink_aliases() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let real = temp.path().join("real-cache");
    let alias = temp.path().join("cache-alias");
    std::fs::create_dir_all(&real).unwrap();
    symlink(&real, &alias).unwrap();

    let binding_for = |key: &str, cache: PathBuf| {
        let mut binding =
            InstanceAuthBinding::new(key, jackin_core::Agent::Amp, AuthForwardMode::Ignore, None);
        binding.xdg_roots = Some(jackin_config::XdgRoots {
            data: temp.path().join(format!("{key}-data")),
            config: temp.path().join(format!("{key}-config")),
            cache,
        });
        binding
    };

    let error = validate_selected_account_sources(
        &[
            binding_for("first", real.clone()),
            binding_for("second", alias),
        ],
        temp.path(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("overlap"), "{error:#}");

    let traversal = real.join("..").join("real-cache");
    let error =
        validate_selected_account_sources(&[binding_for("traversal", traversal)], temp.path())
            .unwrap_err();
    assert!(error.to_string().contains("parent traversal"), "{error:#}");
}

#[test]
fn colliding_sanitized_suffixes_get_numeric_tails() {
    // `a@b` and `a-b` sanitize identically; secondary slots must still
    // land in distinct dirs.
    let binding_for = |key: &str| {
        let mut binding = InstanceAuthBinding::new(
            "work",
            jackin_core::Agent::Claude,
            AuthForwardMode::Ignore,
            None,
        );
        binding.key = key.to_owned();
        binding
    };
    let bindings = [
        binding_for("primary"),
        binding_for("a@b"),
        binding_for("a-b"),
    ];
    let suffixes = slot_suffixes(&bindings);
    assert_eq!(suffixes[0], None);
    assert_eq!(suffixes[1].as_deref(), Some("a-b"));
    assert_eq!(suffixes[2].as_deref(), Some("a-b-2"));
    // Dedupe is per agent: a codex secondary keeps `a-b` even
    // though a claude secondary already owns it; store dirs are per
    // agent so they cannot collide.
    let mut codex_primary = binding_for("codex-primary");
    codex_primary.agent = jackin_core::Agent::Codex;
    let mut codex_secondary = binding_for("a@b");
    codex_secondary.agent = jackin_core::Agent::Codex;
    let mixed = [
        bindings[0].clone(),
        bindings[1].clone(),
        codex_primary,
        codex_secondary,
    ];
    let suffixes = slot_suffixes(&mixed);
    assert_eq!(suffixes[0], None);
    assert_eq!(suffixes[1].as_deref(), Some("a-b"));
    assert_eq!(suffixes[2], None);
    assert_eq!(suffixes[3].as_deref(), Some("a-b"));
}

#[test]
fn prepare_for_bindings_honors_explicit_config_id_keys() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    let mut binding = InstanceAuthBinding::new(
        "work",
        jackin_core::Agent::Claude,
        AuthForwardMode::Ignore,
        None,
    );
    binding.key = "work-claude".to_owned();

    let (state, _) = RoleState::prepare_for_bindings(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        std::slice::from_ref(&binding),
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::Agent::Claude,
    )
    .unwrap();

    assert_eq!(state.auth.slots.len(), 1);
    let slot = state
        .auth
        .slots
        .get("work-claude")
        .expect("explicit key lost");
    assert_eq!(slot.account_id, "work");
    // Agent-scoped lookups still resolve through the explicit key.
    assert!(state.claude_account_json().is_some());
    assert!(state.claude_credentials_json().is_some());
}

#[test]
fn prewarm_auth_for_bindings_provisions_each_binding_once() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let bindings = vec![
        InstanceAuthBinding::new(
            "work",
            jackin_core::Agent::Codex,
            AuthForwardMode::Ignore,
            None,
        ),
        InstanceAuthBinding::new(
            "personal",
            jackin_core::Agent::Codex,
            AuthForwardMode::Ignore,
            None,
        ),
    ];

    let count = RoleState::prewarm_auth_for_bindings(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &bindings,
        temp.path(),
    )
    .unwrap();

    assert_eq!(count, 2);
}
