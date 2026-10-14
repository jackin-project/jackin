// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[test]
fn rejects_symlink_at_credentials_json() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    let decoy = temp.path().join("decoy-creds.txt");
    std::fs::write(&decoy, "secret").unwrap();
    let creds_path = state.claude_credentials_json().unwrap();
    std::fs::remove_file(creds_path).unwrap();
    std::os::unix::fs::symlink(&decoy, creds_path).unwrap();
    drop(state);

    let err = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("symlink"),
        "expected symlink error, got: {err}"
    );

    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "secret");
}

#[test]
fn sync_copies_host_secrets_json_when_present() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = stage_host_secrets(
        &temp,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    );

    let (outcome, mounted) =
        provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
    assert_eq!(
        std::fs::read_to_string(&secrets_json).unwrap(),
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}"
    );
}

#[test]
fn sync_source_dir_copies_direct_secrets_json() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let source_dir = temp.path().join("amp-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    let expected = "{\"apiKey@https://ampcode.com/\":\"sgamp_workspace_test\"}";
    std::fs::write(source_dir.join("secrets.json"), expected).unwrap();

    let (outcome, mounted) =
        provision_amp_auth_from_source_dir(&secrets_json, AuthForwardMode::Sync, &source_dir)
            .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
    assert_eq!(std::fs::read_to_string(&secrets_json).unwrap(), expected);
}

#[test]
fn sync_preserves_existing_secrets_when_host_file_missing() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"in_container_login\":true}").unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, mounted) =
        provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
    assert_eq!(
        std::fs::read_to_string(&secrets_json).unwrap(),
        "{\"in_container_login\":true}"
    );
}

#[test]
fn sync_with_no_host_and_no_prior_file_skips_mount() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = temp.path().join("empty_host_home");

    let (outcome, mounted) =
        provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(mounted.is_none());
    assert!(!secrets_json.exists());
}

#[test]
fn api_key_mode_wipes_role_secrets_json() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"stale\":\"creds\"}").unwrap();
    let host_home = stage_host_secrets(
        &temp,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    );

    let (outcome, mounted) =
        provision_amp_auth(&secrets_json, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(mounted.is_none());
    assert!(!secrets_json.exists());
}

#[test]
fn ignore_mode_wipes_role_secrets_json() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"stale\":\"creds\"}").unwrap();

    let (outcome, mounted) = provision_amp_auth(
        &secrets_json,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(mounted.is_none());
    assert!(!secrets_json.exists());
}

#[test]
fn oauth_token_defensive_arm_wipes_role_state() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"prior_sync\":true}").unwrap();

    let (outcome, mounted) = provision_amp_auth(
        &secrets_json,
        AuthForwardMode::OAuthToken,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(mounted.is_none(), "bypass arm must not produce a mount");
    assert!(
        !secrets_json.exists(),
        "bypass arm must wipe the prior Sync residue"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_secrets_json_under_every_mode() {
    for mode in [
        AuthForwardMode::Sync,
        AuthForwardMode::ApiKey,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::Ignore,
    ] {
        let temp = tempdir().unwrap();
        let secrets_json = temp.path().join("secrets.json");
        let decoy = temp.path().join("decoy.txt");
        std::fs::write(&decoy, "secret").unwrap();
        std::os::unix::fs::symlink(&decoy, &secrets_json).unwrap();

        let err = provision_amp_auth(&secrets_json, mode, Path::new("/nonexistent")).unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "mode={mode:?}: expected symlink rejection, got: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&decoy).unwrap(),
            "secret",
            "mode={mode:?}: decoy contents must survive"
        );
    }
}

#[cfg(unix)]
#[test]
fn surfaces_unreadable_host_secrets_json_as_error() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = temp.path().join("host_home");
    let amp_dir = host_home.join(".local/share/amp");
    std::fs::create_dir_all(&amp_dir).unwrap();
    let host_secrets = amp_dir.join("secrets.json");
    std::fs::write(
        &host_secrets,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    )
    .unwrap();
    std::fs::set_permissions(&host_secrets, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&host_secrets).is_ok() {
        return;
    }

    let result = provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home);

    drop(std::fs::set_permissions(
        &host_secrets,
        std::fs::Permissions::from_mode(0o600),
    ));

    let err = result.expect_err("EACCES on host secrets.json must surface as an error");
    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("secrets.json"),
        "error must name the host file: {rendered}"
    );
    assert!(
        !rendered.contains("not found") && !rendered.to_ascii_lowercase().contains("nonexistent"),
        "EACCES must not be reported as not-found: {rendered}"
    );
}

#[cfg(unix)]
#[test]
fn synced_secrets_json_has_restricted_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = stage_host_secrets(
        &temp,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    );

    provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    let mode = std::fs::metadata(&secrets_json)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "synced secrets.json must be 0o600, got {mode:o}"
    );
}

#[cfg(unix)]
#[test]
fn sync_repairs_permissions_when_host_secrets_missing() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"in_container_login\":true}").unwrap();
    std::fs::set_permissions(&secrets_json, std::fs::Permissions::from_mode(0o644)).unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, _) =
        provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    let mode = std::fs::metadata(&secrets_json)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "HostMissing must repair the carry-over file's permissions; got {mode:o}"
    );
}

#[test]
fn sync_ignores_xdg_config_settings_json_decoy() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = temp.path().join("host_home");

    let xdg_config = host_home.join(".config/amp");
    std::fs::create_dir_all(&xdg_config).unwrap();
    std::fs::write(xdg_config.join("settings.json"), "WRONG").unwrap();

    let (outcome, mounted) =
        provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(
        outcome,
        AuthProvisionOutcome::HostMissing,
        "decoy at XDG_CONFIG must not produce a Sync"
    );
    assert!(mounted.is_none());
    assert!(
        !secrets_json.exists(),
        "decoy contents must not be copied into the role state"
    );
}

#[test]
fn sync_treats_empty_host_secrets_as_host_missing() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = stage_host_secrets(&temp, "   \n\t  \n");

    let (outcome, mounted) =
        provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(mounted.is_none());
    assert!(
        !secrets_json.exists(),
        "empty host file must not produce a role-state copy"
    );
}

#[test]
fn sync_copies_host_auth_json_when_present() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, expected) = stage_host_auth_json(&temp, "abc.test");

    let (outcome, _) = provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
}
