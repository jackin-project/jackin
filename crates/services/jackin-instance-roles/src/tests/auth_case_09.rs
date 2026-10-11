// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn sync_treats_empty_host_auth_json_as_host_missing() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let host_home = temp.path().join("host_home");
    std::fs::create_dir_all(host_home.join(".codex")).unwrap();
    std::fs::write(host_home.join(".codex/auth.json"), " \n\t").unwrap();

    let (outcome, mounted) =
        provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(mounted.is_none());
    assert!(
        !auth_json.exists(),
        "empty Codex credentials must not create a role-state mount"
    );
}

#[cfg(unix)]
#[test]
fn sync_does_not_rewrite_inode_when_content_unchanged() {
    use std::os::unix::fs::MetadataExt as _;

    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "stable.test");

    let (outcome1, _) =
        provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome1, AuthProvisionOutcome::Synced);
    let ino_before = std::fs::metadata(&auth_json).unwrap().ino();

    // Second identical sync (mirrors the background prewarm re-provisioning
    // the same file the foreground launch already bind-mounted).
    let (outcome2, mounted) =
        provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome2, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(auth_json.as_path()));
    let ino_after = std::fs::metadata(&auth_json).unwrap().ino();

    assert_eq!(
        ino_before, ino_after,
        "unchanged re-sync must not replace the inode (would stale a live bind mount)"
    );
}

#[test]
fn sync_source_dir_copies_direct_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let source_dir = temp.path().join("codex-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    let expected = r#"{"auth_mode":"chatgpt","tokens":{"id_token":"workspace.test"}}"#;
    std::fs::write(source_dir.join("auth.json"), expected).unwrap();

    let (outcome, mounted) =
        provision_codex_auth_from_source_dir(&auth_json, AuthForwardMode::Sync, &source_dir)
            .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(auth_json.as_path()));
    assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
}

#[cfg(unix)]
#[test]
fn single_file_source_replacement_between_validation_and_open_is_rejected() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("codex-work");
    let target = temp.path().join("role/auth.json");
    let source = source_dir.join("auth.json");
    let replacement = source_dir.join("auth.json.replacement");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(&source, "old").unwrap();
    std::fs::write(&replacement, "replacement").unwrap();
    let source_for_hook = source.clone();
    set_source_open_hook(Box::new(move || {
        std::fs::rename(&replacement, &source_for_hook).unwrap();
    }));

    let error = provision_codex_auth_from_source_dir(&target, AuthForwardMode::Sync, &source_dir)
        .unwrap_err();
    assert!(
        error.to_string().contains("replaced during secure open"),
        "unexpected error: {error:#}"
    );
    assert!(
        !target.exists(),
        "replaced source must not publish a credential"
    );
}

#[test]
fn sync_returns_host_missing_when_host_lacks_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let host_home = temp.path().join("host_home_without_codex_dir");

    let (outcome, _) = provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(!auth_json.exists(), "no bootstrap file should be created");
}

#[test]
fn sync_preserves_existing_role_auth_json_when_host_file_missing() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"in_container_login\":true}").unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, _) = provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert_eq!(
        std::fs::read_to_string(&auth_json).unwrap(),
        "{\"in_container_login\":true}",
        "in-container login state must survive sync-with-no-host"
    );
}

#[test]
fn ignore_deletes_existing_role_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"stale\":\"creds\"}").unwrap();

    let (outcome, _) = provision_codex_auth(
        &auth_json,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!auth_json.exists());
}

#[test]
fn token_mode_leaves_role_auth_json_untouched() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"existing\":true}").unwrap();
    let (host_home, _) = stage_host_auth_json(&temp, "should-not-be-copied");

    let (outcome, _) =
        provision_codex_auth(&auth_json, AuthForwardMode::OAuthToken, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert_eq!(
        std::fs::read_to_string(&auth_json).unwrap(),
        "{\"existing\":true}"
    );
}

#[test]
fn api_key_mode_wipes_role_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"stale\":\"creds\"}").unwrap();
    // Stage a host auth.json too — api_key mode must NOT copy it,
    // and must NOT leave the stale role-state file in place either.
    let (host_home, _) = stage_host_auth_json(&temp, "should-not-be-copied");

    let (outcome, mounted) =
        provision_codex_auth(&auth_json, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(
        !auth_json.exists(),
        "api_key mode must wipe role-state auth.json"
    );
    assert!(
        mounted.is_none(),
        "api_key mode must report no auth.json to mount"
    );
}

#[test]
fn switching_from_sync_to_api_key_wipes_synced_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "switch.test");

    let (outcome, _) = provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(auth_json.exists());

    let (outcome, mounted) =
        provision_codex_auth(&auth_json, AuthForwardMode::ApiKey, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(!auth_json.exists(), "ApiKey must wipe prior synced creds");
    assert!(mounted.is_none());
}

#[cfg(unix)]
#[test]
fn synced_auth_json_has_restricted_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "perm.test");

    provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    let mode = std::fs::metadata(&auth_json).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "codex auth.json must be 0o600, got {mode:o}");
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_auth_json_under_ignore() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");

    let decoy = temp.path().join("decoy.txt");
    std::fs::write(&decoy, "secret").unwrap();
    std::os::unix::fs::symlink(&decoy, &auth_json).unwrap();

    let err = provision_codex_auth(
        &auth_json,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("symlink"),
        "expected symlink rejection, got: {err}"
    );
    // Decoy file must be untouched.
    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "secret");
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_auth_json_under_sync_and_token() {
    for mode in [
        AuthForwardMode::Sync,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::ApiKey,
    ] {
        let temp = tempdir().unwrap();
        let auth_json = temp.path().join("auth.json");

        let decoy = temp.path().join("decoy.txt");
        std::fs::write(&decoy, "secret").unwrap();
        std::os::unix::fs::symlink(&decoy, &auth_json).unwrap();

        let err = provision_codex_auth(&auth_json, mode, Path::new("/nonexistent")).unwrap_err();
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

#[test]
fn switching_from_sync_to_ignore_wipes_synced_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "rev.test");

    let (outcome, _) = provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(auth_json.exists());

    let (outcome, _) =
        provision_codex_auth(&auth_json, AuthForwardMode::Ignore, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!auth_json.exists(), "Ignore must wipe prior synced creds");
}

#[cfg(unix)]
#[test]
fn surfaces_unreadable_host_auth_json_as_error() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let host_home = temp.path().join("host_home");
    let host_codex = host_home.join(".codex");
    std::fs::create_dir_all(&host_codex).unwrap();
    let host_auth_json = host_codex.join("auth.json");
    std::fs::write(&host_auth_json, "{\"auth_mode\":\"chatgpt\"}").unwrap();
    // chmod 0 — file exists but is unreadable. Skip if we can't
    // produce an unreadable file (e.g. running as root in CI).
    std::fs::set_permissions(&host_auth_json, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&host_auth_json).is_ok() {
        // Running as root — chmod 0 doesn't block reads. Skip.
        return;
    }

    let err = provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("auth.json"),
        "error must mention the host path: {msg}"
    );
    assert!(
        !msg.to_lowercase().contains("not found"),
        "EACCES must not be reported as not-found: {msg}"
    );
}

#[test]
fn parse_hosts_yml_extracts_oauth_token_and_user() {
    let text = "github.com:\n    oauth_token: ghp_xxx\n    user: alice\n";
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_xxx");
    assert_eq!(parsed.user.as_deref(), Some("alice"));
}

#[test]
fn parse_hosts_yml_handles_quoted_values() {
    let text = "github.com:\n    oauth_token: \"ghp_xxx\"\n    user: \'bob\'\n";
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_xxx");
    assert_eq!(parsed.user.as_deref(), Some("bob"));
}

#[test]
fn parse_hosts_yml_returns_none_when_github_block_missing() {
    let text = "ghe.acme.com:\n    oauth_token: ghp_acme\n";
    assert!(parse_gh_hosts_yml(text).is_none());
}

#[test]
fn parse_hosts_yml_returns_none_without_oauth_token() {
    let text = "github.com:\n    user: alice\n";
    assert!(parse_gh_hosts_yml(text).is_none());
}

#[test]
fn parse_hosts_yml_ignores_other_hosts() {
    let text = concat!(
        "ghe.acme.com:\n    oauth_token: ghp_acme\n    user: bob\n",
        "github.com:\n    oauth_token: ghp_real\n    user: alice\n",
    );
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_real");
    assert_eq!(parsed.user.as_deref(), Some("alice"));
}

#[test]
fn parse_hosts_yml_preserves_hash_inside_token_value() {
    let text = "github.com:\n    oauth_token: ghp_real#segment\n";
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_real#segment");
}

#[test]
fn parse_hosts_yml_strips_trailing_whitespace_comment() {
    let text = "github.com:\n    oauth_token: ghp_real # rotated 2026-01\n";
    let parsed = parse_gh_hosts_yml(text).expect("must parse");
    assert_eq!(parsed.token, "ghp_real");
}

#[test]
fn parse_hosts_yml_rejects_malformed_yaml() {
    let text = "github.com:\n    oauth_token: \'broken\"\n";
    assert!(parse_gh_hosts_yml(text).is_none());
}
