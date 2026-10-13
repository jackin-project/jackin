// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn applies_multi_step_chain_in_order_to_alpha3() {
    let old = parse_version("v1alpha1").unwrap();
    let current = parse_version("v1alpha3").unwrap();
    let migrations = [
        MigrationStep {
            from: "v1alpha1",
            to: "v1alpha2",
            migrate: alpha1_to_alpha2,
        },
        MigrationStep {
            from: "v1alpha2",
            to: "v1alpha3",
            migrate: alpha2_to_alpha3,
        },
    ];
    let mut doc = DocumentMut::new();

    apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap();

    assert_eq!(doc["alpha1_to_alpha2"].as_bool(), Some(true));
    assert_eq!(doc["alpha2_to_alpha3"].as_bool(), Some(true));
    assert_eq!(doc["version"].as_str(), Some("v1alpha3"));
}

#[test]
fn op_account_moves_onto_each_op_ref_and_top_level_key_removed() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("prod.toml");
    std::fs::write(
        &path,
        r#"version = "v1alpha4"
workdir = "/workspace/prod"
op_account = "ACCT123"

[env]
TOKEN = { op = "op://v/i/f", path = "V/I/F" }
PLAIN = "literal"

[github.env]
GH = { op = "op://gv/gi/gf", path = "GV/GI/GF" }

[roles."org/agent".env]
RT = { op = "op://rv/ri/rf", path = "RV/RI/RF" }

[roles."org/agent".github.env]
RG = { op = "op://rgv/rgi/rgf", path = "RGV/RGI/RGF" }
"#,
    )
    .unwrap();

    assert!(migrate_workspace_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();

    assert_eq!(
        parsed["version"].as_str().unwrap(),
        CURRENT_WORKSPACE_VERSION
    );
    assert!(
        !out.contains("op_account"),
        "top-level key must be gone:\n{out}"
    );
    assert_eq!(parsed["env"]["TOKEN"]["account"].as_str(), Some("ACCT123"));
    assert!(
        parsed["env"]["PLAIN"].as_str() == Some("literal"),
        "plain string untouched:\n{out}"
    );
    assert_eq!(
        parsed["github"]["env"]["GH"]["account"].as_str(),
        Some("ACCT123")
    );
    assert_eq!(
        parsed["roles"]["org/agent"]["env"]["RT"]["account"].as_str(),
        Some("ACCT123")
    );
    assert_eq!(
        parsed["roles"]["org/agent"]["github"]["env"]["RG"]["account"].as_str(),
        Some("ACCT123")
    );
}

#[test]
fn workspace_without_op_account_leaves_refs_unaccounted() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("prod.toml");
    std::fs::write(
        &path,
        r#"version = "v1alpha4"
workdir = "/workspace/prod"

[env]
TOKEN = { op = "op://v/i/f", path = "V/I/F" }
"#,
    )
    .unwrap();

    assert!(migrate_workspace_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    assert!(
        !out.contains("account"),
        "no account key without op_account:\n{out}"
    );
}

#[test]
fn workspace_with_non_string_op_account_bails_loudly() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("prod.toml");
    std::fs::write(
        &path,
        r#"version = "v1alpha4"
workdir = "/workspace/prod"
op_account = 123

[env]
TOKEN = { op = "op://v/i/f", path = "V/I/F" }
"#,
    )
    .unwrap();

    let err = migrate_workspace_file_if_needed(&path).unwrap_err();
    // The framework wraps the step error with a "running … migration"
    // context, so check the full chain (alternate Display) for our message.
    let chain = format!("{err:#}");
    assert!(
        chain.contains("op_account") && chain.contains("must be a string"),
        "non-string op_account must bail loudly, not silently drop: {chain}"
    );
}

#[test]
fn version_field_is_migrated_to_first_line() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("prod.toml");
    std::fs::write(&path, "workdir = \"/workspace/prod\"\n# trailing comment\n").unwrap();

    assert!(migrate_workspace_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    assert!(
        out.starts_with(&format!("version = \"{CURRENT_WORKSPACE_VERSION}\"")),
        "{out}"
    );
    assert!(out.contains("workdir = \"/workspace/prod\""), "{out}");
    assert!(out.contains("# trailing comment"), "{out}");
}

#[test]
fn prop_config_migration_idempotent() {
    use proptest::prelude::*;

    let versions = [
        "v1alpha1",
        "v1alpha2",
        "v1alpha3",
        "v1alpha4",
        "v1alpha5",
        "v1alpha6",
        "v1alpha7",
        "v1alpha8",
        "v1alpha9",
        "v1alpha10",
    ];
    proptest!(|(idx in 0usize..versions.len())| {
        let version = versions[idx];
        let temp = tempdir().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(
            &path,
            format!(
                "version = \"{version}\"\n\n[roles.agent-smith]\ngit = \"https://example.test/role.git\"\n"
            ),
        )
        .unwrap();

        let first_run = migrate_config_file_if_needed(&path);
        prop_assert!(first_run.is_ok(), "first migrate: {:?}", first_run.err());
        let first = std::fs::read_to_string(&path).unwrap();
        let second_run = migrate_config_file_if_needed(&path);
        prop_assert!(second_run.is_ok(), "second migrate: {:?}", second_run.err());
        prop_assert!(!second_run.unwrap(), "second migrate must be a no-op");
        let second = std::fs::read_to_string(&path).unwrap();
        prop_assert_eq!(&first, &second);
        let parsed: toml::Value = toml::from_str(&second).unwrap();
        prop_assert_eq!(
            parsed["version"].as_str().unwrap(),
            CURRENT_CONFIG_VERSION
        );
    });
}

#[test]
fn prop_workspace_migration_idempotent() {
    use proptest::prelude::*;

    let versions = [
        "v1alpha1", "v1alpha2", "v1alpha3", "v1alpha4", "v1alpha5", "v1alpha6", "v1alpha7",
        "v1alpha8", "v1alpha9",
    ];
    proptest!(|(idx in 0usize..versions.len())| {
        let version = versions[idx];
        let temp = tempdir().unwrap();
        let path = temp.path().join("ws.toml");
        std::fs::write(
            &path,
            format!("version = \"{version}\"\nworkdir = \"/workspace/x\"\n"),
        )
        .unwrap();

        let first_run = migrate_workspace_file_if_needed(&path);
        prop_assert!(first_run.is_ok(), "first migrate: {:?}", first_run.err());
        let first = std::fs::read_to_string(&path).unwrap();
        let second_run = migrate_workspace_file_if_needed(&path);
        prop_assert!(second_run.is_ok());
        prop_assert!(!second_run.unwrap());
        let second = std::fs::read_to_string(&path).unwrap();
        prop_assert_eq!(&first, &second);
    });
}

#[test]
fn account_schema_strips_old_policies_from_config() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let original = "version = \"v1alpha9\"\n[claude]\nauth_forward = \"sync\"\n";
    std::fs::write(&path, original).unwrap();
    assert!(migrate_config_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(parsed["version"].as_str().unwrap(), CURRENT_CONFIG_VERSION);
    assert!(
        !out.contains("claude"),
        "claude table must be stripped: {out}"
    );
}

#[test]
fn account_schema_strips_role_policy_from_workspace() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("workspace.toml");
    let original = "version = \"v1alpha8\"\nworkdir = \"/workspace\"\n[roles.builder.codex]\nauth_forward = \"sync\"\n";
    std::fs::write(&path, original).unwrap();
    assert!(migrate_workspace_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(
        parsed["version"].as_str().unwrap(),
        CURRENT_WORKSPACE_VERSION
    );
    assert!(
        !out.contains("codex"),
        "role codex table must be stripped: {out}"
    );
    assert!(out.contains("workdir = \"/workspace\""), "{out}");
}

#[test]
fn account_schema_preserves_existing_registry_and_assignments() {
    let mut doc: DocumentMut = "version = \"v1alpha8\"\nworkdir = \"/workspace\"\naccounts = [\"personal\"]\n[account_bindings]\ncodex = \"personal\"\n".parse().unwrap();
    strip_legacy_agent_tables(&mut doc).unwrap();
    assert_eq!(doc["account_bindings"]["codex"].as_str(), Some("personal"));
}

#[test]
fn migrates_config_with_top_level_and_role_legacy_agent_tables_to_current() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let original = r#"version = "v1alpha9"

[claude]
auth_forward = "sync"

[roles.builder]
git = "https://example.test/builder.git"

[roles.builder.codex]
auth_forward = "sync"
"#;
    std::fs::write(&path, original).unwrap();
    assert!(migrate_config_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(parsed["version"].as_str().unwrap(), CURRENT_CONFIG_VERSION);
    assert!(
        !out.contains("claude"),
        "top-level [claude] must be stripped:\n{out}"
    );
    assert!(
        !out.contains("codex"),
        "role [codex] must be stripped:\n{out}"
    );
    assert!(
        out.contains("builder.git"),
        "role git url must be preserved:\n{out}"
    );
}

#[test]
fn v1alpha10_to_current_stamps_initialized_sentinel_without_touching_accounts() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let original = "version = \"v1alpha10\"\n\n[accounts.work]\nenabled = true\nname = \"Work\"\nprovider = \"anthropic\"\n\n[accounts.work.credential]\ntype = \"api_key\"\nvalue = \"${ANTHROPIC_API_KEY}\"\n";
    std::fs::write(&path, original).unwrap();
    assert!(migrate_config_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(parsed["version"].as_str().unwrap(), CURRENT_CONFIG_VERSION);
    assert_eq!(parsed["bootstrap"]["version"].as_integer(), Some(1));
    assert_eq!(parsed["bootstrap"]["fresh_install"].as_bool(), Some(false));
    assert_eq!(parsed["accounts"]["work"]["name"].as_str(), Some("Work"));
    // Second run is a no-op (idempotent, never rescans).
    assert!(!migrate_config_file_if_needed(&path).unwrap());
}

#[test]
fn migration_preserves_installer_fresh_install_marker() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(
        &path,
        "version = \"v1alpha10\"\n\n[bootstrap]\nversion = 1\nfresh_install = true\n",
    )
    .unwrap();
    assert!(migrate_config_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(parsed["version"].as_str().unwrap(), CURRENT_CONFIG_VERSION);
    assert_eq!(parsed["bootstrap"]["fresh_install"].as_bool(), Some(true));
}
