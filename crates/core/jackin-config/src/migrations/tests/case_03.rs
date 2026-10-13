// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn config_breadcrumb_migration_preserves_metadata_and_escapes_literal_percent() {
    let input = r#"version = "v1alpha12"

[env.API_TOKEN]
op = "op://vault-id/item-id/section-id/field-id?attribute=username"
path = "Vault/Item/Team%2FBlue/Token?attribute=username"
account = "work"

[github.env.GH_TOKEN]
op = "op://vault-id/github-item/token-field"
path = "Work/Automation/Token"

[roles.builder.env.DEPLOY_TOKEN]
op = "op://vault-id/item-id/deploy-id"
path = "Vault/Builder/Deploy"

[workspaces.dev.env.WORKSPACE_TOKEN]
op = "op://vault-id/workspace-item/workspace-token"
path = "Work/Workspace/Token"

[workspaces.dev.github.env.WORKSPACE_GH_TOKEN]
op = "op://vault-id/workspace-item/github-token"
path = "Work/Workspace/GitHub Token"

[workspaces.dev.roles.builder.env.ROLE_TOKEN]
op = "op://vault-id/role-item/role-token"
path = "Work/Builder/Role Token"

[workspaces.dev.roles.builder.github.env.ROLE_GH_TOKEN]
op = "op://vault-id/role-item/role-gh-token"
path = "Work/Builder/GitHub Token"

[accounts.work.credential]
type = "api_key"
value = { op = "op://vault-id/account-item/api-key", path = "Work/Account/API Key", account = "work" }
"#;
    let mut doc: DocumentMut = input.parse().unwrap();

    migrate_config_op_breadcrumbs(&mut doc).unwrap();

    let token = &doc["env"]["API_TOKEN"];
    assert_eq!(
        token["op"].as_str(),
        Some("op://vault-id/item-id/section-id/field-id?attribute=username")
    );
    assert_eq!(token["account"].as_str(), Some("work"));
    assert!(token.get("path").is_none());
    assert_eq!(token["breadcrumb"]["version"].as_integer(), Some(1));
    assert_eq!(
        token["breadcrumb"]["value"].as_str(),
        Some("Vault/Item/Team%252FBlue/Token?attribute=username")
    );
    assert_eq!(
        doc["roles"]["builder"]["env"]["DEPLOY_TOKEN"]["breadcrumb"]["value"].as_str(),
        Some("Vault/Builder/Deploy")
    );
    assert_eq!(
        doc["github"]["env"]["GH_TOKEN"]["breadcrumb"]["value"].as_str(),
        Some("Work/Automation/Token")
    );
    assert_eq!(
        doc["workspaces"]["dev"]["env"]["WORKSPACE_TOKEN"]["breadcrumb"]["value"].as_str(),
        Some("Work/Workspace/Token")
    );
    assert_eq!(
        doc["workspaces"]["dev"]["github"]["env"]["WORKSPACE_GH_TOKEN"]["breadcrumb"]["value"]
            .as_str(),
        Some("Work/Workspace/GitHub Token")
    );
    assert_eq!(
        doc["workspaces"]["dev"]["roles"]["builder"]["env"]["ROLE_TOKEN"]["breadcrumb"]["value"]
            .as_str(),
        Some("Work/Builder/Role Token")
    );
    assert_eq!(
        doc["workspaces"]["dev"]["roles"]["builder"]["github"]["env"]["ROLE_GH_TOKEN"]
            ["breadcrumb"]["value"]
            .as_str(),
        Some("Work/Builder/GitHub Token")
    );
    assert_eq!(
        doc["accounts"]["work"]["credential"]["value"]["breadcrumb"]["value"].as_str(),
        Some("Work/Account/API Key")
    );
}

#[test]
fn workspace_breadcrumb_migration_rejects_ambiguous_data_without_writing_file() {
    let original = r#"version = "v1alpha10"

[env.TOKEN]
op = "op://vault-id/item-id/field-id"
path = "Vault/Item/Section/Field"
"#;
    let temp = tempdir().unwrap();
    let path = temp.path().join("workspace.toml");
    std::fs::write(&path, original).unwrap();

    let error = migrate_workspace_file_if_needed(&path).unwrap_err();

    assert!(format!("{error:#}").contains("ambiguous"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn workspace_breadcrumb_migration_rejects_later_invalid_entry_without_partial_write() {
    let original = r#"version = "v1alpha10"

[env.A_VALID]
op = "op://vault-id/item-id/field-id"
path = "Vault/Item/Field"

[env.Z_AMBIGUOUS]
op = "op://vault-id/item-id/field-id"
path = "Vault/Item/Section/Field"
"#;
    let temp = tempdir().unwrap();
    let path = temp.path().join("workspace.toml");
    std::fs::write(&path, original).unwrap();

    let error = migrate_workspace_file_if_needed(&path).unwrap_err();

    assert!(format!("{error:#}").contains("ambiguous"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn migration_rejects_invalid_versioned_breadcrumb_before_stamping_current_schema() {
    let original = r#"version = "v1alpha10"

[env.TOKEN]
op = "op://vault-id/item-id/field-id"
breadcrumb = { version = 2, value = "Vault/Item/Field" }
"#;
    let temp = tempdir().unwrap();
    let path = temp.path().join("workspace.toml");
    std::fs::write(&path, original).unwrap();

    let error = migrate_workspace_file_if_needed(&path).unwrap_err();

    assert!(format!("{error:#}").contains("strict environment-value schema"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn migration_rejects_unknown_versioned_breadcrumb_fields_without_stamping() {
    let original = r#"version = "v1alpha10"

[env.TOKEN]
op = "op://vault-id/item-id/field-id"
breadcrumb = { version = 1, value = "Vault/Item/Field", extra = "x" }
"#;
    let temp = tempdir().unwrap();
    let path = temp.path().join("workspace.toml");
    std::fs::write(&path, original).unwrap();

    let error = migrate_workspace_file_if_needed(&path).unwrap_err();

    assert!(format!("{error:#}").contains("strict environment-value schema"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn migration_rejects_legacy_op_ref_unknown_fields_before_stamping() {
    for (name, op_ref) in [
        (
            "unknown-key",
            r#"op = "op://vault-id/item-id/field-id"
path = "Vault/Item/Field"
extra = "x""#,
        ),
        (
            "invalid-account-type",
            r#"op = "op://vault-id/item-id/field-id"
path = "Vault/Item/Field"
account = 1"#,
        ),
    ] {
        let original = format!("version = \"v1alpha10\"\n\n[env.TOKEN]\n{op_ref}\n");
        let temp = tempdir().unwrap();
        let path = temp.path().join(format!("{name}.toml"));
        std::fs::write(&path, &original).unwrap();

        let error = migrate_workspace_file_if_needed(&path).unwrap_err();

        assert!(
            format!("{error:#}").contains("strict environment-value schema"),
            "{name}: {error:#}"
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), original, "{name}");
    }
}
