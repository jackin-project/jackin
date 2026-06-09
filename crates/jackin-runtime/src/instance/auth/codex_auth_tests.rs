//! Tests for `instance/auth` — codex auth tests.
use crate::instance::{AuthProvisionOutcome, RoleState};
use jackin_config::AuthForwardMode;
use std::path::Path;
use tempfile::tempdir;

/// Stage a fake host home with a populated `~/.codex/auth.json` so
/// the sync-mode tests below have a real source file to copy from.
/// Returns the host-home root and the auth.json contents written.
fn stage_host_auth_json(temp: &tempfile::TempDir, tail: &str) -> (std::path::PathBuf, String) {
    let host_home = temp.path().join("host_home");
    let codex_dir = host_home.join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let content = format!(
        "{{\"auth_mode\":\"chatgpt\",\"OPENAI_API_KEY\":null,\"tokens\":{{\"id_token\":\"{tail}\"}}}}",
    );
    std::fs::write(codex_dir.join("auth.json"), &content).unwrap();
    (host_home, content)
}

#[test]
fn sync_copies_host_auth_json_when_present() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, expected) = stage_host_auth_json(&temp, "abc.test");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
}

#[test]
fn sync_returns_host_missing_when_host_lacks_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let host_home = temp.path().join("host_home_without_codex_dir");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(!auth_json.exists(), "no bootstrap file should be created");
}

#[test]
fn sync_preserves_existing_role_auth_json_when_host_file_missing() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"in_container_login\":true}").unwrap();
    let host_home = temp.path().join("empty_host_home");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

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

    let (outcome, _) = RoleState::provision_codex_auth(
        &auth_json,
        AuthForwardMode::Ignore,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!auth_json.exists());
}

/// `OAuthToken` is parser-rejected for Codex so this arm is
/// unreachable from operator config in production. The test pins the
/// defensive no-wipe behavior of the `OAuthToken` arm anyway: if a
/// parser bypass ever lands a Codex+OAuthToken config at this layer,
/// the existing role-state `auth.json` is preserved (rather than
/// silently destroyed) so the operator can recover.
#[test]
fn token_mode_leaves_role_auth_json_untouched() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"existing\":true}").unwrap();
    let (host_home, _) = stage_host_auth_json(&temp, "should-not-be-copied");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::OAuthToken, &host_home)
            .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
    assert_eq!(
        std::fs::read_to_string(&auth_json).unwrap(),
        "{\"existing\":true}"
    );
}

/// `ApiKey` mode authenticates via `OPENAI_API_KEY`; a leftover
/// `auth.json` from a prior Sync run would let Codex silently fall
/// back to forwarded OAuth credentials that the operator has
/// explicitly chosen to bypass. Pin the wipe contract here so a
/// future refactor can't quietly downgrade `ApiKey` to no-op.
#[test]
fn api_key_mode_wipes_role_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(&auth_json, "{\"stale\":\"creds\"}").unwrap();
    // Stage a host auth.json too — api_key mode must NOT copy it,
    // and must NOT leave the stale role-state file in place either.
    let (host_home, _) = stage_host_auth_json(&temp, "should-not-be-copied");

    let (outcome, mounted) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::ApiKey, &host_home).unwrap();

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

/// Switching from Sync (creds present on host) to `ApiKey` must wipe
/// the synced `auth.json` so the next container start cannot fall
/// back to forwarded OAuth credentials. Without this, an operator
/// who toggles to `ApiKey` to use `OPENAI_API_KEY` would still be
/// running on stale OAuth state from the previous sync run.
#[test]
fn switching_from_sync_to_api_key_wipes_synced_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "switch.test");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(auth_json.exists());

    let (outcome, mounted) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::ApiKey, &host_home).unwrap();
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

    RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

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

    let err = RoleState::provision_codex_auth(
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

/// Pin symlink rejection across all credential-bearing modes via the
/// pre-mode-dispatch check at the top of `provision_codex_auth`.
/// Without this, a compromised role could plant a symlink at the
/// role-state `auth.json` and have subsequent sync/token-mode
/// provisioning bind-mount it into the container as-is.
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

        let err = RoleState::provision_codex_auth(&auth_json, mode, Path::new("/nonexistent"))
            .unwrap_err();
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

/// Switching from Sync (creds present on host) to Ignore must wipe
/// the synced auth.json so the next container start forces a fresh
/// in-container login. Without this, an operator who toggles to
/// Ignore to revoke access keeps the prior credentials accessible.
#[test]
fn switching_from_sync_to_ignore_wipes_synced_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let (host_home, _) = stage_host_auth_json(&temp, "rev.test");

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(auth_json.exists());

    let (outcome, _) =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Ignore, &host_home).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!auth_json.exists(), "Ignore must wipe prior synced creds");
}

/// An unreadable host `auth.json` (e.g. `chmod 0` after a `sudo
/// codex login`) used to be silently bucketed as `HostMissing`,
/// trapping operators in a re-login loop. Verify the EACCES path
/// now surfaces an explicit error mentioning the host path.
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

    let err =
        RoleState::provision_codex_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap_err();
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
