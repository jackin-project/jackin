// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn disc_same_env_key_through_two_accounts_dedupes_to_one_source() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[
            ("or-alpha", AiProvider::OpenRouter, "fixture-or-shared-key"),
            ("or-beta", AiProvider::OpenRouter, "fixture-or-shared-key"),
        ],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].provenance.len(), 2);

    let validated = validate_usage_sources(catalog, &resolver);
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].provenance.len(), 2);
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 1);
}

#[test]
fn disc_env_key_without_profile_keeps_own_source_scoped_row() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[("xai-key", AiProvider::Xai, "xai-fixture-secret")],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    let validated = validate_usage_sources(catalog, &resolver);
    assert_eq!(validated.accounts.len(), 1);
    assert!(matches!(
        validated.accounts[0].identity.subject,
        CanonicalAccountSubject::SourceCapability(_)
    ));
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 1);
    assert_eq!(validated.unresolved_capabilities().count(), 0);
}

#[test]
fn disc_env_key_with_two_provider_identities_stays_separate() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let first = temp.path().join("first-profile");
    let second = temp.path().join("second-profile");
    write_accounts_config(
        &config_root,
        &[
            ("codex-one", Agent::Codex, &first),
            ("codex-two", Agent::Codex, &second),
        ],
        &[("codex-key", AiProvider::OpenAi, "fixture-openai-key")],
    );
    write_codex_auth(
        &first,
        "account-one",
        "eyJlbWFpbCI6Im9uZUBleGFtcGxlLnRlc3QifQ",
        "secret-one",
    );
    write_codex_auth(
        &second,
        "account-two",
        "eyJlbWFpbCI6InR3b0BleGFtcGxlLnRlc3QifQ",
        "secret-two",
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    let validated =
        validate_usage_sources_with_reader(catalog, &resolver, &RecordingProfileReader::default());

    // Ambiguous targets are never guessed: the key keeps its own row.
    assert_eq!(validated.accounts.len(), 3);
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 3);
}

#[test]
fn disc_env_key_does_not_attach_to_label_only_profile() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-profile");
    write_accounts_config(
        &config_root,
        &[("codex-profile", Agent::Codex, &profile)],
        &[("codex-key", AiProvider::OpenAi, "fixture-openai-key")],
    );
    // OAuth material without any provider-issued id or label: the profile
    // mints a source-scoped identity, which is not an attach target.
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("auth.json"),
        r#"{"tokens":{"access_token":"fixture-secret"}}"#,
    )
    .unwrap();
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    let validated =
        validate_usage_sources_with_reader(catalog, &resolver, &RecordingProfileReader::default());

    assert_eq!(validated.accounts.len(), 2);
    assert!(validated.accounts.iter().all(|account| matches!(
        account.identity.subject,
        CanonicalAccountSubject::SourceCapability(_)
    )));
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 2);
}

#[test]
fn refresh_gemini_binding_dispatches_to_collector() {
    let temp = tempfile::tempdir().unwrap();
    let creds = temp.path().join("oauth_creds.json");
    std::fs::write(&creds, "{}").unwrap();
    let binding = test_binding(
        HostSurfaceId::Google,
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Gemini {
            creds_path: creds.clone(),
        }),
    );
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
            assert_eq!(view.account.provider_label, "Google");
            assert_eq!(view.focused_agent.as_deref(), Some("gemini"));
        }
        other => panic!("gemini refresh must dispatch to the collector: {other:?}"),
    }
    // A credential file deleted after discovery re-proves as NeedsSecret.
    std::fs::remove_file(&creds).unwrap();
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.status, UsageSnapshotStatus::NeedsSecret);
        }
        other => panic!("deleted gemini creds must need secret: {other:?}"),
    }
}
