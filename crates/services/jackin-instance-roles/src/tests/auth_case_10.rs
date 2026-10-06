// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn sync_falls_back_to_hosts_yml_file_when_gh_binary_absent() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_filebased");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();

    match &outcome {
        GithubProvisionOutcome::Synced { token, source } => {
            assert_eq!(token, "ghp_filebased");
            assert_eq!(*source, GithubTokenSource::HostsFile);
        }
        other => panic!("expected Synced, got {other:?}"),
    }
    assert_eq!(outcome.token(), Some("ghp_filebased"));
    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
    let written = std::fs::read_to_string(&hosts_yml).unwrap();
    assert!(written.contains("oauth_token: ghp_filebased"));
    assert!(written.contains("git_protocol: https"));
    assert!(written.contains("user: alice"));
}

#[test]
fn sync_returns_host_missing_when_neither_source_resolves() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("host_home_with_no_gh_state");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();

    assert_eq!(
        outcome,
        GithubProvisionOutcome::HostMissing {
            reason: HostMissingReason::NoGhAndNoHostsFile
        }
    );
    assert!(outcome.token().is_none());
    assert!(!hosts_yml.exists());
}

#[test]
fn sync_preserves_existing_role_hosts_yml_when_host_lacks_token() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("empty_host_home");
    let hosts_yml = temp.path().join("role-state-hosts.yml");
    std::fs::write(&hosts_yml, "github.com:\n    oauth_token: in_container\n").unwrap();

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();

    assert_eq!(outcome.kind(), GithubProvisionKind::HostMissing);
    let preserved = std::fs::read_to_string(&hosts_yml).unwrap();
    assert!(
        preserved.contains("in_container"),
        "in-container login state must survive sync-with-no-host"
    );
}

#[test]
fn token_mode_wipes_role_hosts_yml() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("host_home");
    let hosts_yml = temp.path().join("role-state-hosts.yml");
    std::fs::write(&hosts_yml, "github.com:\n    oauth_token: stale\n").unwrap();

    let outcome = provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Token, Some("ghp_token")),
        &host_home,
    )
    .unwrap();

    assert_eq!(
        outcome,
        GithubProvisionOutcome::TokenMode {
            token: "ghp_token".to_owned()
        }
    );
    assert_eq!(outcome.token(), Some("ghp_token"));
    assert!(
        !hosts_yml.exists(),
        "token mode must wipe role-state hosts.yml"
    );
}

#[test]
fn ignore_mode_wipes_role_hosts_yml() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("host_home");
    let hosts_yml = temp.path().join("role-state-hosts.yml");
    std::fs::write(&hosts_yml, "github.com:\n    oauth_token: stale\n").unwrap();

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Ignore, None), &host_home).unwrap();

    assert_eq!(outcome, GithubProvisionOutcome::Skipped);
    assert!(outcome.token().is_none());
    assert!(!hosts_yml.exists());
}

#[test]
fn switching_from_sync_to_token_wipes_synced_hosts_yml() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_synced");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
    assert!(hosts_yml.exists());

    let outcome = provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Token, Some("ghp_scoped")),
        &host_home,
    )
    .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::TokenMode);
    assert!(!hosts_yml.exists());
}

#[test]
fn switching_from_sync_to_ignore_wipes_synced_hosts_yml() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_synced");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Ignore, None), &host_home).unwrap();
    assert_eq!(outcome, GithubProvisionOutcome::Skipped);
    assert!(!hosts_yml.exists());
}

#[test]
fn round_trip_ignore_sync_token_ignore_state_clean() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_round");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Ignore, None), &host_home).unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Skipped);
    assert!(!hosts_yml.exists());

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
    assert!(hosts_yml.exists());

    let outcome = provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Token, Some("scoped")),
        &host_home,
    )
    .unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::TokenMode);
    assert!(!hosts_yml.exists());

    let outcome =
        provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Ignore, None), &host_home).unwrap();
    assert_eq!(outcome.kind(), GithubProvisionKind::Skipped);
    assert!(!hosts_yml.exists());
}

#[test]
fn sync_idempotent_skips_write_when_content_unchanged() {
    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_unchanged");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();
    let forced_mtime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    #[expect(
        clippy::disallowed_methods,
        reason = "test fixture forces mtime on an already-created hosts.yml file"
    )]
    std::fs::File::options()
        .write(true)
        .open(&hosts_yml)
        .unwrap()
        .set_modified(forced_mtime)
        .unwrap();
    let mtime_first = std::fs::metadata(&hosts_yml).unwrap().modified().unwrap();

    provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();
    let mtime_second = std::fs::metadata(&hosts_yml).unwrap().modified().unwrap();

    assert_eq!(
        mtime_first, mtime_second,
        "no-op Sync provisioning must not touch hosts.yml mtime"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_hosts_yml_under_sync_and_token_and_ignore() {
    for mode in [
        GithubAuthMode::Sync,
        GithubAuthMode::Token,
        GithubAuthMode::Ignore,
    ] {
        let temp = tempdir().unwrap();
        let host_home = temp.path().join("host_home");
        let hosts_yml = temp.path().join("role-state-hosts.yml");

        let decoy = temp.path().join("decoy.yml");
        std::fs::write(&decoy, "secret").unwrap();
        std::os::unix::fs::symlink(&decoy, &hosts_yml).unwrap();

        let token = matches!(mode, GithubAuthMode::Token).then_some("tok");
        let err = provision_github_auth(&hosts_yml, &ctx(mode, token), &host_home).unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "mode {mode:?} did not reject symlink: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&decoy).unwrap(),
            "secret",
            "mode {mode:?} clobbered decoy"
        );
    }
}

#[cfg(unix)]
#[test]
fn synced_hosts_yml_has_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let host_home = stage_host_hosts_yml(&temp, "ghp_perm");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    provision_github_auth(&hosts_yml, &ctx(GithubAuthMode::Sync, None), &host_home).unwrap();

    let mode = std::fs::metadata(&hosts_yml).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "synced hosts.yml must be 0o600, got {mode:o}");
}

#[test]
fn sync_consumes_supplied_token_before_host_lookup() {
    let temp = tempdir().unwrap();
    let host_home = temp.path().join("empty_host_home");
    let hosts_yml = temp.path().join("role-state-hosts.yml");

    let outcome = provision_github_auth(
        &hosts_yml,
        &ctx(GithubAuthMode::Sync, Some("operator_supplied")),
        &host_home,
    )
    .unwrap();

    assert_eq!(outcome.kind(), GithubProvisionKind::Synced);
    assert_eq!(outcome.token(), Some("operator_supplied"));
    assert_eq!(
        outcome,
        GithubProvisionOutcome::Synced {
            token: "operator_supplied".to_owned(),
            source: GithubTokenSource::ConfiguredEnv,
        }
    );
    let hosts = std::fs::read_to_string(hosts_yml).unwrap();
    assert!(hosts.contains("oauth_token: operator_supplied"));
}

#[test]
fn github_auth_context_debug_redacts_token() {
    let ctx = ctx(GithubAuthMode::Token, Some("ghp_secret_value"));
    let s = format!("{ctx:?}");
    assert!(
        !s.contains("ghp_secret_value"),
        "token leaked in Debug: {s}"
    );
    assert!(s.contains("<redacted>"));
}

#[test]
fn github_provision_outcome_debug_redacts_token() {
    let synced = GithubProvisionOutcome::Synced {
        token: "ghp_synced_secret".to_owned(),
        source: GithubTokenSource::GhCli,
    };
    let s = format!("{synced:?}");
    assert!(!s.contains("ghp_synced_secret"), "Synced token leaked: {s}");

    let tok = GithubProvisionOutcome::TokenMode {
        token: "ghp_token_secret".to_owned(),
    };
    let s = format!("{tok:?}");
    assert!(
        !s.contains("ghp_token_secret"),
        "TokenMode token leaked: {s}"
    );
}

#[test]
fn sync_copies_config_toml_when_present() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, Some("[profile]\nname = \"test\""), &[], None, None);

    let (outcome, forward_auth) =
        provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("config.toml")).unwrap(),
        "[profile]\nname = \"test\""
    );
}

#[test]
fn sync_source_dir_copies_direct_kimi_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let source_dir = temp.path().join("kimi-work");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::write(
        source_dir.join("config.toml"),
        "[profile]\nname = \"workspace\"",
    )
    .unwrap();
    std::fs::write(source_dir.join("device_id"), "device-workspace").unwrap();
    std::fs::write(
        source_dir.join("credentials").join("token_main"),
        "tok_workspace",
    )
    .unwrap();

    let (outcome, forward_auth) =
        provision_kimi_auth_from_source_dir(&kimi_dir, AuthForwardMode::Sync, &source_dir).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("config.toml")).unwrap(),
        "[profile]\nname = \"workspace\""
    );
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("device_id")).unwrap(),
        "device-workspace"
    );
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("credentials").join("token_main")).unwrap(),
        "tok_workspace"
    );
}
