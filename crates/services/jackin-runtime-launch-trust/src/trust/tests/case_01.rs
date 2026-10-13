// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn mise_env_collects_workspace_paths_sorted_and_skips_blanks() {
    let workspace = workspace("/work", &["/work/deep", "  ", "/extra"]);
    let mut vars = Vec::new();
    inject_workspace_mise_env(&mut vars, &workspace);
    assert_eq!(
        vars,
        vec![(
            MISE_TRUSTED_CONFIG_PATHS_ENV.to_owned(),
            "/extra:/work:/work/deep".to_owned(),
        )]
    );
}

#[test]
fn mise_env_never_overrides_an_explicit_operator_value() {
    let workspace = workspace("/work", &["/extra"]);
    let mut vars = vec![(
        MISE_TRUSTED_CONFIG_PATHS_ENV.to_owned(),
        "/operator".to_owned(),
    )];
    inject_workspace_mise_env(&mut vars, &workspace);
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].1, "/operator");
}

#[test]
fn mise_env_stays_absent_without_trustable_paths() {
    let workspace = workspace("   ", &[""]);
    let mut vars = Vec::new();
    inject_workspace_mise_env(&mut vars, &workspace);
    assert!(vars.is_empty());
}

#[test]
fn codex_trust_seeds_each_workspace_path_and_preserves_existing_keys() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let config_dir = root.join("home").join(".codex");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.toml"), "model = \"fixture\"\n").unwrap();
    let state = codex_state(&root);

    seed_codex_project_trust(&state, &workspace("/work", &["/extra"])).unwrap();

    let raw = std::fs::read_to_string(config_dir.join("config.toml")).unwrap();
    assert!(raw.contains("model = \"fixture\""), "{raw}");
    let doc: toml::Value = toml::from_str(&raw).unwrap();
    for path in ["/work", "/extra"] {
        assert_eq!(
            doc["projects"][path]["trust_level"].as_str(),
            Some("trusted"),
            "{raw}"
        );
    }
}

#[test]
fn codex_trust_is_a_noop_without_paths_or_codex_slots() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let state = codex_state(&root);
    seed_codex_project_trust(&state, &workspace("  ", &[])).unwrap();
    assert!(!root.join("home").exists());

    let mut claude_only = codex_state(&root);
    claude_only.auth.slots.get_mut("acct@codex").unwrap().agent = Agent::Claude;
    seed_codex_project_trust(&claude_only, &workspace("/work", &[])).unwrap();
    assert!(!root.join("home").exists());
}

#[test]
fn codex_trust_coerces_a_non_table_projects_value() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let config_dir = root.join("home").join(".codex");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.toml"), "projects = \"stale\"\n").unwrap();
    let state = codex_state(&root);

    seed_codex_project_trust(&state, &workspace("/work", &[])).unwrap();

    let raw = std::fs::read_to_string(config_dir.join("config.toml")).unwrap();
    let doc: toml::Value = toml::from_str(&raw).unwrap();
    assert_eq!(
        doc["projects"]["/work"]["trust_level"].as_str(),
        Some("trusted"),
        "{raw}"
    );
}
