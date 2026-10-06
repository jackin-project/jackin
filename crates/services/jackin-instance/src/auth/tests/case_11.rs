// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn kimi_sync_replaces_removed_entries_and_revokes_missing_source() {
    let temp = tempdir().unwrap();
    let kimi_dir = temp.path().join("role/.kimi-code");
    let source_dir = temp.path().join("host/.kimi-code");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::write(source_dir.join("config.toml"), "profile = \"work\"\n").unwrap();
    std::fs::write(source_dir.join("device_id"), "device-old\n").unwrap();
    std::fs::write(source_dir.join("credentials/token_old"), "old-secret").unwrap();

    let (outcome, forward_auth) = RoleState::provision_kimi_auth_from_source_dir(
        &kimi_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(kimi_dir.join("credentials/token_old").exists());

    std::fs::remove_file(source_dir.join("device_id")).unwrap();
    std::fs::remove_file(source_dir.join("credentials/token_old")).unwrap();
    std::fs::write(source_dir.join("credentials/token_new"), "new-secret").unwrap();
    let (outcome, forward_auth) = RoleState::provision_kimi_auth_from_source_dir(
        &kimi_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(!kimi_dir.join("device_id").exists());
    assert!(!kimi_dir.join("credentials/token_old").exists());
    assert_eq!(
        std::fs::read_to_string(kimi_dir.join("credentials/token_new")).unwrap(),
        "new-secret"
    );

    std::fs::remove_dir_all(&source_dir).unwrap();
    let (outcome, forward_auth) = RoleState::provision_kimi_auth_from_source_dir(
        &kimi_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(forward_auth);
    assert!(kimi_dir.is_dir());
    assert!(std::fs::read_dir(&kimi_dir).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn directory_sync_replaces_nested_destination_symlinks_without_following_them() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.kimi-code");
    let target_dir = temp.path().join("role/.kimi-code");
    let decoy_dir = temp.path().join("decoy");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::create_dir_all(&decoy_dir).unwrap();
    std::fs::write(source_dir.join("config.toml"), "profile = \"fresh\"\n").unwrap();
    std::fs::write(source_dir.join("credentials/token"), "fresh-secret").unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    symlink(&decoy_dir, target_dir.join("credentials")).unwrap();

    let (outcome, forward_auth) = RoleState::provision_kimi_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        !std::fs::symlink_metadata(target_dir.join("credentials"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_to_string(target_dir.join("credentials/token")).unwrap(),
        "fresh-secret"
    );
    assert!(!decoy_dir.join("token").exists());
}

#[cfg(unix)]
#[test]
fn directory_sync_rejects_group_writable_nested_destination_files() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.kimi-code");
    let target_dir = temp.path().join("role/.kimi-code");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::write(source_dir.join("config.toml"), "profile = \"fresh\"\n").unwrap();
    std::fs::create_dir_all(target_dir.join("credentials")).unwrap();
    let loose = target_dir.join("credentials/token");
    std::fs::write(&loose, "stale-secret").unwrap();
    std::fs::set_permissions(&loose, std::fs::Permissions::from_mode(0o664)).unwrap();

    let error = RoleState::provision_kimi_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("previous auth directory is writable"),
        "unexpected error: {error:#}"
    );
}

#[cfg(unix)]
#[test]
fn tree_entry_classification_routes_symlinks_away_from_mode_checks() {
    use nix::sys::stat::{SFlag, mode_t};

    let mode = |kind: SFlag, perm: mode_t| kind.bits() | perm;
    // Linux reports every symlink as 0777; macOS reports 0755. Both are
    // unlinkable links, not writable files.
    assert_eq!(
        classify_tree_entry_for_removal(mode(SFlag::S_IFLNK, 0o777)),
        TreeEntryKind::Symlink
    );
    assert_eq!(
        classify_tree_entry_for_removal(mode(SFlag::S_IFLNK, 0o755)),
        TreeEntryKind::Symlink
    );
    assert_eq!(
        classify_tree_entry_for_removal(mode(SFlag::S_IFREG, 0o600)),
        TreeEntryKind::Regular
    );
    assert_eq!(
        classify_tree_entry_for_removal(mode(SFlag::S_IFDIR, 0o700)),
        TreeEntryKind::Directory
    );
    for special in [
        SFlag::S_IFIFO,
        SFlag::S_IFCHR,
        SFlag::S_IFBLK,
        SFlag::S_IFSOCK,
    ] {
        assert_eq!(
            classify_tree_entry_for_removal(mode(special, 0o600)),
            TreeEntryKind::Special,
            "file type {special:?} must stay fail-closed"
        );
    }
}

#[cfg(unix)]
#[test]
fn directory_sync_rejects_destination_ancestor_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let real_parent = temp.path().join("real-role");
    let linked_parent = temp.path().join("linked-role");
    let source_dir = temp.path().join("host/.kimi-code");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::create_dir_all(&real_parent).unwrap();
    std::fs::write(source_dir.join("config.toml"), "profile = \"fresh\"\n").unwrap();
    symlink(&real_parent, &linked_parent).unwrap();

    let error = RoleState::provision_kimi_auth_from_source_dir(
        &linked_parent.join(".kimi-code"),
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("symlink"),
        "unexpected error: {error}"
    );
    assert!(!real_parent.join(".kimi-code").exists());
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
