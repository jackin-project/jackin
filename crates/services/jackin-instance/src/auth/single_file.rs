// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Single blob/file credential helpers shared by simple agents.

#![expect(
    clippy::print_stderr,
    reason = "credential provisioning warnings are operator-visible launch diagnostics"
)]

use crate::AuthProvisionOutcome;

use jackin_config::AuthForwardMode;

use std::path::{Path, PathBuf};

use crate::auth::{
    auth_directory, private_file_exists, read_source_bytes, read_source_text, reject_auth_path,
    repair_permissions, wipe_file_if_present, write_private_bytes, write_private_file,
};
use zeroize::Zeroizing;

/// Byte-oriented twin of [`provision_single_file_credential`] for binary
/// single-file stores (omp's `SQLite` `agent.db`). Same outcome/mount
/// contract; an empty host file counts as host-missing.
pub(crate) fn provision_single_blob_credential(
    target: &Path,
    host_path: &Path,
    mode: AuthForwardMode,
    label: &str,
    agent_name: &str,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    let content = if mode == AuthForwardMode::Sync {
        read_source_bytes(host_path, &format!("{agent_name} {label}"))?.map(Zeroizing::new)
    } else {
        None
    };
    provision_single_blob_credential_from_content(target, mode, content, label, agent_name)
}

pub(crate) fn provision_single_blob_credential_from_content(
    target: &Path,
    mode: AuthForwardMode,
    content: Option<Zeroizing<Vec<u8>>>,
    label: &str,
    agent_name: &str,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    use anyhow::Context;

    reject_auth_path(target)?;

    let outcome = match mode {
        AuthForwardMode::OAuthToken => {
            eprintln!(
                "[jackin] internal: {agent_name} provision received unsupported \
                 OAuthToken mode — parser invariant bypassed; \
                 wiping role state and falling back to token-mode."
            );
            wipe_agent_file_state(target, label)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::ApiKey => {
            wipe_agent_file_state(target, label)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Ignore => {
            wipe_agent_file_state(target, label)?;
            AuthProvisionOutcome::Skipped
        }
        AuthForwardMode::Sync => match content {
            Some(content) if content.is_empty() => {
                eprintln!(
                    "[jackin] host {agent_name} credential is empty — treating as host-missing"
                );
                repair_permissions(target)?;
                AuthProvisionOutcome::HostMissing
            }
            Some(content) => {
                write_private_bytes(target, content.as_slice()).with_context(|| {
                    format!(
                        "failed to write {agent_name} role-state {label} at {}",
                        target.display()
                    )
                })?;
                AuthProvisionOutcome::Synced
            }
            None => {
                repair_permissions(target)?;
                AuthProvisionOutcome::HostMissing
            }
        },
    };

    let mounted = match outcome {
        AuthProvisionOutcome::Synced => Some(target.to_path_buf()),
        AuthProvisionOutcome::Skipped => None,
        AuthProvisionOutcome::HostMissing | AuthProvisionOutcome::TokenMode => {
            private_file_exists(target)?.then(|| target.to_path_buf())
        }
    };
    Ok((outcome, mounted))
}

/// Shared file-credential provisioner for agents that use a single JSON
/// credential file with standard `AuthForwardMode` semantics.
///
/// `treat_empty_as_missing` — when `true`, an empty/whitespace file on the
/// host is treated as host-missing and invalidates stale role-state auth.
/// When `false`, an empty file is written as-is.
///
/// `warn_on_oauth` — when `true`, receiving `OAuthToken` mode logs a warning
/// that the parser invariant was bypassed. When `false`, `OAuthToken`
/// silently returns `TokenMode`.
///
/// `wipe_on_oauth` — when `true`, the role-state file is wiped on `OAuthToken`.
/// When `false`, the existing file is preserved (Codex: `OAuthToken` is a
/// parser-rejected no-op; preserving the file allows recovery from a bypass).
#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn provision_single_file_credential(
    target: &Path,
    host_path: &Path,
    mode: AuthForwardMode,
    label: &str,
    agent_name: &str,
    treat_empty_as_missing: bool,
    warn_on_oauth: bool,
    wipe_on_oauth: bool,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    let content = if mode == AuthForwardMode::Sync {
        read_source_text(host_path, label)?
    } else {
        None
    };
    provision_single_file_credential_with_content(
        target,
        mode,
        content,
        label,
        agent_name,
        treat_empty_as_missing,
        warn_on_oauth,
        wipe_on_oauth,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "content and mode policy are deliberately explicit at this security boundary"
)]
pub(crate) fn provision_single_file_credential_with_content(
    target: &Path,
    mode: AuthForwardMode,
    content: Option<String>,
    label: &str,
    agent_name: &str,
    treat_empty_as_missing: bool,
    warn_on_oauth: bool,
    wipe_on_oauth: bool,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    use anyhow::Context;

    reject_auth_path(target)?;

    let mut retain_existing_on_missing = true;
    let outcome = match mode {
        AuthForwardMode::OAuthToken => {
            if warn_on_oauth {
                eprintln!(
                    "[jackin] internal: {agent_name} provision received unsupported \
                     OAuthToken mode — parser invariant bypassed; \
                     wiping role state and falling back to token-mode."
                );
            }
            if wipe_on_oauth {
                wipe_agent_file_state(target, label)?;
            }
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::ApiKey => {
            wipe_agent_file_state(target, label)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Ignore => {
            wipe_agent_file_state(target, label)?;
            AuthProvisionOutcome::Skipped
        }
        AuthForwardMode::Sync => match content {
            Some(content) if treat_empty_as_missing && content.trim().is_empty() => {
                eprintln!("[jackin] host {label} is empty/whitespace — treating as host-missing");
                // Empty input is an invalid source, not an absent host login:
                // invalidate any persisted role-state credential before mount
                // admission so a later launch cannot reuse stale auth.
                retain_existing_on_missing = false;
                wipe_agent_file_state(target, label)?;
                AuthProvisionOutcome::HostMissing
            }
            Some(content) => {
                // No-churn guard: skip the atomic write when the role-state
                // file already holds identical content. `write_private_file`
                // replaces the inode (temp + rename); on macOS that
                // invalidates a live single-file bind mount into the running
                // container. The background sibling-auth prewarm
                // (`prewarm_auth_for_agents`, spawned during launch) re-runs
                // this for already-foreground-provisioned agents, so an
                // unconditional rename races `docker create` and silently
                // breaks the foreground container's auth mounts — leaving the
                // sibling agent unauthenticated. Mirrors the GitHub
                // provisioner's no-churn guard.
                write_private_file(target, &content).with_context(|| {
                    format!(
                        "failed to write {agent_name} role-state {label} at {}",
                        target.display()
                    )
                })?;
                AuthProvisionOutcome::Synced
            }
            None => {
                repair_permissions(target)?;
                AuthProvisionOutcome::HostMissing
            }
        },
    };

    let mounted = match outcome {
        AuthProvisionOutcome::Synced => Some(target.to_path_buf()),
        AuthProvisionOutcome::Skipped => None,
        AuthProvisionOutcome::HostMissing => {
            if retain_existing_on_missing && private_file_exists(target)? {
                Some(target.to_path_buf())
            } else {
                None
            }
        }
        AuthProvisionOutcome::TokenMode => {
            private_file_exists(target)?.then(|| target.to_path_buf())
        }
    };
    Ok((outcome, mounted))
}

/// Wipe a single credential file from role state.
///
/// `label` names the agent + file for the operator-visible error message
/// (e.g. `"Amp secrets.json"`, `"OpenCode auth.json"`).
pub(crate) fn wipe_agent_file_state(path: &Path, label: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    wipe_file_if_present(path).with_context(|| {
        format!(
            "failed to wipe stale {label} at {} \
             (auth_forward switched to ignore/api_key); remove the file \
             manually if it has unexpected ownership",
            path.display()
        )
    })
}

/// Remove role-state Kimi auth files so a prior Sync run cannot leak
/// credentials under env-driven modes.
pub(crate) fn wipe_kimi_state(kimi_dir: &Path) -> anyhow::Result<()> {
    auth_directory::wipe_auth_directory(kimi_dir).map_err(|error| {
        anyhow::anyhow!(format!(
            "failed to wipe stale Kimi state at {}: {error:#} \
                 (auth_forward switched to ignore/api_key); remove the directory \
                 manually if it has unexpected ownership",
            kimi_dir.display()
        ))
    })?;
    Ok(())
}
