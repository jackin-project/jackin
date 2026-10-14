// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_instance_credentials::SyncSourceValidationError;

use jackin_config::{AiProvider, ProfileSelector};
use jackin_core::Agent;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{
    capture_locked_source, create_source_snapshot_dir, lock_amp_source_dir,
    snapshot_content_revision, validate_locked_sync_source_dir,
};
use jackin_instance_credentials::auth_directory;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthSourceDescriptor {
    pub agent: Agent,
    pub provider: Option<AiProvider>,
    pub selector: Option<ProfileSelector>,
    pub source_dir: PathBuf,
}

#[derive(Debug)]
pub struct SelectedAuthSourceSnapshotInner {
    descriptor: AuthSourceDescriptor,
    materialized_source: SelectedSourceDirectory,
    content_revision: String,
}

#[derive(Debug)]
pub struct SelectedSourceDirectory {
    #[cfg(unix)]
    pub owner: auth_directory::SnapshotDirectory,
    #[cfg(not(unix))]
    pub owner: tempfile::TempDir,
}

impl SelectedSourceDirectory {
    pub fn path(&self) -> &Path {
        self.owner.path()
    }
}

#[derive(Clone)]
pub struct SelectedAuthSourceSnapshot {
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
    pub fn descriptor(&self) -> &AuthSourceDescriptor {
        &self.inner.descriptor
    }

    pub fn materialized_source_dir(&self) -> &Path {
        self.inner.materialized_source.path()
    }

    pub fn content_revision(&self) -> &str {
        &self.inner.content_revision
    }
}

pub fn finish_source_snapshot(
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

pub fn claude_source_missing_error(source_dir: &Path) -> anyhow::Error {
    anyhow::anyhow!(
        "Not a Claude config folder: {} has no .credentials.json and no matching macOS Keychain \
         login. Select the folder you set as CLAUDE_CONFIG_DIR when you logged in to Claude.",
        source_dir.display()
    )
}

pub fn validate_sync_source_dir(
    agent: Agent,
    source_dir: &Path,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    validate_sync_source_dir_for_provider(agent, None, source_dir, host_home)
}

pub fn validate_sync_source_dir_for_provider(
    agent: Agent,
    provider: Option<AiProvider>,
    source_dir: &Path,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    validate_sync_source_dir_for_selection(agent, provider, None, source_dir, host_home)
}

pub fn validate_sync_source_dir_for_selection(
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

            Agent::Antigravity => {
                require_credential_file(source_dir, "settings.json", "Antigravity")
            }
            Agent::Gemini => require_credential_file(source_dir, "oauth_creds.json", "Gemini"),
            Agent::Cursor => require_credential_file(source_dir, "auth.json", "Cursor"),
            Agent::Muse => require_credential_file(source_dir, "auth.json", "Muse"),

            Agent::Omp | Agent::Hermes => {
                validate_store_source_dir(agent, provider, selector, source_dir, host_home)
            }
            Agent::Amp => {
                require_credential_file(&amp_credentials_dir(source_dir), "secrets.json", "Amp")
            }

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

pub fn capture_selected_source(
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
