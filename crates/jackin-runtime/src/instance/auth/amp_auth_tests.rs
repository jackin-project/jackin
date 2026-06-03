//! Tests for `instance/auth` — amp auth tests.
use crate::instance::{AuthProvisionOutcome, RoleState};
use jackin_config::AuthForwardMode;
use std::path::Path;
use tempfile::tempdir;

fn stage_host_secrets(temp: &tempfile::TempDir, content: &str) -> std::path::PathBuf {
    let host_home = temp.path().join("host_home");
    let amp_dir = host_home.join(".local/share/amp");
    std::fs::create_dir_all(&amp_dir).unwrap();
    std::fs::write(amp_dir.join("secrets.json"), content).unwrap();
    host_home
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
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(secrets_json.as_path()));
    assert_eq!(
        std::fs::read_to_string(&secrets_json).unwrap(),
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}"
    );
}

#[test]
fn sync_preserves_existing_secrets_when_host_file_missing() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"in_container_login\":true}").unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

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
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

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
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(mounted.is_none());
    assert!(!secrets_json.exists());
}

#[test]
fn ignore_mode_wipes_role_secrets_json() {
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"stale\":\"creds\"}").unwrap();

    let (outcome, mounted) = RoleState::provision_amp_auth(
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
    // OAuthToken is parser-rejected for Amp; the defensive arm in
    // provision_amp_auth wipes any prior Sync's role-state file so a
    // config bypass cannot leak forwarded credentials into the
    // container. The arm should never run in production, but if it
    // does the bypass must be loud and safe.
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"prior_sync\":true}").unwrap();

    let (outcome, mounted) = RoleState::provision_amp_auth(
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
    // Loop every mode. The symlink check is hoisted above the mode
    // match, so the defense holds for all four arms today — but a
    // future refactor that pushes the check inside specific arms
    // could silently regress Sync (highest blast radius — it would
    // otherwise read through the symlink).
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

        let err = RoleState::provision_amp_auth(&secrets_json, mode, Path::new("/nonexistent"))
            .unwrap_err();

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
    // Sync arm with an unreadable host secrets.json must surface
    // the io::Error rather than misdiagnosing as HostMissing —
    // otherwise the operator gets trapped in a re-login loop they
    // cannot escape until they spot the bad permissions on the
    // host file.
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

    let result = RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home);

    // Restore perms so tempdir cleanup succeeds regardless of test
    // outcome.
    let _ = std::fs::set_permissions(&host_secrets, std::fs::Permissions::from_mode(0o600));

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
    // Bypassing `write_private_file` would land at 0o644 and leak
    // the token. Pin 0o600 explicitly.
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = stage_host_secrets(
        &temp,
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    );

    RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

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
    // HostMissing is the only path that carries a prior file
    // across launches; the arm must tighten its perms.
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    std::fs::write(&secrets_json, "{\"in_container_login\":true}").unwrap();
    std::fs::set_permissions(&secrets_json, std::fs::Permissions::from_mode(0o644)).unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, _) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

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
    // Catches a regression that swapped the Sync source from
    // XDG_DATA `secrets.json` to XDG_CONFIG `settings.json`.
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = temp.path().join("host_home");

    // Only the XDG_CONFIG decoy exists; the canonical XDG_DATA
    // path is empty.
    let xdg_config = host_home.join(".config/amp");
    std::fs::create_dir_all(&xdg_config).unwrap();
    std::fs::write(xdg_config.join("settings.json"), "WRONG").unwrap();

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

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
    // Without this guard, an empty host file would be Synced and
    // the agent would fail to auth with no breadcrumb.
    let temp = tempdir().unwrap();
    let secrets_json = temp.path().join("secrets.json");
    let host_home = stage_host_secrets(&temp, "   \n\t  \n");

    let (outcome, mounted) =
        RoleState::provision_amp_auth(&secrets_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(mounted.is_none());
    assert!(
        !secrets_json.exists(),
        "empty host file must not produce a role-state copy"
    );
}
