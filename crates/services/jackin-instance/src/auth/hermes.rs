// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Hermes credential provisioning.

#![expect(
    clippy::print_stderr,
    reason = "credential provisioning warnings are operator-visible launch diagnostics"
)]

use crate::{AuthProvisionOutcome, RoleState};

use anyhow::Context;
use jackin_config::{AiProvider, AuthForwardMode, ProfileSelector};
use jackin_core::Agent;
use std::path::Path;

use crate::auth::{auth_directory, validate_store_source_dir};

#[cfg(unix)]
pub(crate) fn create_hermes_source_snapshot(
    hermes_dir: &Path,
    source_dir: &Path,
) -> anyhow::Result<auth_directory::SnapshotDirectory> {
    let source_parent = source_dir
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let source_parent_error = if let Some(parent) = source_parent {
        match auth_directory::create_snapshot_directory(parent) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => Some(error),
        }
    } else {
        None
    };
    let sidecar_parent = hermes_dir
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Hermes target has no protected parent"))?
        .join(".jackin-auth-source-snapshots");
    auth_directory::create_snapshot_directory(&sidecar_parent).with_context(|| {
        source_parent_error.map_or_else(
            || "creating Hermes source snapshot".to_owned(),
            |error| format!("creating Hermes source snapshot (source parent failed: {error:#})"),
        )
    })
}

impl RoleState {
    /// Provision Hermes's host-side `~/.hermes/` dir per the chosen mode.
    ///
    /// Best-effort layout (unverified upstream): forwards `config.yaml`,
    /// `.env`, `auth.json` when present plus a `profiles/` subtree.
    pub(crate) fn provision_hermes_auth(
        hermes_dir: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
        provider: Option<AiProvider>,
        selector: Option<&ProfileSelector>,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        Self::provision_hermes_auth_from_source_dir(
            hermes_dir,
            mode,
            &host_home.join(".hermes"),
            provider,
            selector,
        )
    }

    pub(crate) fn provision_hermes_auth_from_source_dir(
        hermes_dir: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
        provider: Option<AiProvider>,
        selector: Option<&ProfileSelector>,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        #[cfg(unix)]
        if mode == AuthForwardMode::Sync {
            let live_source = auth_directory::lock_source_dir(source_dir)?;
            let Some(source) = live_source.as_ref() else {
                return provision_hermes_dir_credential_with_locked_source(
                    hermes_dir, source_dir, mode, None,
                );
            };
            // A selected source already lives beneath the protected
            // `source-snapshots` parent captured for this launch. Reuse that
            // parent for the validation copy; legacy callers fall back to a
            // private sidecar beneath the role target instead of ambient /tmp.
            let snapshot = create_hermes_source_snapshot(hermes_dir, source_dir)?;
            let snapshot_root = auth_directory::open_directory_path(snapshot.path())?;
            auth_directory::snapshot_source(&source.root, &snapshot_root)?;
            validate_store_source_dir(
                Agent::Hermes,
                provider,
                selector,
                snapshot.path(),
                snapshot.path(),
            )?;
            auth_directory::run_hermes_snapshot_hook();
            // Keep the source lock held until the snapshot has been
            // validated and the descriptor-safe destination swap has
            // completed. The staged copy reads only the immutable
            // snapshot, not the live source.
            return provision_hermes_dir_credential_with_locked_source(
                hermes_dir,
                snapshot.path(),
                mode,
                Some(auth_directory::LockedSource {
                    root: snapshot_root,
                }),
            );
        }
        provision_hermes_dir_credential(hermes_dir, source_dir, mode)
    }
}

/// Directory credential provisioner for Hermes's multi-file store, mirroring
/// [`provision_kimi_dir_credential`] semantics (OAuthToken/ApiKey/Ignore wipe
/// the dir; Sync copies known files + a `profiles/` subtree).
///
/// Returns `(outcome, forward_auth)` where `forward_auth` is `true` when the
/// role-state directory should be bind-mounted into the container.
pub(crate) fn provision_hermes_dir_credential(
    target_dir: &Path,
    host_dir: &Path,
    mode: AuthForwardMode,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    #[cfg(unix)]
    let source = if mode == AuthForwardMode::Sync {
        auth_directory::lock_source_dir(host_dir)?
    } else {
        None
    };
    #[cfg(not(unix))]
    let source = ();
    provision_hermes_dir_credential_with_locked_source(target_dir, host_dir, mode, source)
}

pub(crate) fn provision_hermes_dir_credential_with_locked_source(
    target_dir: &Path,
    host_dir: &Path,
    mode: AuthForwardMode,
    #[cfg(unix)] source: Option<auth_directory::LockedSource>,
    #[cfg(not(unix))] _source: (),
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    // Best-effort file set; the layout is unverified upstream.
    pub(crate) const SYNC_FILES: &[&str] = &["config.yaml", ".env", "auth.json"];

    let outcome = match mode {
        AuthForwardMode::OAuthToken => {
            eprintln!(
                "[jackin] internal: Hermes provision received unsupported \
                 OAuthToken mode — parser invariant bypassed; \
                 wiping role state and falling back to token-mode."
            );
            wipe_hermes_state(target_dir)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::ApiKey => {
            wipe_hermes_state(target_dir)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Ignore => {
            wipe_hermes_state(target_dir)?;
            AuthProvisionOutcome::Skipped
        }
        AuthForwardMode::Sync => {
            #[cfg(unix)]
            {
                auth_directory::stage_auth_directory_with_locked_source(
                    target_dir,
                    host_dir,
                    source,
                    |host_dir, source, staged| {
                        for name in SYNC_FILES {
                            auth_directory::copy_optional_source_file(
                                source,
                                name,
                                staged,
                                name,
                                &format!("reading {}", host_dir.join(name).display()),
                            )?;
                        }
                        auth_directory::copy_optional_source_tree(
                            source,
                            "profiles",
                            staged,
                            "profiles",
                            &format!("copying {}", host_dir.join("profiles").display()),
                        )?;
                        Ok(())
                    },
                )?
            }
            #[cfg(not(unix))]
            {
                auth_directory::stage_auth_directory(
                    target_dir,
                    host_dir,
                    |host_dir, source, staged| {
                        for name in SYNC_FILES {
                            auth_directory::copy_optional_source_file(
                                source,
                                name,
                                staged,
                                name,
                                &format!("reading {}", host_dir.join(name).display()),
                            )?;
                        }
                        auth_directory::copy_optional_source_tree(
                            source,
                            "profiles",
                            staged,
                            "profiles",
                            &format!("copying {}", host_dir.join("profiles").display()),
                        )?;
                        Ok(())
                    },
                )?
            }
        }
    };

    let forward_auth = matches!(
        outcome,
        AuthProvisionOutcome::Synced | AuthProvisionOutcome::HostMissing
    );
    Ok((outcome, forward_auth))
}

/// Remove role-state Hermes auth files so a prior Sync run cannot leak
/// credentials under env-driven modes.
pub(crate) fn wipe_hermes_state(hermes_dir: &Path) -> anyhow::Result<()> {
    auth_directory::wipe_auth_directory(hermes_dir).map_err(|error| {
        anyhow::anyhow!(format!(
            "failed to wipe stale Hermes state at {}: {error:#} \
                 (auth_forward switched to ignore/api_key); remove the directory \
                 manually if it has unexpected ownership",
            hermes_dir.display()
        ))
    })?;
    Ok(())
}
