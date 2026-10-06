// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    if std::fs::read_to_string(&cred).is_ok() {
        return;
    }

    let result = RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home);

    drop(std::fs::set_permissions(
        &cred,
        std::fs::Permissions::from_mode(0o600),
    ));

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
    if std::fs::read_to_string(&cfg).is_ok() {
        return;
    }

    let result = RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home);

    drop(std::fs::set_permissions(
        &cfg,
        std::fs::Permissions::from_mode(0o600),
    ));

    let err = result.expect_err("unreadable config.toml must surface as error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("config.toml"),
        "error must name the unreadable file: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn credentials_nested_symlink_is_rejected_not_skipped() {
    // A symlink planted under `credentials/mcp/` (e.g. by a hostile or
    // misconfigured host) must fail the complete source snapshot. Silently
    // dropping it would publish an incomplete credential tree.
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

    let error =
        RoleState::provision_kimi_auth(&kimi_dir, AuthForwardMode::Sync, &host_home).unwrap_err();

    assert!(error.to_string().contains("symlink"), "{error:#}");
    assert!(!kimi_dir.exists(), "failed source must not publish a tree");
    // The decoy on the host must remain untouched: no write through the
    // rejected symlink and no read into the role state.
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

#[test]
fn claude_metadata_persists_inside_directory_and_supports_atomic_replacement() {
    for mode in [
        AuthForwardMode::Sync,
        AuthForwardMode::ApiKey,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::Ignore,
    ] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);
        let source = temp.path().join("selected-claude");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
        std::fs::write(source.join(".claude.json"), r#"{"account":"selected"}"#).unwrap();
        let resolvers = PrepareResolvers {
            auth_modes: &|_| mode,
            sync_source_dirs: &|_| (mode == AuthForwardMode::Sync).then(|| source.clone()),
        };
        let (state, _) = RoleState::prepare(
            &paths,
            "jk-metadata",
            &manifest,
            &resolvers,
            &GithubAuthContext::default(),
            temp.path(),
            Agent::Claude,
        )
        .unwrap();
        let directory = state.root.join("home/.claude");
        let metadata = directory.join(".claude.json");
        if mode == AuthForwardMode::Ignore {
            // Fresh ignore-mode provisioning deliberately creates no auth home.
            // The container's directory mount/CLI creates its mutable state later.
            assert!(!directory.exists());
            std::fs::create_dir_all(&directory).unwrap();
        } else {
            assert!(metadata.is_file(), "{mode} must provision metadata");
        }
        assert!(!state.root.join("home/.claude.json").exists());
        let replacement = directory.join(".claude.json.tmp");
        std::fs::write(&replacement, r#"{"onboarding":true}"#).unwrap();
        std::fs::rename(replacement, &metadata).unwrap();
        drop(state);
        RoleState::prepare(
            &paths,
            "jk-metadata",
            &manifest,
            &resolvers,
            &GithubAuthContext::default(),
            temp.path(),
            Agent::Claude,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(metadata).unwrap(),
            r#"{"onboarding":true}"#
        );
        assert_eq!(
            std::fs::read_to_string(source.join(".claude.json")).unwrap(),
            r#"{"account":"selected"}"#
        );
    }
}
