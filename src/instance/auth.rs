use super::{AgentState, AuthProvisionOutcome};
use crate::config::AuthForwardMode;
use std::path::Path;

impl AgentState {
    /// Provision both `.claude.json` (preferences/metadata) and
    /// `.claude/.credentials.json` (OAuth tokens) according to the chosen
    /// auth forwarding strategy.
    ///
    /// `.claude.json` must always exist after this call because Docker
    /// bind-mounts require the source to be a file, not a missing path
    /// (otherwise Docker creates a directory, breaking Claude Code).
    ///
    /// On macOS the host credentials live in the system Keychain
    /// ("Claude Code-credentials"), not in a file.  On Linux they are
    /// stored at `~/.claude/.credentials.json`.
    pub(super) fn provision_claude_auth(
        claude_json: &Path,
        claude_dir: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<AuthProvisionOutcome> {
        let host_claude_json = host_home.join(".claude.json");
        let credentials_json = claude_dir.join(".credentials.json");

        let outcome = match mode {
            AuthForwardMode::Ignore => {
                // Always ensure a clean slate — if switching from copy/sync to
                // ignore, the previously forwarded credentials must be revoked.
                if !claude_json.exists() || std::fs::read_to_string(claude_json)? != "{}" {
                    write_private_file(claude_json, "{}")?;
                }
                if credentials_json.exists() {
                    std::fs::remove_file(&credentials_json)?;
                }
                AuthProvisionOutcome::Skipped
            }
            AuthForwardMode::Copy => {
                if claude_json.exists() {
                    AuthProvisionOutcome::Skipped
                } else if let Some(creds) = read_host_credentials(host_home) {
                    copy_host_claude_json(&host_claude_json, claude_json)?;
                    write_private_file(&credentials_json, &creds)?;
                    AuthProvisionOutcome::Copied
                } else {
                    // Host has no auth — create an empty bootstrap file
                    // so Docker can bind-mount it. Claude Code will run
                    // its own first-time auth flow inside the container.
                    write_private_file(claude_json, "{}")?;
                    AuthProvisionOutcome::HostMissing
                }
            }
            AuthForwardMode::Sync => {
                if let Some(creds) = read_host_credentials(host_home) {
                    copy_host_claude_json(&host_claude_json, claude_json)?;
                    write_private_file(&credentials_json, &creds)?;
                    AuthProvisionOutcome::Synced
                } else {
                    // Host has no auth — leave the container's existing
                    // files untouched (it may have credentials from a
                    // previous manual login). Only bootstrap an empty
                    // file if nothing exists yet.
                    if !claude_json.exists() {
                        write_private_file(claude_json, "{}")?;
                    }
                    // Repair permissions on pre-existing auth files that
                    // may have legacy permissive modes (e.g. 0644).
                    repair_permissions(claude_json);
                    repair_permissions(&credentials_json);
                    AuthProvisionOutcome::HostMissing
                }
            }
        };

        Ok(outcome)
    }
}

/// Copy the host's `.claude.json` into the container state, or write `{}`
/// if the host file doesn't exist.
fn copy_host_claude_json(host_path: &Path, dest_path: &Path) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(host_path).unwrap_or_else(|_| "{}".to_string());
    write_private_file(dest_path, &content)
}

/// Read the host's Claude Code OAuth credentials.
///
/// Checks the file-based store at `~/.claude/.credentials.json` first
/// (used on Linux, and makes the function testable with temp dirs).
/// Falls back to the macOS Keychain ("Claude Code-credentials") when
/// the file is absent and `host_home` matches the real home directory.
fn read_host_credentials(host_home: &Path) -> Option<String> {
    // File-based credentials (Linux, or macOS with an explicit export).
    let creds_path = host_home.join(".claude/.credentials.json");
    if let Ok(content) = std::fs::read_to_string(creds_path) {
        return Some(content);
    }

    // macOS Keychain fallback — only attempted when host_home is the
    // real home directory.  This keeps tests hermetic (they use temp
    // dirs) while still supporting the Keychain in production.
    #[cfg(target_os = "macos")]
    {
        let real_home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
        if real_home.as_deref() == Some(host_home) {
            let output = std::process::Command::new("security")
                .args([
                    "find-generic-password",
                    "-s",
                    "Claude Code-credentials",
                    "-w",
                ])
                .output()
                .ok()?;
            if output.status.success() {
                let creds = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !creds.is_empty() {
                    return Some(creds);
                }
            }
        }
    }

    None
}

/// Reject symlinks at `path` to prevent a compromised agent from
/// redirecting host-side writes to arbitrary files.
///
/// The agent's `.claude/` directory is mounted read-write into the
/// container, so an agent could replace `.credentials.json` with a
/// symlink.  Without this check, the next `write_private_file` or
/// `repair_permissions` call would follow the symlink and overwrite
/// or chmod the target on the host.
fn reject_symlink(path: &Path) -> anyhow::Result<()> {
    // Use symlink_metadata (lstat) — regular metadata() follows symlinks.
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        anyhow::ensure!(
            !meta.file_type().is_symlink(),
            "refusing to write through symlink at {}; \
             this may indicate a compromised agent state — \
             remove the symlink and retry",
            path.display()
        );
    }
    Ok(())
}

/// Write a file with restricted permissions (`0o600` on Unix) since it
/// may contain authentication credentials.
///
/// Rejects symlinks to prevent a compromised agent from redirecting
/// writes to arbitrary host paths.  Uses `tempfile::NamedTempFile` to
/// create an unpredictable temp file (opened with `O_EXCL`, so a
/// pre-planted symlink at the temp path is impossible), then renames
/// it to the destination — closing the TOCTOU window entirely.
fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    reject_symlink(path)?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent directory for {}", path.display()))?;

        // NamedTempFile uses O_EXCL internally, so it will never follow
        // a pre-planted symlink.  The random suffix makes the path
        // unpredictable.
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(content.as_bytes())?;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))?;
        tmp.persist(path)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)?;
    }
    Ok(())
}

/// Tighten permissions on an existing file to `0o600` if it exists.
/// Refuses to operate on symlinks.  No-op on non-Unix or if the file
/// doesn't exist.
fn repair_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Use symlink_metadata so we don't follow symlinks.
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() {
                eprintln!(
                    "[jackin] warning: refusing to chmod symlink at {}",
                    path.display()
                );
                return;
            }
            if meta.is_file() {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use crate::config::AuthForwardMode;
    use crate::instance::{AgentState, AuthProvisionOutcome};
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    const TEST_CREDENTIALS: &str =
        r#"{"claudeAiOauth":{"accessToken":"test","refreshToken":"test"}}"#;

    /// Set up a fake host auth environment in the temp dir.
    fn seed_host_auth(temp: &tempfile::TempDir) {
        std::fs::write(
            temp.path().join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"test@example.com"}}"#,
        )
        .unwrap();
        let creds_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&creds_dir).unwrap();
        std::fs::write(creds_dir.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
    }

    fn simple_manifest(temp: &tempfile::TempDir) -> crate::manifest::AgentManifest {
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        crate::manifest::AgentManifest::load(temp.path()).unwrap()
    }

    // ── Auth forwarding tests ───────────────────────────────────────────

    // ── Auth forwarding tests ───────────────────────────────────────────

    #[test]
    fn ignore_mode_writes_empty_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
        assert!(!state.claude_dir.join(".credentials.json").exists());
        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    }

    #[test]
    fn copy_mode_copies_host_auth_on_first_run() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Copy,
            temp.path(),
        )
        .unwrap();

        assert!(
            std::fs::read_to_string(&state.claude_json)
                .unwrap()
                .contains("test@example.com")
        );
        assert_eq!(
            std::fs::read_to_string(state.claude_dir.join(".credentials.json")).unwrap(),
            TEST_CREDENTIALS
        );
        assert_eq!(outcome, AuthProvisionOutcome::Copied);
    }

    #[test]
    fn copy_mode_falls_back_to_empty_json_when_host_has_none() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        // No host auth seeded
        let manifest = simple_manifest(&temp);

        let (state, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Copy,
            temp.path(),
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
        assert!(!state.claude_dir.join(".credentials.json").exists());
        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    }

    #[test]
    fn copy_mode_does_not_overwrite_existing() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: creates the file
        let (state, outcome1) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Copy,
            temp.path(),
        )
        .unwrap();
        assert_eq!(outcome1, AuthProvisionOutcome::Copied);

        // Simulate the container modifying its own .claude.json
        let container_content = r#"{"oauthAccount":{"emailAddress":"container@example.com"}}"#;
        std::fs::write(&state.claude_json, container_content).unwrap();

        // Second run: should NOT overwrite
        let (state2, outcome2) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Copy,
            temp.path(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&state2.claude_json).unwrap(),
            container_content
        );
        assert_eq!(outcome2, AuthProvisionOutcome::Skipped);
    }

    #[test]
    fn sync_mode_overwrites_existing() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        // First run with host auth
        seed_host_auth(&temp);
        let (state, outcome1) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();
        assert_eq!(outcome1, AuthProvisionOutcome::Synced);

        // Simulate container modifying its own .claude.json
        std::fs::write(&state.claude_json, r#"{"container":"data"}"#).unwrap();

        // Update host credentials
        let updated_creds = r#"{"claudeAiOauth":{"accessToken":"new","refreshToken":"new"}}"#;
        std::fs::write(temp.path().join(".claude/.credentials.json"), updated_creds).unwrap();

        // Second run: should overwrite with host content
        let (state2, outcome2) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(state2.claude_dir.join(".credentials.json")).unwrap(),
            updated_creds
        );
        assert_eq!(outcome2, AuthProvisionOutcome::Synced);
    }

    // ── Mode transition tests ───────────────────────────────────────────

    #[test]
    fn switching_from_copy_to_ignore_revokes_forwarded_credentials() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: copy mode seeds credentials
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Copy,
            temp.path(),
        )
        .unwrap();
        assert!(state.claude_dir.join(".credentials.json").exists());

        // Operator switches to ignore — credentials must be wiped
        let (state2, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&state2.claude_json).unwrap(), "{}");
        assert!(!state2.claude_dir.join(".credentials.json").exists());
    }

    #[test]
    fn switching_from_sync_to_ignore_revokes_forwarded_credentials() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: sync mode writes credentials
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();
        assert!(state.claude_dir.join(".credentials.json").exists());

        // Operator switches to ignore — credentials must be wiped
        let (state2, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&state2.claude_json).unwrap(), "{}");
        assert!(!state2.claude_dir.join(".credentials.json").exists());
    }

    #[test]
    fn sync_mode_preserves_container_auth_when_host_file_missing() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        // First run: host has auth, sync copies it
        seed_host_auth(&temp);
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();

        // Host auth disappears (e.g. user logged out)
        std::fs::remove_file(temp.path().join(".claude.json")).unwrap();
        std::fs::remove_file(temp.path().join(".claude/.credentials.json")).unwrap();

        // Container may have its own auth by now (from manual login inside)
        let container_auth = r#"{"oauthAccount":{"emailAddress":"container@example.com"}}"#;
        std::fs::write(&state.claude_json, container_auth).unwrap();

        // Second run: host auth missing — container auth must be preserved
        let (state2, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&state2.claude_json).unwrap(),
            container_auth
        );
        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    }

    #[cfg(unix)]
    #[test]
    fn auth_file_has_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Copy,
            temp.path(),
        )
        .unwrap();

        let perms = std::fs::metadata(&state.claude_json).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "claude.json should have 0600 permissions"
        );
        let creds_perms = std::fs::metadata(state.claude_dir.join(".credentials.json"))
            .unwrap()
            .permissions();
        assert_eq!(
            creds_perms.mode() & 0o777,
            0o600,
            ".credentials.json should have 0600 permissions"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_repairs_permissions_on_legacy_permissive_file() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        // First run: create the file with ignore mode (gets 0600)
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
        )
        .unwrap();

        // Simulate a legacy state file with permissive mode
        std::fs::set_permissions(&state.claude_json, std::fs::Permissions::from_mode(0o644))
            .unwrap();
        let perms = std::fs::metadata(&state.claude_json).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o644, "precondition: file is 0644");

        // Sync with host auth — must tighten permissions
        seed_host_auth(&temp);
        let (state2, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();

        let perms = std::fs::metadata(&state2.claude_json)
            .unwrap()
            .permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "sync should repair permissions on existing file"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_repairs_permissions_when_host_auth_missing() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        // First run: sync with host auth to seed both files
        seed_host_auth(&temp);
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();

        // Simulate legacy permissive modes on both auth files
        std::fs::set_permissions(&state.claude_json, std::fs::Permissions::from_mode(0o644))
            .unwrap();
        let creds_path = state.claude_dir.join(".credentials.json");
        std::fs::set_permissions(&creds_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Remove host auth so sync takes the preserve path
        std::fs::remove_file(temp.path().join(".claude.json")).unwrap();
        std::fs::remove_file(temp.path().join(".claude/.credentials.json")).unwrap();

        // Second run: host auth missing — files preserved but permissions repaired
        let (state2, outcome) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);

        let json_perms = std::fs::metadata(&state2.claude_json)
            .unwrap()
            .permissions();
        assert_eq!(
            json_perms.mode() & 0o777,
            0o600,
            "sync should repair .claude.json permissions even when host auth is missing"
        );
        let creds_perms = std::fs::metadata(state2.claude_dir.join(".credentials.json"))
            .unwrap()
            .permissions();
        assert_eq!(
            creds_perms.mode() & 0o777,
            0o600,
            "sync should repair .credentials.json permissions even when host auth is missing"
        );
    }

    // ── Symlink traversal protection ────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_at_claude_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: create the state directory
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Copy,
            temp.path(),
        )
        .unwrap();

        // Replace .claude.json with a symlink to a decoy file
        let decoy = temp.path().join("decoy.txt");
        std::fs::write(&decoy, "original").unwrap();
        std::fs::remove_file(&state.claude_json).unwrap();
        std::os::unix::fs::symlink(&decoy, &state.claude_json).unwrap();

        // Sync should refuse to write through the symlink
        let err = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected symlink error, got: {err}"
        );

        // Decoy file must be untouched
        assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "original");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_at_credentials_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        seed_host_auth(&temp);
        let manifest = simple_manifest(&temp);

        // First run: create the state directory with credentials
        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Copy,
            temp.path(),
        )
        .unwrap();

        // Replace .credentials.json with a symlink
        let decoy = temp.path().join("decoy-creds.txt");
        std::fs::write(&decoy, "secret").unwrap();
        let creds_path = state.claude_dir.join(".credentials.json");
        std::fs::remove_file(&creds_path).unwrap();
        std::os::unix::fs::symlink(&decoy, &creds_path).unwrap();

        // Sync should refuse to write through the symlink
        let err = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Sync,
            temp.path(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected symlink error, got: {err}"
        );

        // Decoy file must be untouched
        assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "secret");
    }
}
