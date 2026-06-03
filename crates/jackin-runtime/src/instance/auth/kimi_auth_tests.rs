//! Tests for `instance/auth` — kimi auth tests.
use jackin_config::AuthForwardMode;
use crate::instance::{AuthProvisionOutcome, RoleState};
use std::path::Path;
use tempfile::tempdir;

fn stage_host_kimi_dir(
    temp: &tempfile::TempDir,
    config_content: Option<&str>,
    cred_files: &[(&str, &str)],
    mcp_json: Option<&str>,
    device_id: Option<&str>,
) -> std::path::PathBuf {
    let host_home = temp.path().join("host_home");
    let kimi_dir = host_home.join(".kimi-code");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    if let Some(content) = config_content {
        std::fs::write(kimi_dir.join("config.toml"), content).unwrap();
    }
    if !cred_files.is_empty() {
        let creds_dir = kimi_dir.join("credentials");
        std::fs::create_dir_all(&creds_dir).unwrap();
        for (name, content) in cred_files {
            std::fs::write(creds_dir.join(name), content).unwrap();
        }
    }
    if let Some(content) = mcp_json {
        std::fs::write(kimi_dir.join("mcp.json"), content).unwrap();
    }
    if let Some(content) = device_id {
        std::fs::write(kimi_dir.join("device_id"), content).unwrap();
    }
    host_home
}

#[test]
fn sync_copies_config_toml_when_present() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, Some("[profile]\nname = \"test\""), &[], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("config.toml")).unwrap(),
        "[profile]\nname = \"test\""
    );
}

#[test]
fn sync_copies_credentials_files_when_present() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[("token_main", "tok_abc123")], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("credentials").join("token_main")).unwrap(),
        "tok_abc123"
    );
}

#[test]
fn sync_does_not_forward_mcp_json() {
    // `mcp.json` is operator-preference MCP server config, not auth
    // state. Forwarding it would leak host paths/binaries into the
    // sealed container and bypass the role-author model for declaring
    // in-container MCP servers. Regression guard for that decision:
    // even with the host file present, Sync must not copy it.
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[], Some(r#"{"servers":{}}"#), None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        !kimi_dir.join("mcp.json").exists(),
        "mcp.json must not be forwarded into the role state"
    );
}

#[test]
fn sync_copies_device_id_when_present() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[], None, Some("device-abc123\n"));

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("device_id")).unwrap(),
        "device-abc123\n"
    );
}

#[test]
fn sync_with_empty_kimi_dir_creates_role_state_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(kimi_dir.is_dir(), "role-state kimi dir must be created");
}

#[test]
fn sync_with_no_host_kimi_dir_returns_host_missing_with_forward_auth_true() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("empty_host_home");

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(forward_auth);
}

#[test]
fn sync_host_missing_still_creates_kimi_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("empty_host_home");

    RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert!(
        kimi_dir.is_dir(),
        "role-state kimi dir must exist even when host is absent"
    );
}

#[test]
fn api_key_mode_wipes_prior_kimi_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    std::fs::write(kimi_dir.join("config.toml"), "stale").unwrap();
    let host_home = stage_host_kimi_dir(&temp, Some("[profile]\nname=\"test\""), &[], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(!forward_auth);
    assert!(!kimi_dir.exists(), "api_key mode must wipe the kimi dir");
}

#[test]
fn ignore_mode_wipes_prior_kimi_dir() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    std::fs::write(kimi_dir.join("config.toml"), "old_config").unwrap();

    let (outcome, forward_auth) = RoleState::provision_kimi_auth(
        &kimi_dir,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!forward_auth);
    assert!(!kimi_dir.exists(), "ignore mode must wipe the kimi dir");
}

#[test]
fn oauth_token_defensive_arm_wipes_kimi_dir() {
    // OAuthToken is parser-rejected for Kimi; the defensive arm wipes
    // any prior Sync's role-state dir so a config bypass cannot leak
    // forwarded credentials into the container.
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    std::fs::write(kimi_dir.join("config.toml"), "prior_sync = true").unwrap();

    let (outcome, forward_auth) = RoleState::provision_kimi_auth(
        &kimi_dir,
        AuthForwardMode::OAuthToken,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(!forward_auth, "bypass arm must not set forward_auth");
    assert!(
        !kimi_dir.exists(),
        "bypass arm must wipe the prior Sync residue"
    );
}

#[test]
fn forward_auth_true_for_synced() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, Some("[x]"), &[], None, None);

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth, "forward_auth must be true for Synced");
}

#[test]
fn forward_auth_true_for_host_missing() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("no_host");

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(forward_auth, "forward_auth must be true for HostMissing");
}

#[test]
fn forward_auth_false_for_api_key() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::ApiKey, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert!(!forward_auth, "forward_auth must be false for TokenMode");
}

#[test]
fn forward_auth_false_for_ignore() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Ignore, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!forward_auth, "forward_auth must be false for Skipped");
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_kimi_dir_under_every_mode() {
    // The symlink check is hoisted above the mode match; verify all
    // four arms are protected so a future refactor cannot regress the
    // Sync arm (highest blast radius).
    for mode in [
        AuthForwardMode::Sync,
        AuthForwardMode::ApiKey,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::Ignore,
    ] {
        let temp = tempdir().unwrap();
        let kimi_dir = temp.path().join("kimi_state");
        let decoy = temp.path().join("decoy_dir");
        std::fs::create_dir_all(&decoy).unwrap();
        std::os::unix::fs::symlink(&decoy, &kimi_dir).unwrap();

        let err =
            RoleState::provision_kimi_auth(&kimi_dir, mode, Path::new("/nonexistent")).unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "mode={mode:?}: expected symlink rejection, got: {err}"
        );
        assert!(decoy.exists(), "mode={mode:?}: decoy dir must survive");
    }
}

#[cfg(unix)]
#[test]
fn synced_credential_files_have_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(
        &temp,
        Some("[profile]"),
        &[("access_token", "tok_secret_xyz")],
        None,
        Some("device-secret"),
    );

    RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    for rel in &["config.toml", "credentials/access_token", "device_id"] {
        let path = kimi_dir.join(rel);
        let mode = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("missing synced file {rel}: {e}"))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "synced file {rel} must be 0o600, got {mode:o}");
    }
}

#[test]
fn credentials_subdir_copies_recursively() {
    // Kimi Code stores MCP OAuth credentials under credentials/mcp/, so
    // subdirectories must be copied recursively.
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");
    let host_creds = host_home.join(".kimi-code/credentials");
    std::fs::create_dir_all(&host_creds).unwrap();
    std::fs::write(host_creds.join("real_token"), "real_tok_value").unwrap();
    let nested = host_creds.join("nested_subdir");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("inner_file"), "should_copy").unwrap();

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        kimi_dir.join("credentials/real_token").exists(),
        "real_token must be copied"
    );
    assert!(
        kimi_dir
            .join("credentials/nested_subdir/inner_file")
            .exists(),
        "nested subdir must be copied recursively"
    );
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("credentials/nested_subdir/inner_file")).unwrap(),
        "should_copy"
    );
}

#[cfg(unix)]
#[test]
fn surfaces_unreadable_credential_file_as_error() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, None, &[("access_token", "secret")], None, None);

    let cred = host_home.join(".kimi-code/credentials/access_token");
    std::fs::set_permissions(&cred, std::fs::Permissions::from_mode(0o000)).unwrap();

    let result = RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home);

    let _ = std::fs::set_permissions(&cred, std::fs::Permissions::from_mode(0o600));

    let err = result.expect_err("unreadable credential file must surface as error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("access_token"),
        "error must name the unreadable file: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn surfaces_unreadable_config_toml_as_error() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = stage_host_kimi_dir(&temp, Some("[profile]\nname=\"x\""), &[], None, None);

    let cfg = host_home.join(".kimi-code/config.toml");
    std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o000)).unwrap();

    let result = RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home);

    let _ = std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o600));

    let err = result.expect_err("unreadable config.toml must surface as error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("config.toml"),
        "error must name the unreadable file: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn credentials_nested_symlink_is_skipped_not_followed() {
    // A symlink planted under `credentials/mcp/` (e.g. by a hostile or
    // misconfigured host) must NOT be copied or dereferenced into the
    // sealed container. Real files in the same subtree must still copy.
    use std::os::unix::fs::symlink;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");
    let host_creds = host_home.join(".kimi-code/credentials");
    std::fs::create_dir_all(host_creds.join("mcp")).unwrap();
    std::fs::write(host_creds.join("mcp").join("real_token"), "real").unwrap();
    let decoy = temp.path().join("decoy_outside_tree");
    std::fs::write(&decoy, "must_not_leak").unwrap();
    symlink(&decoy, host_creds.join("mcp").join("evil")).unwrap();

    let (outcome, forward_auth) =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        kimi_dir.join("credentials/mcp/real_token").exists(),
        "real nested file must still be copied"
    );
    assert!(
        !kimi_dir.join("credentials/mcp/evil").exists(),
        "nested symlink must not appear in role state"
    );
    // The decoy on the host must remain untouched: no write through the
    // skipped symlink, no read into the role state.
    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "must_not_leak");
}

#[cfg(unix)]
#[test]
fn credentials_directories_are_chmodded_0700() {
    // Every copied credentials directory (root + nested) must be 0o700 so
    // the OAuth token subtree is not group/other-readable inside the
    // role-state bind mount.
    use std::os::unix::fs::PermissionsExt;
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("kimi_state");
    let host_home = temp.path().join("host_home");
    let host_mcp = host_home.join(".kimi-code/credentials/mcp");
    std::fs::create_dir_all(&host_mcp).unwrap();
    std::fs::write(host_mcp.join("token"), "tok").unwrap();

    RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap();

    for rel in &["credentials", "credentials/mcp"] {
        let mode = std::fs::metadata(kimi_dir.join(rel))
            .unwrap_or_else(|e| panic!("missing credentials dir {rel}: {e}"))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "{rel} must be 0o700, got 0o{mode:o}");
    }
}
