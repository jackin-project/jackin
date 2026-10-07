// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn disc_kimi_missing_selected_credentials_never_uses_other_home_profile() {
    let temp = tempfile::tempdir().unwrap();
    let selected = temp.path().join("selected");
    let home = temp.path().join("home");
    let ambient = home.join(".kimi/credentials");
    let config_root = temp.path().join("config");
    std::fs::create_dir_all(&selected).unwrap();
    std::fs::create_dir_all(&ambient).unwrap();
    std::fs::write(
        ambient.join("kimi-code.json"),
        r#"{"access_token":"ambient-secret"}"#,
    )
    .unwrap();
    write_registry(&config_root, &[("kimi", Agent::Kimi, &selected)]);
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: home,
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(validated.bindings.is_empty());
    assert!(
        validated
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.issue == UsageDiscoveryIssue::CredentialMissing)
    );
}

#[test]
fn disc_amp_composite_profile_reads_selected_data_root() {
    let temp = tempfile::tempdir().unwrap();
    let selected = temp.path().join("amp-work");
    let data = selected.join("data/amp");
    let config_root = temp.path().join("config");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("secrets.json"),
        r#"{"apiKey@work@example.test":"selected-secret"}"#,
    )
    .unwrap();
    write_registry(&config_root, &[("amp-work", Agent::Amp, &selected)]);
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].account_label, "work@example.test");
    assert!(!format!("{validated:?}").contains("selected-secret"));
}

#[test]
fn disc_dedup_repeated_roots_read_once_and_same_identity_merges() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let workspaces = config_root.join("workspaces");
    let shared = temp.path().join("shared-profile");
    let second = temp.path().join("second-profile");
    write_codex_only_global(&config_root, &shared);
    std::fs::create_dir_all(&workspaces).unwrap();
    write_codex_workspace(&workspaces.join("first.toml"), &shared);
    write_codex_workspace(&workspaces.join("second.toml"), &second);
    for (root, token) in [(&shared, "secret-one"), (&second, "secret-two")] {
        write_codex_auth(
            root,
            "same-provider-account",
            "eyJlbWFpbCI6InNhbWVAZXhhbXBsZS50ZXN0In0",
            token,
        );
    }
    let reader = RecordingProfileReader::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    assert_eq!(
        catalog
            .candidates
            .iter()
            .filter(|candidate| candidate.surface_id == "codex")
            .count(),
        2
    );
    let capability_ids = catalog
        .candidates
        .iter()
        .map(|candidate| candidate.capability_id.clone())
        .collect::<Vec<_>>();
    assert!(
        capability_ids
            .iter()
            .all(|capability_id| capability_id.len() == 64),
        "source capability ids must be stable opaque hashes: {capability_ids:?}"
    );
    let rediscovered = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    assert_eq!(
        capability_ids,
        rediscovered
            .candidates
            .iter()
            .map(|candidate| candidate.capability_id.clone())
            .collect::<Vec<_>>()
    );

    let validated = validate_usage_sources_with_reader(catalog, &NoEnvResolver, &reader);

    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].source_ids.len(), 2);
    assert_eq!(
        reader.reads.lock().unwrap().get(&shared.join("auth.json")),
        Some(&1)
    );
    assert_eq!(
        reader.reads.lock().unwrap().get(&second.join("auth.json")),
        Some(&1)
    );
    assert!(
        validated.accounts[0]
            .provenance
            .iter()
            .any(|scope| scope == "account codex")
    );
    assert!(
        validated.accounts[0]
            .provenance
            .iter()
            .any(|scope| scope == "workspace first")
    );
}

#[test]
fn disc_cursor_token_profile_binds_refreshable_material() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let cursor_root = temp.path().join("cursor-work");
    std::fs::create_dir_all(&cursor_root).unwrap();
    write_registry(
        &config_root,
        &[("cursor-work", Agent::Cursor, &cursor_root)],
    );
    std::fs::write(
        cursor_root.join("auth.json"),
        r#"{"accessToken":"fixture-token","refreshToken":"fixture-refresh"}"#,
    )
    .unwrap();
    std::fs::write(
        cursor_root.join("cli-config.json"),
        r#"{"authInfo":{"email":"work@example.test"}}"#,
    )
    .unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].account_label, "work@example.test");
    assert_eq!(validated.bindings.len(), 1);
    match &validated.bindings[0].source {
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Cursor { auth_path }) => {
            assert_eq!(auth_path, &cursor_root.join("auth.json"));
        }
        _ => panic!("cursor token profile must bind refreshable material"),
    }
}

#[test]
fn disc_cursor_tokenless_profile_is_malformed_without_binding() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let cursor_root = temp.path().join("cursor-bare");
    std::fs::create_dir_all(&cursor_root).unwrap();
    write_registry(
        &config_root,
        &[("cursor-bare", Agent::Cursor, &cursor_root)],
    );
    std::fs::write(
        cursor_root.join("auth.json"),
        r#"{"refreshToken":"only-refresh"}"#,
    )
    .unwrap();
    std::fs::write(
        cursor_root.join("cli-config.json"),
        r#"{"authInfo":{"email":"bare@example.test"}}"#,
    )
    .unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(validated.bindings.is_empty());
    assert!(validated.accounts.is_empty());
    assert_eq!(validated.diagnostics.len(), 1);
    assert!(matches!(
        validated.diagnostics[0].issue,
        UsageDiscoveryIssue::CredentialMalformed
    ));
}

#[test]
fn disc_cursor_profile_mints_material_with_cli_identity() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cursor");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("auth.json"),
        r#"{"accessToken":"fixture-opaque-token"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("cli-config.json"),
        r#"{"authInfo":{"email":"cursor@example.test"}}"#,
    )
    .unwrap();
    let reader = RecordingProfileReader::default();

    let ProfileValidation::Authenticated {
        account_label,
        material: Some(material),
        ..
    } = profile_identity(&reader, Agent::Cursor, &root, temp.path())
    else {
        panic!("cursor profile must authenticate with material");
    };
    assert_eq!(account_label.as_deref(), Some("cursor@example.test"));
    assert!(matches!(
        *material,
        ProfileCredentialMaterial::Cursor { .. }
    ));
}

#[test]
fn disc_cursor_profile_tokenless_is_malformed_and_missing_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cursor");
    std::fs::create_dir_all(&root).unwrap();
    let reader = RecordingProfileReader::default();

    assert!(matches!(
        profile_identity(&reader, Agent::Cursor, &root, temp.path()),
        ProfileValidation::Missing
    ));

    std::fs::write(root.join("auth.json"), r#"{"noToken":true}"#).unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Cursor, &root, temp.path()),
        ProfileValidation::Malformed
    ));
}

#[test]
fn disc_gemini_profile_mints_material_with_or_without_label() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("gemini");
    std::fs::create_dir_all(&root).unwrap();
    let reader = RecordingProfileReader::default();

    assert!(matches!(
        profile_identity(&reader, Agent::Gemini, &root, temp.path()),
        ProfileValidation::Missing
    ));

    std::fs::write(
        root.join("oauth_creds.json"),
        r#"{"user_email":"g@example.test"}"#,
    )
    .unwrap();
    let ProfileValidation::Authenticated {
        account_label,
        material: Some(material),
        ..
    } = profile_identity(&reader, Agent::Gemini, &root, temp.path())
    else {
        panic!("gemini profile must authenticate with material");
    };
    assert_eq!(account_label.as_deref(), Some("g@example.test"));
    assert!(matches!(
        *material,
        ProfileCredentialMaterial::Gemini { .. }
    ));

    std::fs::write(root.join("oauth_creds.json"), "{}").unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Gemini, &root, temp.path()),
        ProfileValidation::Anonymous(Some(_))
    ));

    std::fs::write(root.join("oauth_creds.json"), b"{invalid").unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Gemini, &root, temp.path()),
        ProfileValidation::Malformed
    ));
}
