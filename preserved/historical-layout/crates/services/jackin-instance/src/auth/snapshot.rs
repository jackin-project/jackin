// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Selected-source snapshots: descriptors, Sync capture, validation entry.

use crate::SyncSourceValidationError;

use jackin_config::{AiProvider, ProfileSelector};
use jackin_core::Agent;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::auth::{
    auth_directory, capture_locked_source, create_source_snapshot_dir, lock_amp_source_dir,
    snapshot_content_revision, validate_locked_sync_source_dir,
};

/// Maximum bytes read from one selected credential file while it is captured
/// for launch. Credential sources are operator-owned input, so every read
/// must have a finite bound before the bytes cross into a worker thread.
pub(crate) const MAX_AUTH_SOURCE_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum aggregate bytes copied from one selected directory source.
pub(crate) const MAX_AUTH_SOURCE_TREE_BYTES: usize = 32 * 1024 * 1024;
/// Maximum entries copied from one selected directory source.
pub(crate) const MAX_AUTH_SOURCE_TREE_ENTRIES: usize = 4096;

/// Secret-free identity for one selected profile source.
///
/// `source_dir` is the original descriptor path. The snapshot itself is a
/// separate host-private directory and is exposed through
/// [`SelectedAuthSourceSnapshot::materialized_source_dir`]. Keeping these
/// values together prevents a worker from accidentally pairing bytes with a
/// different provider or profile selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthSourceDescriptor {
    pub(crate) agent: Agent,
    pub(crate) provider: Option<AiProvider>,
    pub(crate) selector: Option<ProfileSelector>,
    pub(crate) source_dir: PathBuf,
}

pub(crate) struct SelectedAuthSourceSnapshotInner {
    descriptor: AuthSourceDescriptor,
    materialized_source: SelectedSourceDirectory,
    content_revision: String,
}

/// Owned materialized source tree. Unix keeps the parent directory descriptor
/// and child name so cleanup remains descriptor-relative even if a pathname is
/// replaced while workers retain the snapshot.
pub(crate) struct SelectedSourceDirectory {
    #[cfg(unix)]
    pub(crate) owner: auth_directory::SnapshotDirectory,
    #[cfg(not(unix))]
    pub(crate) owner: tempfile::TempDir,
}

impl SelectedSourceDirectory {
    pub(crate) fn path(&self) -> &Path {
        self.owner.path()
    }
}

/// Immutable selected-source material retained across launch worker threads.
///
/// Capture owns a descriptor-relative read of the selected source and stores
/// only the validated bytes in a private temporary tree. Workers may use the
/// materialized path, but never need to reopen the original source path.
#[derive(Clone)]
pub(crate) struct SelectedAuthSourceSnapshot {
    inner: Arc<SelectedAuthSourceSnapshotInner>,
}

impl std::fmt::Debug for SelectedAuthSourceSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectedAuthSourceSnapshot")
            .field("descriptor", &self.inner.descriptor)
            .field("content_revision", &self.inner.content_revision)
            .finish_non_exhaustive()
    }
}

impl SelectedAuthSourceSnapshot {
    /// Descriptor that was bound to the captured bytes.
    pub(crate) fn descriptor(&self) -> &AuthSourceDescriptor {
        &self.inner.descriptor
    }

    /// Host-private source tree containing the captured, validated bytes.
    pub(crate) fn materialized_source_dir(&self) -> &Path {
        self.inner.materialized_source.path()
    }

    /// Stable digest of the captured opaque source content.
    pub(crate) fn content_revision(&self) -> &str {
        &self.inner.content_revision
    }
}

pub(crate) fn finish_source_snapshot(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    materialized_source: SelectedSourceDirectory,
) -> anyhow::Result<SelectedAuthSourceSnapshot> {
    let content_revision = snapshot_content_revision(materialized_source.path())?;
    Ok(SelectedAuthSourceSnapshot {
        inner: Arc::new(SelectedAuthSourceSnapshotInner {
            descriptor: AuthSourceDescriptor {
                agent,
                provider,
                selector: selector.cloned(),
                source_dir: source_dir.to_path_buf(),
            },
            materialized_source,
            content_revision,
        }),
    })
}

pub(crate) fn claude_source_missing_error(source_dir: &Path) -> anyhow::Error {
    anyhow::anyhow!(
        "Not a Claude config folder: {} has no .credentials.json and no matching macOS Keychain \
         login. Select the folder you set as CLAUDE_CONFIG_DIR when you logged in to Claude.",
        source_dir.display()
    )
}

/// Validate that `source_dir` carries the credential structure `agent`
/// expects for sync-mode auth forwarding.
///
/// Returns `Ok(())` when the folder holds usable credentials for that
/// agent, or `Err` describing what is missing. The message is
/// shown verbatim in the Source Folder picker so an operator cannot
/// silently select a folder that yields no credentials (and, for Claude,
/// would otherwise leak the default account into the capsule).
///
/// `host_home` is the operator's real home directory; it gates the macOS
/// Keychain probe used to validate a file-less Claude config dir and is
/// otherwise unused.
pub fn validate_sync_source_dir(
    agent: Agent,
    source_dir: &Path,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    validate_sync_source_dir_for_provider(agent, None, source_dir, host_home)
}

/// Validate one sync source with the selected provider identity when the
/// agent has a multi-provider store. `OpenCode`'s `auth.json` is a single
/// source file containing several independent credentials; the provider must
/// therefore travel with the account binding or an ambiguous source is
/// rejected before launch.
pub(crate) fn validate_sync_source_dir_for_provider(
    agent: Agent,
    provider: Option<AiProvider>,
    source_dir: &Path,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    validate_sync_source_dir_for_selection(agent, provider, None, source_dir, host_home)
}

/// Validate one sync source with both its provider and immutable store
/// selector. Omp and Hermes are only accepted when discovery proves the
/// source still contains exactly the selected one-account store.
pub(crate) fn validate_sync_source_dir_for_selection(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    #[cfg(unix)]
    {
        let source = if agent == Agent::Amp {
            lock_amp_source_dir(source_dir).map_err(|error| {
                SyncSourceValidationError::new(format!("source rejected: {error:#}"))
            })?
        } else {
            auth_directory::lock_source_dir(source_dir).map_err(|error| {
                SyncSourceValidationError::new(format!("source rejected: {error:#}"))
            })?
        };
        let source = source.ok_or_else(|| {
            SyncSourceValidationError::new(format!("{} is not a directory.", source_dir.display()))
        })?;
        validate_locked_sync_source_dir(agent, provider, selector, source_dir, host_home, &source)
    }

    #[cfg(not(unix))]
    {
        let Ok(source_metadata) = std::fs::symlink_metadata(source_dir) else {
            return Err(SyncSourceValidationError::new(format!(
                "{} is not a directory.",
                source_dir.display()
            )));
        };
        if source_metadata.file_type().is_symlink() {
            return Err(SyncSourceValidationError::new(format!(
                "{} is a symlink; source folders must be real directories.",
                source_dir.display()
            )));
        }
        if !source_metadata.is_dir() {
            return Err(SyncSourceValidationError::new(format!(
                "{} is not a directory.",
                source_dir.display()
            )));
        }
        match agent {
            // Claude has no single credential file on macOS — the login lives
            // in the Keychain — so accept either the file or a matching
            // Keychain entry for this exact config dir.
            Agent::Claude => {
                if read_host_credentials_from_claude_config_dir(source_dir, host_home)
                    .ok()
                    .flatten()
                    .is_some()
                {
                    Ok(())
                } else {
                    Err(SyncSourceValidationError::new(format!(
                        "Not a Claude config folder: {} has no .credentials.json and no matching \
                     macOS Keychain login. Select the folder you set as CLAUDE_CONFIG_DIR when \
                     you logged in to Claude.",
                        source_dir.display()
                    )))
                }
            }
            Agent::Codex => require_credential_file(source_dir, "auth.json", "Codex"),
            Agent::Grok => require_credential_file(source_dir, "auth.json", "Grok"),
            Agent::Opencode => validate_opencode_source_dir(source_dir, provider),
            // Sync carries prefs only; the OAuth grant stays in the host Keychain.
            Agent::Antigravity => {
                require_credential_file(source_dir, "settings.json", "Antigravity")
            }
            Agent::Gemini => require_credential_file(source_dir, "oauth_creds.json", "Gemini"),
            Agent::Cursor => require_credential_file(source_dir, "auth.json", "Cursor"),
            Agent::Muse => require_credential_file(source_dir, "auth.json", "Muse"),
            // Store-backed agents are re-enumerated and must still resolve to the
            // selected single-account source before their whole store is copied.
            Agent::Omp | Agent::Hermes => {
                validate_store_source_dir(agent, provider, selector, source_dir, host_home)
            }
            Agent::Amp => {
                require_credential_file(&amp_credentials_dir(source_dir), "secrets.json", "Amp")
            }
            // Kimi syncs a directory tree rather than a single file.
            Agent::Kimi => {
                let config = std::fs::symlink_metadata(source_dir.join("config.toml"));
                let credentials = std::fs::symlink_metadata(source_dir.join("credentials"));
                if config.is_ok_and(|metadata| metadata.is_file())
                    && credentials.is_ok_and(|metadata| metadata.is_dir())
                {
                    Ok(())
                } else {
                    Err(SyncSourceValidationError::new(format!(
                        "Not a Kimi config folder: {} must contain config.toml and a credentials/ \
                     directory.",
                        source_dir.display()
                    )))
                }
            }
        }
    }
}

/// Capture one selected profile source before launch worker threads begin.
///
/// The source directory is locked and traversed descriptor-relatively on Unix
/// for the whole capture. The returned snapshot owns a private temporary tree;
/// callers must retain it until provisioning completes and pass
/// [`SelectedAuthSourceSnapshot::materialized_source_dir`] to the existing
/// provisioner. `snapshot_parent` must be a host-private directory that is
/// not mounted into the capsule. A missing source directory returns
/// `Ok(None)`; a source that exists but no longer contains the selected
/// credential fails closed.
pub(crate) fn capture_selected_source(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    host_home: &Path,
    snapshot_parent: &Path,
) -> anyhow::Result<Option<SelectedAuthSourceSnapshot>> {
    #[cfg(unix)]
    {
        let source = if agent == Agent::Amp {
            lock_amp_source_dir(source_dir)?
        } else {
            auth_directory::lock_source_dir(source_dir)?
        };
        let Some(source) = source else {
            return Ok(None);
        };
        let snapshot = create_source_snapshot_dir(snapshot_parent)?;
        capture_locked_source(
            agent,
            provider,
            selector,
            source_dir,
            host_home,
            &source,
            snapshot.path(),
        )?;
        finish_source_snapshot(agent, provider, selector, source_dir, snapshot).map(Some)
    }

    #[cfg(not(unix))]
    {
        let source_metadata = match std::fs::symlink_metadata(source_dir) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(
            source_metadata.is_dir() && !source_metadata.file_type().is_symlink(),
            "source auth directory {} is not a real directory",
            source_dir.display()
        );
        let snapshot = create_source_snapshot_dir(snapshot_parent)?;
        capture_unixless_source(
            agent,
            provider,
            selector,
            source_dir,
            host_home,
            snapshot.path(),
        )?;
        finish_source_snapshot(agent, provider, selector, source_dir, snapshot).map(Some)
    }
}
