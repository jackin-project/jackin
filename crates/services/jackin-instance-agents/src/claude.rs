// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Claude credential provisioning plus host credential readers.

use jackin_instance_credentials::AuthProvisionOutcome;

use anyhow::Context;
use jackin_config::AuthForwardMode;

use std::path::Path;

use crate::{host_home_is_real, read_source_text, wipe_file_if_present};
use jackin_instance_credentials::{auth_directory, repair_permissions, write_private_file};

/// Provision Claude's host-side auth files (`account_json` and
/// `credentials_json`) according to the chosen auth-forwarding
/// strategy and report whether the files should be bind-mounted
/// into the container under `/jackin/claude/`.
///
/// Returns `(outcome, forward_auth)`. `forward_auth` controls
/// whether the launcher will bind-mount the files; the underlying
/// host paths are unconditionally tracked on `RoleState` so callers
/// can still inspect them (tests, debug output, future migration).
///
///   * **Sync** + host file present → write both files at `0o600`,
///     `forward_auth = true`. Container auth flows from host.
///   * **Sync** + host file absent → preserve any existing role-
///     state files (may carry forward an in-container login),
///     `forward_auth = true`. The launcher then mounts only the
///     files that actually exist on disk.
///   * **`OAuthToken`** → remove any forwarded `credentials.json`
///     (revokes prior Sync state) and write a
///     `{"hasCompletedOnboarding":true}` skeleton at `account_json`,
///     `forward_auth = true`. The skeleton suppresses the CLI's
///     "Select login method" wizard so it reads the
///     `CLAUDE_CODE_OAUTH_TOKEN` env var instead.
///   * **`ApiKey`/`Ignore`** → wipe both role-state files and
///     `forward_auth = false`. `ApiKey` authenticates via
///     `ANTHROPIC_API_KEY`; `Ignore` forces a fresh login inside
///     the durable per-instance agent home.
///
/// On macOS the host credentials live in the system Keychain
/// ("Claude Code-credentials"), not in a file. On Linux they are
/// stored at `~/.claude/.credentials.json`.
pub fn provision_claude_auth(
    account_json: &Path,
    credentials_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    let host_claude_json = host_home.join(".claude.json");

    let outcome = match mode {
        AuthForwardMode::Ignore => {
            // Always ensure a clean slate — if switching from sync/token
            // to ignore, the previously forwarded credentials must be
            // revoked.
            wipe_claude_state(account_json, credentials_json)?;
            AuthProvisionOutcome::Skipped
        }
        // ApiKey: wipe any forwarded host creds; agent authenticates
        // via ANTHROPIC_API_KEY in the env. No skeleton needed —
        // console-API auth path does not require ~/.claude.json.
        AuthForwardMode::ApiKey => {
            wipe_claude_state(account_json, credentials_json)?;
            AuthProvisionOutcome::Skipped
        }
        // OAuthToken: write a minimal skeleton so the Claude CLI skips
        // its interactive login wizard and reads CLAUDE_CODE_OAUTH_TOKEN
        // from the env instead. Without this file, the CLI shows the
        // "Select login method" prompt even when the env var is set.
        AuthForwardMode::OAuthToken => {
            if credentials_json.exists() {
                std::fs::remove_file(credentials_json)?;
            }
            write_private_file(account_json, r#"{"hasCompletedOnboarding":true}"#)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Sync => {
            if let Some(creds) = read_host_credentials(host_home)? {
                copy_host_claude_json(&host_claude_json, account_json)?;
                write_private_file(credentials_json, &creds)?;
                AuthProvisionOutcome::Synced
            } else {
                // Host has no auth — leave the container's existing
                // files untouched (they may carry credentials from a
                // previous manual login). Bootstrap an empty
                // account.json if nothing exists yet so the file is
                // always present after `prepare`, simplifying
                // inspection callers.
                if !account_json.exists() {
                    write_private_file(account_json, "{}")?;
                }
                // Repair permissions on pre-existing auth files that
                // may have legacy permissive modes (e.g. 0644).
                repair_permissions(account_json)?;
                repair_permissions(credentials_json)?;
                AuthProvisionOutcome::HostMissing
            }
        }
    };

    // Sync and token modes forward auth state (the launcher checks
    // file existence at mount time). ApiKey and Ignore do not.
    let forward_auth = matches!(
        outcome,
        AuthProvisionOutcome::Synced
            | AuthProvisionOutcome::HostMissing
            | AuthProvisionOutcome::TokenMode
    );
    Ok((outcome, forward_auth))
}

pub fn provision_claude_auth_from_config_dir(
    account_json: &Path,
    credentials_json: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
    source_dir: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    let outcome = match mode {
        AuthForwardMode::Ignore | AuthForwardMode::ApiKey => {
            wipe_claude_state(account_json, credentials_json)?;
            AuthProvisionOutcome::Skipped
        }
        AuthForwardMode::OAuthToken => {
            wipe_file_if_present(credentials_json)?;
            write_private_file(account_json, r#"{"hasCompletedOnboarding":true}"#)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Sync => {
            // Read ONLY the selected source folder's credentials. An
            // explicit source dir must never fall back to the default
            // host `~/.claude` / default Keychain account — that leak
            // is exactly the bug this path guards against (an operator
            // who picked an Enterprise source folder would otherwise
            // get their default Max account inside the capsule).
            #[cfg(unix)]
            let source_data = {
                let source = auth_directory::lock_source_dir(source_dir)?;
                let creds = match source.as_ref() {
                    Some(source) => locked_claude_credentials(source, source_dir, host_home)?,
                    None => None,
                };
                let account = match source.as_ref() {
                    Some(source) => auth_directory::read_locked_source_file(
                        &source.root,
                        &[".claude.json"],
                        "Claude account metadata",
                    )?
                    .map(|bytes| {
                        String::from_utf8(bytes)
                            .context("Claude account metadata is not valid UTF-8")
                    })
                    .transpose()?
                    .unwrap_or_else(|| "{}".to_owned()),
                    None => "{}".to_owned(),
                };
                (creds, account)
            };
            #[cfg(not(unix))]
            let source_data = (
                read_host_credentials_from_claude_config_dir(source_dir, host_home)?,
                read_source_text(&source_dir.join(".claude.json"), "Claude account metadata")?
                    .unwrap_or_else(|| "{}".to_owned()),
            );

            if let (Some(creds), account) = source_data {
                write_private_file(account_json, &account)?;
                write_private_file(credentials_json, &creds)?;
                AuthProvisionOutcome::Synced
            } else {
                anyhow::bail!(
                    "Not a Claude config folder: {} has no .credentials.json and no matching \
                         macOS Keychain login. Select the folder you set as CLAUDE_CONFIG_DIR when \
                         you logged in to Claude.",
                    source_dir.display()
                );
            }
        }
    };

    let forward_auth = matches!(
        outcome,
        AuthProvisionOutcome::Synced
            | AuthProvisionOutcome::HostMissing
            | AuthProvisionOutcome::TokenMode
    );
    Ok((outcome, forward_auth))
}

/// Copy the host's `.claude.json` into the container state, or write `{}`
/// if the host file doesn't exist.
pub fn copy_host_claude_json(host_path: &Path, dest_path: &Path) -> anyhow::Result<()> {
    let content = read_source_text(host_path, "Claude account metadata")
        .with_context(|| format!("reading Claude account metadata at {}", host_path.display()))?
        .unwrap_or_else(|| "{}".to_owned());
    write_private_file(dest_path, &content)
}

/// Wipe the container's Claude auth state to a clean empty shape.
///
/// Used by every non-Sync mode (`Ignore`, `OAuthToken`, `ApiKey`) — they
/// all must guarantee no stale `.credentials.json` survives from a
/// prior Sync run, and that `.claude.json` is `{}` so Claude Code
/// inside the container authenticates exclusively via env vars (or
/// fresh login) rather than re-using forwarded credentials.
///
/// `account_json` is rewritten only when its current contents differ
/// from `{}` (or the file doesn't exist), to avoid touching mtime on
/// every launch.
pub(crate) fn wipe_claude_state(
    account_json: &Path,
    credentials_json: &Path,
) -> anyhow::Result<()> {
    write_private_file(account_json, "{}")?;
    wipe_file_if_present(credentials_json)?;
    Ok(())
}

/// Read the host's Claude Code OAuth credentials for the default
/// `~/.claude` config dir.
///
/// Checks the file-based store at `~/.claude/.credentials.json` first
/// (used on Linux, and makes the function testable with temp dirs).
/// Falls back to the macOS Keychain ("Claude Code-credentials") when
/// the file is absent and `host_home` matches the real home directory.
pub(crate) fn read_host_credentials(host_home: &Path) -> anyhow::Result<Option<String>> {
    // File-based credentials (Linux, or macOS with an explicit export).
    let creds_path = host_home.join(".claude/.credentials.json");
    if let Some(content) = read_source_text(&creds_path, "Claude credentials")?
        .filter(|content| !content.trim().is_empty())
    {
        return Ok(Some(content));
    }

    // macOS Keychain fallback — only attempted when host_home is the
    // real home directory.  This keeps tests hermetic (they use temp
    // dirs) while still supporting the Keychain in production.
    #[cfg(target_os = "macos")]
    if host_home_is_real(host_home) {
        return read_claude_keychain(jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE);
    }

    Ok(None)
}

/// Read the host's Claude Code OAuth credentials for an explicit
/// `CLAUDE_CONFIG_DIR` source folder (Workspace Auth sync mode).
///
/// Reads ONLY credentials belonging to `source_dir`: the file-based
/// `source_dir/.credentials.json` first, then — on macOS — the Keychain
/// entry Claude Code provisions for that specific config dir. It never
/// falls back to the default `~/.claude` credentials or the default
/// Keychain service; an operator who selected a source folder must get
/// that folder's account (e.g. a company Enterprise login) or nothing,
/// never the default Max account leaking in from the host.
#[cfg(unix)]
pub fn locked_claude_credentials(
    source: &auth_directory::LockedSource,
    source_dir: &Path,
    host_home: &Path,
) -> anyhow::Result<Option<String>> {
    let credentials = auth_directory::read_locked_source_file(
        &source.root,
        &[".credentials.json"],
        "Claude credentials",
    )?;
    if let Some(credentials) = credentials {
        let credentials = String::from_utf8(credentials)
            .context("Claude .credentials.json is not valid UTF-8")?;
        if !credentials.trim().is_empty() {
            return Ok(Some(credentials));
        }
    }

    #[cfg(target_os = "macos")]
    if host_home_is_real(host_home) {
        let scope = jackin_core::claude_keychain_scope(source_dir, host_home, source_dir)
            .ok_or_else(|| anyhow::anyhow!("invalid Claude config directory"))?;
        return read_claude_keychain(&scope.service);
    }

    let _ = (source_dir, host_home);
    Ok(None)
}

#[cfg(not(unix))]
pub fn read_host_credentials_from_claude_config_dir(
    source_dir: &Path,
    host_home: &Path,
) -> anyhow::Result<Option<String>> {
    // File-based credentials (Linux, or macOS with an explicit export).
    let creds_path = source_dir.join(".credentials.json");
    if let Some(content) = read_source_text(&creds_path, "Claude credentials")?
        .filter(|content| !content.trim().is_empty())
    {
        return Ok(Some(content));
    }

    // macOS Keychain — Claude Code stores per-config-dir credentials
    // under a service name derived from the config dir path. Gated on the
    // real home directory so tests stay hermetic (temp dirs never shell
    // out to `security`).
    #[cfg(target_os = "macos")]
    if host_home_is_real(host_home) {
        // Provisioning source dirs are already absolute; the shared core helper
        // normalizes and hashes the same path so instance and usage never drift.
        let scope = jackin_core::claude_keychain_scope(source_dir, host_home, source_dir)
            .ok_or_else(|| anyhow::anyhow!("invalid Claude config directory"))?;
        return read_claude_keychain(&scope.service);
    }

    #[cfg(not(target_os = "macos"))]
    let _ = host_home;
    Ok(None)
}

/// Read a credential blob from the macOS login Keychain under `service`.
/// Returns `None` on lookup failure or an empty value.
#[cfg(target_os = "macos")]
pub fn read_claude_keychain(service: &str) -> anyhow::Result<Option<String>> {
    let Ok(output) = crate::process_telemetry::exec_sync(&jackin_process::ExecRequest::new(
        "security",
        ["find-generic-password", "-s", service, "-w"],
    )) else {
        return Ok(None);
    };
    if output.success {
        let creds = String::from_utf8(output.stdout)
            .context("Claude Keychain credential is not valid UTF-8")?
            .trim()
            .to_owned();
        if !creds.is_empty() {
            return Ok(Some(creds));
        }
    }
    Ok(None)
}
