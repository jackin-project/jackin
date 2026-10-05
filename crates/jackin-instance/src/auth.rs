// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Agent credential provisioning: copies or wipes per-agent auth files in the
//! role-state directory before container launch.
//!
//! Implements `RoleState` methods for each supported agent (`Claude`, `Codex`,
//! `Amp`, `Kimi`, `OpenCode`). Each provisioner applies the `AuthForwardMode`
//! policy (`Sync`, `ApiKey`, `OAuthToken`, `Ignore`) to decide whether to
//! copy the host credential file, leave it, or wipe it.
//!
//! Invariant: any symlink at an auth-file path is rejected before branching
//! on mode — a compromised role cannot redirect a provisioning write through
//! a symlink placed between launches.

#![expect(
    clippy::print_stderr,
    reason = "credential provisioning warnings are operator-visible launch diagnostics"
)]

use super::{
    AuthProvisionOutcome, GithubAuthContext, GithubProvisionOutcome, GithubTokenSource,
    HostMissingReason, RoleState,
};
use crate::{InstanceError, SyncSourceValidationError};
use anyhow::Context;
use jackin_config::{AiProvider, AuthForwardMode, GithubAuthMode, ProfileSelector};
use jackin_core::Agent;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

static AUTH_DIRECTORY_SWAP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Maximum bytes read from one selected credential file while it is captured
/// for launch. Credential sources are operator-owned input, so every read
/// must have a finite bound before the bytes cross into a worker thread.
const MAX_AUTH_SOURCE_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum aggregate bytes copied from one selected directory source.
const MAX_AUTH_SOURCE_TREE_BYTES: usize = 32 * 1024 * 1024;
/// Maximum entries copied from one selected directory source.
const MAX_AUTH_SOURCE_TREE_ENTRIES: usize = 4096;
/// Match the OMP store reader's independent database and WAL bounds.
const MAX_OMP_SOURCE_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Retry a source pair when OMP changes it during the bounded snapshot read.
const OMP_SOURCE_CAPTURE_ATTEMPTS: usize = 3;

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

struct SelectedAuthSourceSnapshotInner {
    descriptor: AuthSourceDescriptor,
    materialized_source: SelectedSourceDirectory,
    content_revision: String,
}

/// Owned materialized source tree. Unix keeps the parent directory descriptor
/// and child name so cleanup remains descriptor-relative even if a pathname is
/// replaced while workers retain the snapshot.
struct SelectedSourceDirectory {
    #[cfg(unix)]
    owner: auth_directory::SnapshotDirectory,
    #[cfg(not(unix))]
    owner: tempfile::TempDir,
}

impl SelectedSourceDirectory {
    fn path(&self) -> &Path {
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

fn finish_source_snapshot(
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

fn claude_source_missing_error(source_dir: &Path) -> anyhow::Error {
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

fn create_source_snapshot_dir(parent: &Path) -> anyhow::Result<SelectedSourceDirectory> {
    #[cfg(unix)]
    {
        Ok(SelectedSourceDirectory {
            owner: auth_directory::create_snapshot_directory(parent)?,
        })
    }

    #[cfg(not(unix))]
    {
        match std::fs::symlink_metadata(parent) {
            Ok(metadata) => {
                anyhow::ensure!(
                    metadata.is_dir() && !metadata.file_type().is_symlink(),
                    "auth snapshot parent {} is not a real directory",
                    parent.display()
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("creating auth snapshot parent {}", parent.display())
                })?;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("opening auth snapshot parent {}", parent.display()));
            }
        }
        let metadata = std::fs::symlink_metadata(parent)
            .with_context(|| format!("opening auth snapshot parent {}", parent.display()))?;
        anyhow::ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "auth snapshot parent {} is not a real directory",
            parent.display()
        );
        let owner = tempfile::Builder::new()
            .prefix(".jackin-auth-source-")
            .tempdir_in(parent)
            .context("creating selected auth source snapshot")?;
        Ok(SelectedSourceDirectory { owner })
    }
}

#[cfg(unix)]
fn capture_locked_source(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    host_home: &Path,
    source: &auth_directory::LockedSource,
    snapshot_root: &Path,
) -> anyhow::Result<()> {
    match agent {
        Agent::Claude => capture_locked_claude_source(source, source_dir, host_home, snapshot_root),
        Agent::Codex => capture_locked_single_file_source(
            source,
            "auth.json",
            "auth.json",
            "Codex auth.json",
            snapshot_root,
        ),
        Agent::Grok => capture_locked_single_file_source(
            source,
            "auth.json",
            "auth.json",
            "Grok auth.json",
            snapshot_root,
        ),
        Agent::Opencode => capture_locked_opencode_source(source, provider, snapshot_root),
        Agent::Antigravity => capture_locked_single_file_source(
            source,
            "settings.json",
            "settings.json",
            "Antigravity settings.json",
            snapshot_root,
        ),
        Agent::Gemini => capture_locked_single_file_source(
            source,
            "oauth_creds.json",
            "oauth_creds.json",
            "Gemini oauth_creds.json",
            snapshot_root,
        ),
        Agent::Cursor => capture_locked_single_file_source(
            source,
            "auth.json",
            "auth.json",
            "Cursor auth.json",
            snapshot_root,
        ),
        Agent::Muse => capture_locked_single_file_source(
            source,
            "auth.json",
            "auth.json",
            "Muse auth.json",
            snapshot_root,
        ),
        Agent::Amp => capture_locked_single_file_source(
            source,
            "secrets.json",
            "secrets.json",
            "Amp secrets.json",
            snapshot_root,
        ),
        Agent::Kimi => {
            validate_kimi_locked_source(source, source_dir)?;
            let snapshot = auth_directory::open_directory_path(snapshot_root)?;
            auth_directory::snapshot_source(&source.root, &snapshot)
        }
        Agent::Omp => {
            let content = capture_omp_database_snapshot(&source.root, source_dir)?;
            write_snapshot_bytes(snapshot_root, Path::new("agent/agent.db"), &content)?;
            validate_omp_store_content(&content, provider, selector)
        }
        Agent::Hermes => {
            let snapshot = auth_directory::open_directory_path(snapshot_root)?;
            auth_directory::snapshot_source(&source.root, &snapshot)?;
            validate_store_source_dir(
                Agent::Hermes,
                provider,
                selector,
                snapshot_root,
                snapshot_root,
            )
            .map_err(anyhow::Error::from)
        }
    }
}

#[cfg(unix)]
fn capture_locked_claude_source(
    source: &auth_directory::LockedSource,
    source_dir: &Path,
    host_home: &Path,
    snapshot_root: &Path,
) -> anyhow::Result<()> {
    let credentials = auth_directory::read_locked_source_file(
        &source.root,
        &[".credentials.json"],
        "Claude credentials",
    )?;
    let credentials = match credentials {
        Some(bytes) => {
            let text =
                String::from_utf8(bytes).context("Claude .credentials.json is not valid UTF-8")?;
            (!text.trim().is_empty()).then_some(text)
        }
        None => None,
    };
    #[cfg(target_os = "macos")]
    let credentials = if let Some(credentials) = credentials {
        credentials
    } else if host_home_is_real(host_home) {
        let scope = jackin_core::claude_keychain_scope(source_dir, host_home, source_dir)
            .ok_or_else(|| anyhow::anyhow!("invalid Claude config directory"))?;
        read_claude_keychain(&scope.service)?
            .ok_or_else(|| claude_source_missing_error(source_dir))?
    } else {
        return Err(claude_source_missing_error(source_dir));
    };

    #[cfg(not(target_os = "macos"))]
    let Some(credentials) = credentials else {
        let _ = host_home;
        return Err(claude_source_missing_error(source_dir));
    };

    let account = auth_directory::read_locked_source_file(
        &source.root,
        &[".claude.json"],
        "Claude account metadata",
    )?
    .map(|bytes| String::from_utf8(bytes).context("Claude account metadata is not valid UTF-8"))
    .transpose()?
    .unwrap_or_else(|| "{}".to_owned());
    write_snapshot_bytes(
        snapshot_root,
        Path::new(".credentials.json"),
        credentials.as_bytes(),
    )?;
    write_snapshot_bytes(snapshot_root, Path::new(".claude.json"), account.as_bytes())
}

#[cfg(unix)]
fn capture_locked_single_file_source(
    source: &auth_directory::LockedSource,
    source_name: &str,
    snapshot_name: &str,
    label: &str,
    snapshot_root: &Path,
) -> anyhow::Result<()> {
    let bytes = auth_directory::read_locked_source_file(&source.root, &[source_name], label)?
        .ok_or_else(|| anyhow::anyhow!("{label} is missing"))?;
    let text =
        String::from_utf8(bytes.clone()).with_context(|| format!("{label} is not valid UTF-8"))?;
    anyhow::ensure!(!text.trim().is_empty(), "{label} is empty");
    write_snapshot_bytes(snapshot_root, Path::new(snapshot_name), text.as_bytes())
}

#[cfg(unix)]
fn capture_locked_opencode_source(
    source: &auth_directory::LockedSource,
    provider: Option<AiProvider>,
    snapshot_root: &Path,
) -> anyhow::Result<()> {
    let bytes = auth_directory::read_locked_source_file(
        &source.root,
        &["auth.json"],
        "OpenCode auth.json",
    )?
    .ok_or_else(|| anyhow::anyhow!("OpenCode auth.json is missing"))?;
    let content = String::from_utf8(bytes).context("OpenCode auth.json is not valid UTF-8")?;
    anyhow::ensure!(!content.trim().is_empty(), "OpenCode auth.json is empty");
    let value = serde_json::from_str::<serde_json::Value>(&content)
        .context("OpenCode auth.json is malformed")?;
    let (key, entry) = select_opencode_auth_entry(&value, provider).map_err(|reason| {
        anyhow::anyhow!("OpenCode auth.json cannot be selected safely: {reason}")
    })?;
    let mut selected = serde_json::Map::new();
    selected.insert(key.to_owned(), entry.clone());
    let selected = serde_json::to_vec(&serde_json::Value::Object(selected))
        .context("serializing selected OpenCode credential")?;
    write_snapshot_bytes(snapshot_root, Path::new("auth.json"), &selected)
}

#[cfg(not(unix))]
fn capture_unixless_source(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    host_home: &Path,
    snapshot_root: &Path,
) -> anyhow::Result<()> {
    match agent {
        Agent::Claude => {
            let credentials = read_host_credentials_from_claude_config_dir(source_dir, host_home)?
                .ok_or_else(|| anyhow::anyhow!("Claude credentials are missing"))?;
            let account =
                read_source_text(&source_dir.join(".claude.json"), "Claude account metadata")?
                    .unwrap_or_else(|| "{}".to_owned());
            write_snapshot_bytes(
                snapshot_root,
                Path::new(".credentials.json"),
                credentials.as_bytes(),
            )?;
            write_snapshot_bytes(snapshot_root, Path::new(".claude.json"), account.as_bytes())
        }
        Agent::Amp => capture_unixless_single_file_source(
            &amp_credentials_dir(source_dir),
            "secrets.json",
            "secrets.json",
            "Amp secrets.json",
            snapshot_root,
        ),
        Agent::Kimi | Agent::Hermes => {
            copy_unixless_source_tree(source_dir, snapshot_root)?;
            if agent == Agent::Kimi {
                validate_kimi_source_dir_unixless(snapshot_root)?;
            } else {
                validate_store_source_dir(agent, provider, selector, snapshot_root, snapshot_root)
                    .map_err(anyhow::Error::from)?;
            }
            Ok(())
        }
        Agent::Omp => {
            let bytes = capture_omp_database_snapshot_from_paths(source_dir)?
                .ok_or_else(|| anyhow::anyhow!("omp agent.db is missing"))?;
            write_snapshot_bytes(snapshot_root, Path::new("agent/agent.db"), &bytes)?;
            validate_omp_store_content(&bytes, provider, selector)
        }
        Agent::Opencode => {
            let content = read_source_text(&source_dir.join("auth.json"), "OpenCode auth.json")?
                .ok_or_else(|| anyhow::anyhow!("OpenCode auth.json is missing"))?;
            let value = serde_json::from_str::<serde_json::Value>(&content)
                .context("OpenCode auth.json is malformed")?;
            let (key, entry) = select_opencode_auth_entry(&value, provider).map_err(|reason| {
                anyhow::anyhow!("OpenCode auth.json cannot be selected safely: {reason}")
            })?;
            let mut selected = serde_json::Map::new();
            selected.insert(key.to_owned(), entry.clone());
            let selected = serde_json::to_vec(&serde_json::Value::Object(selected))?;
            write_snapshot_bytes(snapshot_root, Path::new("auth.json"), &selected)
        }
        Agent::Codex => capture_unixless_single_file_source(
            source_dir,
            "auth.json",
            "auth.json",
            "Codex auth.json",
            snapshot_root,
        ),
        Agent::Grok => capture_unixless_single_file_source(
            source_dir,
            "auth.json",
            "auth.json",
            "Grok auth.json",
            snapshot_root,
        ),
        Agent::Antigravity => capture_unixless_single_file_source(
            source_dir,
            "settings.json",
            "settings.json",
            "Antigravity settings.json",
            snapshot_root,
        ),
        Agent::Gemini => capture_unixless_single_file_source(
            source_dir,
            "oauth_creds.json",
            "oauth_creds.json",
            "Gemini oauth_creds.json",
            snapshot_root,
        ),
        Agent::Cursor => capture_unixless_single_file_source(
            source_dir,
            "auth.json",
            "auth.json",
            "Cursor auth.json",
            snapshot_root,
        ),
        Agent::Muse => capture_unixless_single_file_source(
            source_dir,
            "auth.json",
            "auth.json",
            "Muse auth.json",
            snapshot_root,
        ),
    }
}

#[cfg(not(unix))]
fn capture_unixless_single_file_source(
    source_dir: &Path,
    source_name: &str,
    snapshot_name: &str,
    label: &str,
    snapshot_root: &Path,
) -> anyhow::Result<()> {
    let bytes = read_source_bytes(&source_dir.join(source_name), label)?
        .ok_or_else(|| anyhow::anyhow!("{label} is missing"))?;
    let text = String::from_utf8(bytes).with_context(|| format!("{label} is not valid UTF-8"))?;
    anyhow::ensure!(!text.trim().is_empty(), "{label} is empty");
    write_snapshot_bytes(snapshot_root, Path::new(snapshot_name), text.as_bytes())
}

fn write_snapshot_bytes(root: &Path, relative: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let path = root.join(relative);
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("snapshot file has no parent"))?;
    std::fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    write_private_bytes(&path, bytes)
}

fn snapshot_content_revision(root: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    let mut budget = SnapshotHashBudget::default();
    hash_snapshot_tree(root, Path::new(""), &mut digest, &mut budget)?;
    Ok(hex::encode(digest.finalize()))
}

#[derive(Default)]
struct SnapshotHashBudget {
    bytes: usize,
    entries: usize,
}

fn hash_snapshot_tree(
    root: &Path,
    relative: &Path,
    digest: &mut impl sha2::Digest,
    budget: &mut SnapshotHashBudget,
) -> anyhow::Result<()> {
    let mut entries = std::fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        budget.entries = budget.entries.saturating_add(1);
        anyhow::ensure!(
            budget.entries <= MAX_AUTH_SOURCE_TREE_ENTRIES,
            "selected auth source has too many entries"
        );
        let name = entry.file_name();
        let child_relative = relative.join(&name);
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let file_type = metadata.file_type();
        digest.update(child_relative.as_os_str().as_encoded_bytes());
        digest.update([0]);
        if file_type.is_symlink() {
            anyhow::bail!("selected auth snapshot contains a symlink");
        }
        if metadata.is_dir() {
            digest.update(*b"d");
            hash_snapshot_tree(&entry.path(), &child_relative, digest, budget)?;
        } else if metadata.is_file() {
            let bytes = read_bounded_local_file(&entry.path())?;
            budget.bytes = budget.bytes.saturating_add(bytes.len());
            anyhow::ensure!(
                budget.bytes <= MAX_AUTH_SOURCE_TREE_BYTES,
                "selected auth source exceeds the size limit"
            );
            digest.update(*b"f");
            digest.update((bytes.len() as u64).to_be_bytes());
            digest.update(bytes);
        } else {
            anyhow::bail!("selected auth snapshot contains a special file");
        }
    }
    Ok(())
}

fn read_bounded_local_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    #[expect(
        clippy::disallowed_methods,
        reason = "bounded auth reads run in joined blocking launch/prewarm workers or scoped provisioning OS threads"
    )]
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take((MAX_AUTH_SOURCE_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= MAX_AUTH_SOURCE_FILE_BYTES,
        "selected auth source file exceeds the size limit: {}",
        path.display()
    );
    Ok(bytes)
}

#[cfg(not(unix))]
fn copy_unixless_source_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let mut budget = SnapshotHashBudget::default();
    copy_unixless_source_tree_inner(source, destination, &mut budget)
}

#[cfg(not(unix))]
fn copy_unixless_source_tree_inner(
    source: &Path,
    destination: &Path,
    budget: &mut SnapshotHashBudget,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(destination)?;
    let mut entries = std::fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        budget.entries = budget.entries.saturating_add(1);
        anyhow::ensure!(
            budget.entries <= MAX_AUTH_SOURCE_TREE_ENTRIES,
            "selected auth source has too many entries"
        );
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!("selected auth source contains a symlink");
        }
        if metadata.is_dir() {
            copy_unixless_source_tree_inner(&source_path, &destination_path, budget)?;
        } else if metadata.is_file() {
            let bytes = read_bounded_local_file(&source_path)?;
            budget.bytes = budget.bytes.saturating_add(bytes.len());
            anyhow::ensure!(
                budget.bytes <= MAX_AUTH_SOURCE_TREE_BYTES,
                "selected auth source exceeds the size limit"
            );
            write_snapshot_bytes(destination, Path::new(&entry.file_name()), &bytes)?;
        } else {
            anyhow::bail!("selected auth source contains a special file");
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_kimi_source_dir_unixless(source_dir: &Path) -> anyhow::Result<()> {
    let config = source_dir.join("config.toml");
    let credentials = source_dir.join("credentials");
    let config_bytes = read_bounded_local_file(&config)?;
    String::from_utf8(config_bytes).context("Kimi config.toml is not valid UTF-8")?;
    let metadata = std::fs::symlink_metadata(credentials)?;
    anyhow::ensure!(metadata.is_dir() && !metadata.file_type().is_symlink());
    Ok(())
}

#[cfg(unix)]
fn validate_locked_sync_source_dir(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    host_home: &Path,
    source: &auth_directory::LockedSource,
) -> Result<(), SyncSourceValidationError> {
    match agent {
        Agent::Claude => {
            if locked_claude_credentials(source, source_dir, host_home)
                .map_err(|error| {
                    SyncSourceValidationError::new(format!("Claude source rejected: {error:#}"))
                })?
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
        Agent::Codex => validate_locked_credential_file(source, "auth.json", "Codex"),
        Agent::Grok => validate_locked_credential_file(source, "auth.json", "Grok"),
        Agent::Opencode => validate_locked_opencode(source, provider),
        Agent::Antigravity => {
            validate_locked_credential_file(source, "settings.json", "Antigravity")
        }
        Agent::Gemini => validate_locked_credential_file(source, "oauth_creds.json", "Gemini"),
        Agent::Cursor => validate_locked_credential_file(source, "auth.json", "Cursor"),
        Agent::Muse => validate_locked_credential_file(source, "auth.json", "Muse"),
        Agent::Omp | Agent::Hermes => validate_locked_store_source_dir(
            agent, provider, selector, source_dir, source, host_home,
        ),
        Agent::Amp => validate_locked_credential_file(source, "secrets.json", "Amp"),
        Agent::Kimi => {
            let config = auth_directory::read_locked_source_file(
                &source.root,
                &["config.toml"],
                "Kimi config.toml",
            )
            .map_err(|error| {
                SyncSourceValidationError::new(format!("Kimi source rejected: {error:#}"))
            })?;
            let credentials = auth_directory::validate_locked_source_directory(
                &source.root,
                &["credentials"],
                "Kimi credentials",
            )
            .map_err(|error| {
                SyncSourceValidationError::new(format!("Kimi source rejected: {error:#}"))
            })?;
            if config.is_some() && credentials {
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

#[cfg(unix)]
fn validate_locked_credential_file(
    source: &auth_directory::LockedSource,
    name: &str,
    agent: &str,
) -> Result<(), SyncSourceValidationError> {
    let content =
        auth_directory::read_locked_source_file(&source.root, &[name], &format!("{agent} {name}"))
            .map_err(|error| {
                SyncSourceValidationError::new(format!("{agent} source rejected: {error:#}"))
            })?;
    match content {
        Some(content) => {
            let text = std::str::from_utf8(&content).map_err(|error| {
                SyncSourceValidationError::new(format!(
                    "{agent} credential {name} is not valid UTF-8: {error}"
                ))
            })?;
            if text.trim().is_empty() {
                Err(SyncSourceValidationError::new(format!(
                    "{agent} credential {name} is empty."
                )))
            } else {
                Ok(())
            }
        }
        None => Err(SyncSourceValidationError::new(format!(
            "Not a {agent} config folder: expected {name} directly inside the source directory."
        ))),
    }
}

#[cfg(unix)]
fn validate_locked_opencode(
    source: &auth_directory::LockedSource,
    provider: Option<AiProvider>,
) -> Result<(), SyncSourceValidationError> {
    let content =
        auth_directory::read_locked_source_file(&source.root, &["auth.json"], "OpenCode auth.json")
            .map_err(|error| {
                SyncSourceValidationError::new(format!("OpenCode source rejected: {error:#}"))
            })?;
    let Some(content) = content else {
        return Err(SyncSourceValidationError::new(
            "Not an OpenCode config folder: expected auth.json directly inside the source directory.",
        ));
    };
    if content.iter().all(u8::is_ascii_whitespace) {
        return Err(SyncSourceValidationError::new(
            "OpenCode credential auth.json is empty.",
        ));
    }
    let value = serde_json::from_slice::<serde_json::Value>(&content).map_err(|_| {
        SyncSourceValidationError::new(
            "OpenCode auth.json is malformed; no credentials were selected.",
        )
    })?;
    select_opencode_auth_entry(&value, provider)
        .map(|_| ())
        .map_err(|reason| {
            SyncSourceValidationError::new(format!(
                "OpenCode auth.json cannot be selected safely: {reason}."
            ))
        })
}

#[cfg(unix)]
fn validate_locked_store_source_dir(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    source: &auth_directory::LockedSource,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    if agent == Agent::Omp {
        let content = capture_omp_database_snapshot(&source.root, source_dir).map_err(|error| {
            SyncSourceValidationError::new(format!("OMP source snapshot failed: {error:#}"))
        })?;
        return validate_omp_store_content(&content, provider, selector).map_err(|error| {
            SyncSourceValidationError::new(format!("OMP source rejected: {error:#}"))
        });
    }
    // The source lock remains held while discovery reads the descriptor. The
    // launch admission path performs the stronger protected-root snapshot and
    // revalidates its bytes before any worker starts.
    validate_store_source_dir(agent, provider, selector, source_dir, host_home)
}

/// Validate a stores-backed source through the same single-entry discovery
/// boundary used for account registration. This keeps a source that changes
/// after scan from silently selecting a different account at launch.
fn validate_store_source_dir(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    if agent == Agent::Hermes {
        validate_hermes_source_shape(source_dir)?;
    }
    let found = jackin_config::discover_account_directory(agent, source_dir, host_home)
        .map_err(|error| {
            SyncSourceValidationError::new(format!("{agent} source rejected: {error}"))
        })?
        .ok_or_else(|| {
            SyncSourceValidationError::new(format!(
                "{agent} source has no usable single-account credential store"
            ))
        })?;
    if provider.is_some_and(|expected| found.provider != Some(expected)) {
        return Err(SyncSourceValidationError::new(format!(
            "{agent} source provider no longer matches the selected account"
        )));
    }
    if selector.is_some_and(|expected| found.source_selector.as_ref() != Some(expected)) {
        return Err(SyncSourceValidationError::new(format!(
            "{agent} source entry/profile no longer matches the selected account"
        )));
    }
    Ok(())
}

fn validate_hermes_source_shape(source_dir: &Path) -> Result<(), SyncSourceValidationError> {
    for name in ["config.yaml", ".env", "auth.json"] {
        let path = source_dir.join(name);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            return Err(SyncSourceValidationError::new(format!(
                "Hermes source file {} is a symlink; refusing to follow it.",
                path.display()
            )));
        }
        if !metadata.is_file() {
            return Err(SyncSourceValidationError::new(format!(
                "Hermes source file {} is a special or non-regular file.",
                path.display()
            )));
        }
    }
    let profiles = source_dir.join("profiles");
    if let Ok(metadata) = std::fs::symlink_metadata(&profiles)
        && (metadata.file_type().is_symlink() || !metadata.is_dir())
    {
        return Err(SyncSourceValidationError::new(format!(
            "Hermes source profiles {} is not a real directory.",
            profiles.display()
        )));
    }
    Ok(())
}

/// Validate the exact `OpenCode` credential that will be staged. A single
/// usable `auth.json` entry is the source-bound materialization. A sibling
/// database may coexist in a normal `OpenCode` data directory, but database-only
/// profiles fail because there is no launchable source to stage.
#[cfg(not(unix))]
fn validate_opencode_source_dir(
    source_dir: &Path,
    provider: Option<AiProvider>,
) -> Result<(), SyncSourceValidationError> {
    let auth_path = source_dir.join("auth.json");
    let content = read_source_text(&auth_path, "OpenCode auth.json")
        .map_err(|_| {
            SyncSourceValidationError::new(format!(
                "Not an OpenCode config folder: expected auth.json directly inside {}.",
                source_dir.display()
            ))
        })?
        .ok_or_else(|| {
            SyncSourceValidationError::new(format!(
                "Not an OpenCode config folder: expected auth.json directly inside {}.",
                source_dir.display()
            ))
        })?;
    if content.trim().is_empty() {
        return Err(SyncSourceValidationError::new(format!(
            "OpenCode credential auth.json in {} is empty.",
            source_dir.display()
        )));
    }
    let value = serde_json::from_str::<serde_json::Value>(&content).map_err(|_| {
        SyncSourceValidationError::new(
            "OpenCode auth.json is malformed; no credentials were selected.",
        )
    })?;
    select_opencode_auth_entry(&value, provider)
        .map(|_| ())
        .map_err(|reason| {
            SyncSourceValidationError::new(format!(
                "OpenCode auth.json cannot be selected safely: {reason}."
            ))
        })
}

/// Return the one provider entry that may cross the role-state boundary.
/// Values remain borrowed so validation does not copy secrets. Multi-entry
/// files are rejected even when one entry could be filtered: the persisted
/// account model has no raw store-key field, so filtering would still permit
/// same-directory identities to collapse during later scans.
fn select_opencode_auth_entry(
    value: &serde_json::Value,
    provider: Option<AiProvider>,
) -> Result<(&str, &serde_json::Value), &'static str> {
    let entries = value
        .as_object()
        .ok_or("the top-level value is not an object")?;
    if entries.len() != 1 {
        return Err("multiple provider entries are unsupported");
    }
    let Some((entry_key, entry)) = entries.iter().next() else {
        return Err("no provider credential exists");
    };
    if let Some(provider) = provider {
        let key = opencode_provider_key(provider)?;
        if entry_key != key {
            return Err("the selected provider credential is missing");
        }
    } else if entry_key != "opencode-go" {
        return Err(
            "source-bound OpenCode profiles currently support only the opencode-go auth entry",
        );
    }
    if !usable_opencode_auth_entry(entry) {
        return Err("the selected provider credential is empty or unsupported");
    }
    Ok((entry_key.as_str(), entry))
}

fn opencode_provider_key(provider: AiProvider) -> Result<&'static str, &'static str> {
    if provider == AiProvider::Opencode {
        Ok("opencode-go")
    } else {
        Err("source-bound OpenCode profiles currently support only the opencode-go auth entry")
    }
}

fn usable_opencode_auth_entry(entry: &serde_json::Value) -> bool {
    let Some(kind) = entry.get("type").and_then(serde_json::Value::as_str) else {
        return false;
    };
    match kind {
        "api" => entry
            .get("key")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|key| !key.trim().is_empty()),
        "oauth" => ["access", "refresh"].into_iter().any(|field| {
            entry
                .get(field)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|token| !token.trim().is_empty())
        }),
        _ => false,
    }
}

#[cfg(not(unix))]
pub(super) fn amp_credentials_dir(source: &Path) -> PathBuf {
    let nested = source.join("data/amp");
    if nested.is_dir() {
        nested
    } else {
        source.to_path_buf()
    }
}

#[cfg(unix)]
fn lock_amp_source_dir(source: &Path) -> anyhow::Result<Option<auth_directory::LockedSource>> {
    match auth_directory::lock_source_dir(&source.join("data/amp"))? {
        Some(source) => Ok(Some(source)),
        None => auth_directory::lock_source_dir(source),
    }
}

/// Require a non-empty credential file named `name` directly inside `dir`.
#[cfg(not(unix))]
fn require_credential_file(
    dir: &Path,
    name: &str,
    agent: &str,
) -> Result<(), SyncSourceValidationError> {
    match read_source_text(&dir.join(name), &format!("{agent} {name}")) {
        Ok(content) if !content.trim().is_empty() => Ok(()),
        Ok(_) => Err(SyncSourceValidationError::new(format!(
            "{agent} credential {name} in {} is empty.",
            dir.display()
        ))),
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            }) =>
        {
            Err(SyncSourceValidationError::new(format!(
                "Not a {agent} config folder: expected {name} directly inside {}.",
                dir.display()
            )))
        }
        Err(error) => Err(SyncSourceValidationError::new(format!(
            "{agent} source rejected: {error:#}"
        ))),
    }
}

impl RoleState {
    /// Provision Codex auth state. Runtime policy is passed as CLI
    /// flags by the entrypoint rather than generated into
    /// `~/.codex/config.toml`.
    ///
    /// `auth.json` semantics mirror Claude's `.credentials.json` for
    /// the file-mount surface; in-container `codex login` writes are
    /// only persisted across container removal when a sync mount
    /// already exists at launch (a host file at `~/.codex/auth.json`).
    ///   * **Sync** + host file present → copy with `0600` perms,
    ///     return `Synced`.
    ///   * **Sync** + host file absent → leave any existing role-state
    ///     `auth.json` untouched (it may survive from a prior synced
    ///     run), return `HostMissing`.
    ///   * **`ApiKey`** → wipe the role-state `auth.json` (the agent
    ///     authenticates via `OPENAI_API_KEY`; a forwarded auth.json
    ///     would let it silently fall back to OAuth credentials the
    ///     operator chose to bypass), return `TokenMode`.
    ///   * **`OAuthToken`** → unreachable in production: parser-rejected
    ///     for Codex. Defensive arm returns `TokenMode` without
    ///     touching role-state files.
    ///   * **Ignore** → delete the role-state `auth.json` if present,
    ///     return `Skipped`.
    ///
    /// Returns `(outcome, mounted_auth_json)` where `mounted_auth_json` is
    /// the role-state `auth.json` path when it should be bind-mounted into
    /// the container (file exists post-call), or `None` when the mount must
    /// be skipped (Ignore wiped it / Sync host-missing with no prior file /
    /// Token mode with no prior file). Centralising the decision here means
    /// `RoleState::prepare` does not need to re-stat the file or reason
    /// about which outcome implies which mount state.
    pub(super) fn provision_codex_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_codex_auth_from_path(auth_json, mode, &host_home.join(".codex/auth.json"))
    }

    pub(super) fn provision_codex_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_codex_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    fn provision_codex_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        // OAuthToken is parser-rejected for Codex (unreachable in production),
        // so no warning is needed. Empty/whitespace auth is not a usable
        // credential and must agree with source-folder validation by staying
        // out of the role-state mount surface.
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Codex auth.json",
            "Codex",
            true,
            false,
            false,
        )
    }
}

impl RoleState {
    /// Provision GitHub CLI auth state for the role-state directory.
    ///
    /// `hosts_yml` is the role-state location of `.config/gh/hosts.yml`
    /// (the directory itself is bind-mounted RW into the container under
    /// `/home/agent/.config/gh`, so writing the file directly into that
    /// directory is enough — no separate file mount).
    ///
    ///   * **Sync** + host token resolved → write `hosts.yml` 0o600,
    ///     return `Synced { token, source }` with `source` naming
    ///     which host path produced the token (`gh` CLI vs file
    ///     fallback).
    ///   * **Sync** + host token absent → leave any existing
    ///     `hosts.yml` untouched (preserves in-container login from a
    ///     prior run), return `HostMissing { reason }` with the typed
    ///     reason (`NoGhAndNoHostsFile` / `GhCliFailed { stderr }` /
    ///     `GhCliEmpty` / `HostsFileMalformed`).
    ///   * **Token** → wipe any prior `hosts.yml` (so a stale
    ///     file-based login can't shadow the env token), return
    ///     `TokenMode { token }` with the operator-resolved value.
    ///   * **Ignore** → wipe any prior `hosts.yml`, return `Skipped`.
    ///
    /// On `Sync`-host-missing the existing in-container login is
    /// preserved deliberately — otherwise an operator who logged out
    /// on the host would lose the container's login on the next
    /// launch.
    pub(super) fn provision_github_auth(
        hosts_yml: &Path,
        github: &GithubAuthContext,
        host_home: &Path,
    ) -> anyhow::Result<GithubProvisionOutcome> {
        // Reject pre-existing symlinks before branching on mode. The
        // role-state dir is bind-mounted RW, so a compromised role could
        // plant a symlink between launches; calling reject_auth_path
        // unconditionally is fine — it lstat's and no-ops on ENOENT.
        reject_auth_path(hosts_yml)?;

        match github.mode {
            GithubAuthMode::Ignore => {
                wipe_file_if_present(hosts_yml)?;
                Ok(GithubProvisionOutcome::Skipped)
            }
            GithubAuthMode::Token => {
                wipe_file_if_present(hosts_yml)?;
                let token = github.token.clone().unwrap_or_default();
                Ok(GithubProvisionOutcome::TokenMode { token })
            }
            GithubAuthMode::Sync => {
                let resolved = if let Some(token) = github
                    .token
                    .as_ref()
                    .filter(|token| !token.trim().is_empty())
                {
                    HostGhResolution::Resolved(HostGhAuth {
                        token: token.clone(),
                        user: None,
                        source: GithubTokenSource::ConfiguredEnv,
                    })
                } else {
                    read_host_gh_token(host_home)?
                };
                match resolved {
                    HostGhResolution::Resolved(resolved) => {
                        let content = render_hosts_yml(&resolved.token, resolved.user.as_deref());
                        // Skip the write when content matches what's already
                        // on disk — avoids touching mtime + atomic-rename on
                        // every launch when nothing changed. Mirrors the
                        // codex provisioner's no-churn guard.
                        let needs_write = !read_bounded_local_file(hosts_yml)
                            .is_ok_and(|existing| existing == content.as_bytes());
                        if needs_write {
                            write_private_file(hosts_yml, &content)?;
                        } else {
                            repair_permissions(hosts_yml)?;
                        }
                        Ok(GithubProvisionOutcome::Synced {
                            token: resolved.token,
                            source: resolved.source,
                        })
                    }
                    HostGhResolution::Missing(reason) => {
                        repair_permissions(hosts_yml)?;
                        Ok(GithubProvisionOutcome::HostMissing { reason })
                    }
                }
            }
        }
    }
}

/// Render a minimal `hosts.yml` body for the `github.com` host. `user`
/// is optional and falls back to a placeholder — gh accepts hosts.yml
/// without it, but writing a value keeps the file shape uniform.
fn render_hosts_yml(token: &str, user: Option<&str>) -> String {
    let user_field = user.filter(|s| !s.trim().is_empty()).unwrap_or("git");
    format!(
        "github.com:\n    oauth_token: {token}\n    git_protocol: https\n    user: {user_field}\n",
    )
}

/// Resolved host-side `gh` auth + which source produced it, so the
/// caller can attribute the value in the launch summary.
struct HostGhAuth {
    token: String,
    user: Option<String>,
    source: GithubTokenSource,
}

/// Result of the host-side resolver. `Missing` carries the typed
/// reason so the launch-summary line can render the actual cause
/// instead of guessing "host logged out".
enum HostGhResolution {
    Resolved(HostGhAuth),
    Missing(HostMissingReason),
}

/// Wipe a file if it exists, ignoring `NotFound` so the call is
/// idempotent without a pre-stat that races with the unlink.
fn wipe_file_if_present(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::remove_file(path)
    }
    #[cfg(not(unix))]
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// True when `host_home` is the operator's real home directory. Gates
/// the host-binary shellouts so hermetic tests with a temp-dir
/// `host_home` cannot leak to the real `gh` binary.
fn host_home_is_real(host_home: &Path) -> bool {
    let real_home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    real_home.as_deref() == Some(host_home)
}

/// Read the host's `gh` token, returning a typed reason when neither
/// source resolves so the launch-summary line can render an accurate
/// cause. Priority order:
///
/// 1. `gh auth token --hostname github.com` — Keychain-aware, only
///    consulted when `host_home` is the real home directory.
/// 2. `~/.config/gh/hosts.yml` parse — works without `gh` on PATH.
fn read_host_gh_token(host_home: &Path) -> anyhow::Result<HostGhResolution> {
    // Read hosts.yml once up front so both the CLI-success path (which
    // reads it for the `user` field) and the file-fallback path share
    // one IO.
    let hosts_path = host_home.join(".config/gh/hosts.yml");
    let hosts_yml = match read_bounded_local_file(&hosts_path) {
        Ok(bytes) => Some(String::from_utf8(bytes).context("GitHub hosts.yml is not valid UTF-8")?),
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            }) =>
        {
            None
        }
        Err(error) => {
            let source = error
                .chain()
                .find_map(|cause| {
                    cause
                        .downcast_ref::<std::io::Error>()
                        .map(|error| std::io::Error::new(error.kind(), error.to_string()))
                })
                .unwrap_or_else(|| std::io::Error::other(error.to_string()));
            return Err(InstanceError::HostConfigRead {
                path: hosts_path,
                source,
            }
            .into());
        }
    };

    let mut cli_failure: Option<HostMissingReason> = None;

    if host_home_is_real(host_home) {
        match crate::process_telemetry::exec_sync(&jackin_process::ExecRequest::new(
            "gh",
            ["auth", "token", "--hostname", "github.com"],
        )) {
            Ok(output) if output.success => {
                let token = String::from_utf8(output.stdout)
                    .context("gh authentication command returned invalid UTF-8")?
                    .trim()
                    .to_owned();
                if !token.is_empty() {
                    let user = hosts_yml
                        .as_deref()
                        .and_then(parse_gh_hosts_yml)
                        .and_then(|parsed| parsed.user);
                    return Ok(HostGhResolution::Resolved(HostGhAuth {
                        token,
                        user,
                        source: GithubTokenSource::GhCli,
                    }));
                }
                cli_failure = Some(HostMissingReason::GhCliEmpty);
            }
            Ok(_) => {
                cli_failure = Some(HostMissingReason::GhCliFailed {
                    stderr: "gh authentication command failed".to_owned(),
                });
            }
            Err(_) => {
                cli_failure = Some(HostMissingReason::GhCliFailed {
                    stderr: "gh authentication command could not start".to_owned(),
                });
            }
        }
    }

    let Some(text) = hosts_yml else {
        return Ok(HostGhResolution::Missing(
            cli_failure.unwrap_or(HostMissingReason::NoGhAndNoHostsFile),
        ));
    };
    if let Some(mut parsed) = parse_gh_hosts_yml(&text) {
        parsed.source = GithubTokenSource::HostsFile;
        return Ok(HostGhResolution::Resolved(parsed));
    }
    // CLI failure (when known) is the more actionable signal than
    // "file malformed" — surface it instead.
    Ok(HostGhResolution::Missing(
        cli_failure.unwrap_or(HostMissingReason::HostsFileMalformed),
    ))
}

/// Parse `gh`'s `hosts.yml`, extracting the `github.com.oauth_token`
/// and (best-effort) `github.com.user` fields via `serde_yaml_ng` so
/// quoting, escapes, comments, and indent rules track the YAML 1.x
/// spec rather than a hand-rolled scanner.
///
/// Returns `None` when the document doesn't carry a `github.com` block
/// with a non-empty `oauth_token` field, or when the document is
/// malformed. Malformed input must NOT yield a partial result —
/// silently accepting half-parsed scalars would land bogus credentials
/// in `hosts.yml` and surface as unrelated 401s mid-session.
fn parse_gh_hosts_yml(text: &str) -> Option<HostGhAuth> {
    #[derive(serde::Deserialize)]
    struct HostsFile {
        // `gh` writes the host header literally as `github.com:`, so
        // the top-level map key is `github.com`.
        #[serde(default, rename = "github.com")]
        github_com: Option<HostEntry>,
    }
    #[derive(serde::Deserialize)]
    struct HostEntry {
        #[serde(default)]
        oauth_token: Option<String>,
        #[serde(default)]
        user: Option<String>,
    }

    let parsed: HostsFile = match serde_yaml_ng::from_str(text) {
        Ok(p) => p,
        Err(_) => return None,
    };
    let entry = parsed.github_com?;
    let token = entry.oauth_token.filter(|s| !s.trim().is_empty())?;
    Some(HostGhAuth {
        token,
        user: entry.user.filter(|s| !s.trim().is_empty()),
        // Caller (`read_host_gh_token` file-fallback path) overwrites
        // this with the right `GithubTokenSource` variant; the field
        // gets a placeholder so the struct literal compiles.
        source: GithubTokenSource::HostsFile,
    })
}

impl RoleState {
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
    pub(super) fn provision_claude_auth(
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

    pub(super) fn provision_claude_auth_from_config_dir(
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
}

impl RoleState {
    /// Provision Amp's host-side `secrets.json` per the chosen mode.
    ///
    /// Source: `~/.local/share/amp/secrets.json` (`XDG_DATA`). The
    /// `XDG_CONFIG` `~/.config/amp/settings.json` is preferences only
    /// and never holds the token.
    ///
    /// `mounted_secrets_json` is `None` when the bind mount must be
    /// skipped. `Sync` with no host file preserves any prior role-state
    /// file so an in-container login isn't silently dropped.
    /// `OAuthToken` is parser-rejected; the defensive arm wipes + logs
    /// so a bypass is loud rather than silent.
    pub(super) fn provision_amp_auth(
        secrets_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_amp_auth_from_path(
            secrets_json,
            mode,
            &host_home.join(".local/share/amp/secrets.json"),
        )
    }

    pub(super) fn provision_amp_auth_from_source_dir(
        secrets_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        #[cfg(unix)]
        if mode == AuthForwardMode::Sync {
            let content = match lock_amp_source_dir(source_dir)? {
                Some(source) => auth_directory::read_locked_source_file(
                    &source.root,
                    &["secrets.json"],
                    "Amp secrets.json",
                )?
                .map(|bytes| {
                    String::from_utf8(bytes).context("Amp secrets.json is not valid UTF-8")
                })
                .transpose()?,
                None => None,
            };
            return provision_single_file_credential_with_content(
                secrets_json,
                mode,
                content,
                "Amp secrets.json",
                "Amp",
                true,
                true,
                true,
            );
        }

        #[cfg(unix)]
        let host_secrets_json = source_dir.join("secrets.json");
        #[cfg(not(unix))]
        let host_secrets_json = amp_credentials_dir(source_dir).join("secrets.json");
        Self::provision_amp_auth_from_path(secrets_json, mode, &host_secrets_json)
    }

    fn provision_amp_auth_from_path(
        secrets_json: &Path,
        mode: AuthForwardMode,
        host_secrets_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            secrets_json,
            host_secrets_json,
            mode,
            "Amp secrets.json",
            "Amp",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Kimi Code's host-side `~/.kimi-code` directory per the chosen mode.
    ///
    /// Sync copies only auth-essential host files into the role-state
    /// directory so it can be bind-mounted into the container. Kimi Code stores
    /// OAuth tokens under `credentials/` (including the `credentials/mcp/`
    /// subtree), `config.toml` carries the OAuth-backed provider/model
    /// references created by login, and `device_id` is the host-bound identity
    /// Kimi sends in OAuth/device headers.
    /// `ApiKey` / `Ignore` wipe any prior role-state directory.
    ///
    ///   * **Sync** + `~/.kimi-code` present → copy `config.toml`, the full
    ///     `credentials/` tree (binary-safe, recursive, symlink-safe), and
    ///     `device_id`. Files land at `0600`, directories at `0700`. Return
    ///     `(Synced, true)`.
    ///   * **Sync** + `~/.kimi-code` absent → return `(HostMissing, true)`.
    ///     Unlike Codex and Amp, no prior role-state files are preserved;
    ///     the role-state dir is still created so the bind-mount exists for
    ///     in-container login state to accumulate.
    ///   * **`ApiKey`** → wipe the role-state directory; return
    ///     `(TokenMode, false)`. Agent authenticates via `KIMI_API_KEY`.
    ///   * **`OAuthToken`** → parser-rejected for Kimi; defensive arm
    ///     wipes role-state and logs loudly, returns `(TokenMode, false)`.
    ///   * **Ignore** → wipe the role-state directory; return
    ///     `(Skipped, false)`.
    ///
    /// Kimi syncs a directory rather than a single file, so the second
    /// return value is `bool` rather than `Option<PathBuf>`. `true` when
    /// `Synced` or `HostMissing` (mount the role-state dir); `false` for
    /// `TokenMode` / `Skipped` (dir was wiped, do not mount).
    pub(super) fn provision_kimi_auth(
        kimi_dir: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        provision_kimi_dir_credential(
            kimi_dir,
            &host_home.join(".kimi-code"),
            mode,
            KIMI_SYNC_FILES,
            "Kimi dir",
            "Kimi",
            false,
        )
    }

    pub(super) fn provision_kimi_auth_from_source_dir(
        kimi_dir: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
        provision_kimi_dir_credential(
            kimi_dir,
            source_dir,
            mode,
            KIMI_SYNC_FILES,
            "Kimi dir",
            "Kimi",
            true,
        )
    }
}

/// Generic directory credential provisioner for agents that sync a directory
/// tree with standard `AuthForwardMode` semantics (OAuthToken/ApiKey/Ignore
/// wipe the dir; Sync copies `sync_files` + a `credentials/` subtree).
///
/// Returns `(outcome, forward_auth)` where `forward_auth` is `true` when
/// the role-state directory should be bind-mounted into the container
/// (`Synced` or `HostMissing`), and `false` when it was wiped.
fn provision_kimi_dir_credential(
    target_dir: &Path,
    host_dir: &Path,
    mode: AuthForwardMode,
    sync_files: &[&str],
    _label: &str,
    agent_name: &str,
    validate_source: bool,
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    let outcome = match mode {
        AuthForwardMode::OAuthToken => {
            eprintln!(
                "[jackin] internal: {agent_name} provision received unsupported \
                 OAuthToken mode — parser invariant bypassed; \
                 wiping role state and falling back to token-mode."
            );
            wipe_kimi_state(target_dir)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::ApiKey => {
            wipe_kimi_state(target_dir)?;
            AuthProvisionOutcome::TokenMode
        }
        AuthForwardMode::Ignore => {
            wipe_kimi_state(target_dir)?;
            AuthProvisionOutcome::Skipped
        }
        AuthForwardMode::Sync => {
            #[cfg(unix)]
            {
                let source = auth_directory::lock_source_dir(host_dir)?;
                if validate_source && let Some(source) = source.as_ref() {
                    validate_kimi_locked_source(source, host_dir)?;
                }
                auth_directory::stage_auth_directory_with_locked_source(
                    target_dir,
                    host_dir,
                    source,
                    |host_dir, source, staged| {
                        for name in sync_files {
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
                            "credentials",
                            staged,
                            "credentials",
                            &format!("copying {}", host_dir.join("credentials").display()),
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
                        for name in sync_files {
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
                            "credentials",
                            staged,
                            "credentials",
                            &format!("copying {}", host_dir.join("credentials").display()),
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

#[cfg(unix)]
fn validate_kimi_locked_source(
    source: &auth_directory::LockedSource,
    host_dir: &Path,
) -> anyhow::Result<()> {
    let config = auth_directory::read_locked_source_file(
        &source.root,
        &["config.toml"],
        "Kimi config.toml",
    )?;
    let credentials = auth_directory::validate_locked_source_directory(
        &source.root,
        &["credentials"],
        "Kimi credentials",
    )?;
    anyhow::ensure!(
        config.is_some() && credentials,
        "Kimi source {} must contain config.toml and a credentials/ directory",
        host_dir.display()
    );
    Ok(())
}

/// Single-file host artifacts forwarded into the role-state directory under
/// `Sync` mode. Listed once so a future addition (or removal) only has to be
/// made here, not threaded through three near-identical copy blocks.
const KIMI_SYNC_FILES: &[&str] = &["config.toml", "device_id"];

/// Descriptor-relative auth directory transactions.
///
/// The destination parent and source root are opened once, every component is
/// traversed with `O_NOFOLLOW`, and all mutations use the pinned descriptors.
/// The journal is written and synced before each rename boundary so a retry can
/// complete or roll back an interrupted swap without retaining an old tree.
#[cfg(unix)]
mod auth_directory {
    use super::{
        AUTH_DIRECTORY_SWAP_COUNTER, AuthProvisionOutcome, MAX_AUTH_SOURCE_FILE_BYTES,
        MAX_AUTH_SOURCE_TREE_BYTES, MAX_AUTH_SOURCE_TREE_ENTRIES,
    };
    use anyhow::Context;
    use fs4::{FileExt, TryLockError};
    use nix::dir::Dir;
    use nix::errno::Errno;
    use nix::fcntl::{AtFlags, OFlag, open, openat, renameat};
    use nix::sys::stat::{FileStat, Mode, SFlag, fchmod, fstat, fstatat, mkdirat, mode_t};
    use nix::unistd::{UnlinkatFlags, fsync, geteuid, unlinkat};
    use serde::{Deserialize, Serialize};
    use std::ffi::{CStr, CString};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::OwnedFd;
    use std::os::unix::ffi::OsStrExt;
    // `/private` alias expansion below is macOS-only; keep these imports gated
    // so Linux builds do not trip unused-import denial.
    #[cfg(target_os = "macos")]
    use std::ffi::OsString;
    #[cfg(target_os = "macos")]
    use std::os::unix::ffi::OsStringExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    const JOURNAL_SCHEMA_VERSION: u32 = 1;
    const MAX_JOURNAL_BYTES: usize = 16 * 1024;
    const SOURCE_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
    const SOURCE_LOCK_POLL: Duration = Duration::from_millis(10);

    #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
    enum SwapPhase {
        Prepared,
        BackedUp,
        Installed,
    }

    #[derive(Clone, Debug, Deserialize, Serialize)]
    struct SwapJournal {
        schema_version: u32,
        target: String,
        stage: String,
        previous: Option<String>,
        phase: SwapPhase,
    }

    #[derive(Debug)]
    struct TargetLock {
        parent: File,
        target: CString,
        key: String,
        journal: CString,
        _lock: Arc<File>,
    }

    #[derive(Debug)]
    pub struct AuthMountLease {
        _lock: Arc<File>,
    }

    #[derive(Debug)]
    pub(crate) struct LockedSource {
        pub(crate) root: File,
    }

    pub(crate) struct SnapshotDirectory {
        path: PathBuf,
        parent: File,
        name: CString,
    }

    impl SnapshotDirectory {
        pub(crate) fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for SnapshotDirectory {
        fn drop(&mut self) {
            drop(remove_tree(
                &self.parent,
                self.name.as_c_str(),
                "auth source snapshot",
            ));
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum FailurePoint {
        JournalRewrite,
        Prepared,
        Backup,
        Installed,
    }

    #[cfg(test)]
    thread_local! {
        static FAILURE_POINT: std::cell::Cell<Option<FailurePoint>> = const { std::cell::Cell::new(None) };
    }

    #[cfg(test)]
    pub(crate) struct FailureGuard;

    #[cfg(test)]
    impl Drop for FailureGuard {
        fn drop(&mut self) {
            FAILURE_POINT.with(|point| point.set(None));
        }
    }

    #[cfg(test)]
    pub(crate) fn inject_failure(point: FailurePoint) -> FailureGuard {
        FAILURE_POINT.with(|failure| failure.set(Some(point)));
        FailureGuard
    }

    #[cfg(test)]
    thread_local! {
        static HERMES_SNAPSHOT_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
            const { std::cell::RefCell::new(None) };
        static OMP_AFTER_DATABASE_READ_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
            const { std::cell::RefCell::new(None) };
        static SOURCE_OPEN_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
            const { std::cell::RefCell::new(None) };
    }

    #[cfg(test)]
    pub(crate) fn set_hermes_snapshot_hook(hook: Box<dyn FnOnce()>) {
        HERMES_SNAPSHOT_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
    }

    #[cfg(test)]
    pub(crate) fn set_omp_after_database_read_hook(hook: Box<dyn FnOnce()>) {
        OMP_AFTER_DATABASE_READ_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
    }

    #[cfg(test)]
    pub(crate) fn set_source_open_hook(hook: Box<dyn FnOnce()>) {
        SOURCE_OPEN_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
    }

    pub(crate) fn run_hermes_snapshot_hook() {
        #[cfg(test)]
        if let Some(hook) = HERMES_SNAPSHOT_HOOK.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }

    pub(crate) fn run_omp_after_database_read_hook() {
        #[cfg(test)]
        if let Some(hook) = OMP_AFTER_DATABASE_READ_HOOK.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }

    fn run_source_open_hook() {
        #[cfg(test)]
        if let Some(hook) = SOURCE_OPEN_HOOK.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }

    fn maybe_fail(point: FailurePoint) -> anyhow::Result<()> {
        #[cfg(test)]
        if FAILURE_POINT.with(|failure| failure.get() == Some(point)) {
            anyhow::bail!("injected auth directory crash at {point:?}");
        }
        #[cfg(not(test))]
        let _ = point;
        Ok(())
    }

    fn owned_fd(fd: OwnedFd) -> File {
        fd.into()
    }

    fn nix_error(error: Errno, action: &str) -> anyhow::Error {
        if error == Errno::ELOOP {
            return anyhow::anyhow!("{action}: symlink traversal rejected");
        }
        if error == Errno::ENOTDIR {
            return anyhow::anyhow!("{action}: non-directory or symlink traversal rejected");
        }
        anyhow::Error::new(error).context(action.to_owned())
    }

    /// Normalize only lexical aliases. Accepted paths become absolute so the
    /// lock identity is shared by relative and absolute spellings; symlinks
    /// are deliberately not resolved here and are rejected by descriptor
    /// traversal instead.
    fn normalize_path(path: &Path) -> anyhow::Result<PathBuf> {
        let mut normalized = if path.is_absolute() {
            PathBuf::new()
        } else {
            std::env::current_dir().context("finding auth path base directory")?
        };
        for component in path.components() {
            match component {
                std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
                std::path::Component::RootDir => normalized.push(Path::new("/")),
                std::path::Component::CurDir => {}
                std::path::Component::Normal(component) => normalized.push(component),
                std::path::Component::ParentDir => {
                    anyhow::bail!("auth path contains parent traversal: {}", path.display())
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            let bytes = normalized.as_os_str().as_bytes();
            for alias in [b"/var".as_slice(), b"/tmp".as_slice(), b"/etc".as_slice()] {
                if bytes == alias
                    || bytes
                        .strip_prefix(alias)
                        .is_some_and(|rest| rest.starts_with(b"/"))
                {
                    let mut normalized = b"/private".to_vec();
                    normalized.extend_from_slice(bytes);
                    return Ok(PathBuf::from(OsString::from_vec(normalized)));
                }
            }
        }
        Ok(normalized)
    }

    fn path_key(path: &Path) -> anyhow::Result<String> {
        use sha2::{Digest, Sha256};
        let normalized = normalize_path(path)?;
        let mut digest = Sha256::new();
        digest.update(normalized.as_os_str().as_bytes());
        Ok(hex::encode(digest.finalize()))
    }

    fn cstring_name(path: &Path) -> anyhow::Result<CString> {
        let name = path.file_name().ok_or_else(|| {
            anyhow::anyhow!("auth path has no final component: {}", path.display())
        })?;
        CString::new(name.as_bytes()).context("auth path contains NUL")
    }

    fn component_cstring(component: &std::path::Component<'_>) -> anyhow::Result<CString> {
        CString::new(component.as_os_str().as_bytes()).context("auth path contains NUL")
    }

    fn open_start(absolute: bool) -> anyhow::Result<File> {
        let path = if absolute {
            Path::new("/")
        } else {
            Path::new(".")
        };
        let fd = open(
            path,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| nix_error(error, "opening auth traversal root"))?;
        let file = owned_fd(fd);
        validate_directory(&file, "auth traversal root", false)?;
        Ok(file)
    }

    fn validate_directory(file: &File, label: &str, exact_private: bool) -> anyhow::Result<()> {
        let stat = fstat(file).map_err(|error| nix_error(error, label))?;
        anyhow::ensure!(
            SFlag::from_bits_truncate(stat.st_mode).contains(SFlag::S_IFDIR),
            "{label} is not a directory"
        );
        let mode = stat.st_mode & 0o7777;
        if exact_private {
            anyhow::ensure!(
                stat.st_uid == geteuid().as_raw(),
                "{label} is not owned by the current user"
            );
            anyhow::ensure!(
                mode & 0o777 == 0o700,
                "{label} is not mode 0700 (mode {mode:o})"
            );
        } else {
            anyhow::ensure!(
                mode & 0o022 == 0 || mode & 0o1000 != 0,
                "{label} is writable by an untrusted group or other user"
            );
        }
        Ok(())
    }

    fn validate_owned_stat(stat: &FileStat, label: &str, expected: SFlag) -> anyhow::Result<()> {
        let actual = SFlag::from_bits_truncate(stat.st_mode);
        anyhow::ensure!(
            actual.contains(expected),
            "{} has an unexpected {}",
            label,
            if actual.contains(SFlag::S_IFLNK) {
                "symlink"
            } else if actual
                .intersects(SFlag::S_IFIFO | SFlag::S_IFCHR | SFlag::S_IFBLK | SFlag::S_IFSOCK,)
            {
                "special file"
            } else {
                "file type"
            }
        );
        anyhow::ensure!(
            stat.st_uid == geteuid().as_raw(),
            "{label} is not owned by the current user"
        );
        anyhow::ensure!(
            stat.st_mode & 0o022 == 0,
            "{label} is writable by an untrusted group or other user"
        );
        Ok(())
    }

    fn open_parent(path: &Path, create: bool) -> anyhow::Result<(File, CString, PathBuf)> {
        anyhow::ensure!(
            matches!(
                path.components().next_back(),
                Some(std::path::Component::Normal(_))
            ),
            "auth path must have a normal final component: {}",
            path.display()
        );
        let path = normalize_path(path)?;
        let target = cstring_name(&path)?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut directory = open_start(path.is_absolute())?;
        for component in parent.components() {
            let std::path::Component::Normal(_) = component else {
                if matches!(component, std::path::Component::RootDir) {
                    continue;
                }
                if matches!(component, std::path::Component::CurDir) {
                    continue;
                }
                anyhow::bail!("auth path contains parent traversal: {}", path.display());
            };
            let component = component_cstring(&component)?;
            let next = match openat(
                &directory,
                component.as_c_str(),
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => owned_fd(fd),
                Err(Errno::ENOENT) if create => {
                    mkdirat(
                        &directory,
                        component.as_c_str(),
                        Mode::from_bits_truncate(0o700),
                    )
                    .or_else(ignore_eexist)
                    .map_err(|error| nix_error(error, "creating auth directory parent"))?;
                    let fd = openat(
                        &directory,
                        component.as_c_str(),
                        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|error| nix_error(error, "opening created auth directory parent"))?;
                    owned_fd(fd)
                }
                Err(error) => return Err(nix_error(error, "opening auth directory parent")),
            };
            validate_directory(&next, "auth directory parent", false)?;
            directory = next;
        }
        Ok((directory, target, path))
    }

    fn entry_stat(directory: &File, name: &CStr) -> anyhow::Result<Option<FileStat>> {
        match fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(stat) => Ok(Some(stat)),
            Err(Errno::ENOENT) => Ok(None),
            Err(error) => Err(nix_error(error, "lstat auth directory entry")),
        }
    }

    fn open_private_file(
        directory: &File,
        name: &CStr,
        flags: OFlag,
        mode: Mode,
        label: &str,
    ) -> anyhow::Result<File> {
        let fd = openat(
            directory,
            name,
            flags | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            mode,
        )
        .map_err(|error| nix_error(error, label))?;
        let file = owned_fd(fd);
        let stat = fstat(&file).map_err(|error| nix_error(error, label))?;
        validate_owned_stat(&stat, label, SFlag::S_IFREG)?;
        Ok(file)
    }

    fn ensure_same_source_identity(
        expected: &FileStat,
        actual: &FileStat,
        label: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            expected.st_dev == actual.st_dev
                && expected.st_ino == actual.st_ino
                && expected.st_mode & SFlag::S_IFMT.bits() == actual.st_mode & SFlag::S_IFMT.bits(),
            "{label} was replaced during secure open"
        );
        Ok(())
    }

    fn open_source_file_with_hook(
        directory: &File,
        name: &CStr,
        expected: &FileStat,
        label: &str,
        invoke_hook: bool,
    ) -> anyhow::Result<File> {
        if invoke_hook {
            run_source_open_hook();
        }
        let fd = openat(
            directory,
            name,
            OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| nix_error(error, label))?;
        let file = owned_fd(fd);
        let actual = fstat(&file).map_err(|error| nix_error(error, label))?;
        validate_owned_stat(&actual, label, SFlag::S_IFREG)?;
        ensure_same_source_identity(expected, &actual, label)?;
        Ok(file)
    }

    fn open_source_file(
        directory: &File,
        name: &CStr,
        expected: &FileStat,
        label: &str,
    ) -> anyhow::Result<File> {
        open_source_file_with_hook(directory, name, expected, label, true)
    }

    fn open_directory_at(directory: &File, name: &CStr, label: &str) -> anyhow::Result<File> {
        let fd = openat(
            directory,
            name,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| nix_error(error, label))?;
        let file = owned_fd(fd);
        validate_directory(&file, label, false)?;
        let stat = fstat(&file).map_err(|error| nix_error(error, label))?;
        anyhow::ensure!(
            stat.st_uid == geteuid().as_raw(),
            "{label} is not owned by the current user"
        );
        Ok(file)
    }

    fn open_source_directory_at_with_hook(
        directory: &File,
        name: &CStr,
        expected: &FileStat,
        label: &str,
        invoke_hook: bool,
    ) -> anyhow::Result<File> {
        if invoke_hook {
            run_source_open_hook();
        }
        let fd = openat(
            directory,
            name,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| nix_error(error, label))?;
        let file = owned_fd(fd);
        validate_directory(&file, label, false)?;
        let actual = fstat(&file).map_err(|error| nix_error(error, label))?;
        anyhow::ensure!(
            actual.st_uid == geteuid().as_raw(),
            "{label} is not owned by the current user"
        );
        ensure_same_source_identity(expected, &actual, label)?;
        Ok(file)
    }

    fn open_source_directory_at(
        directory: &File,
        name: &CStr,
        expected: &FileStat,
        label: &str,
    ) -> anyhow::Result<File> {
        open_source_directory_at_with_hook(directory, name, expected, label, true)
    }

    fn fsync_directory(directory: &File) -> anyhow::Result<()> {
        fsync(directory).map_err(|error| nix_error(error, "syncing auth directory"))
    }

    fn ignore_eexist(error: Errno) -> Result<(), Errno> {
        if error == Errno::EEXIST {
            Ok(())
        } else {
            Err(error)
        }
    }

    fn source_entry_kind(
        source: &File,
        name: &CStr,
        label: &str,
    ) -> anyhow::Result<Option<FileStat>> {
        let Some(stat) = entry_stat(source, name)? else {
            return Ok(None);
        };
        let kind = SFlag::from_bits_truncate(stat.st_mode);
        if kind.contains(SFlag::S_IFLNK) {
            anyhow::bail!("{label} is a symlink; refusing to follow source auth state");
        }
        Ok(Some(stat))
    }

    fn read_source_file(
        source: &File,
        name: &CStr,
        stat: &FileStat,
        label: &str,
    ) -> anyhow::Result<Vec<u8>> {
        validate_owned_stat(stat, label, SFlag::S_IFREG)?;
        let file = open_source_file(source, name, stat, label)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut &file)
            .take((MAX_AUTH_SOURCE_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .with_context(|| format!("reading {label}"))?;
        anyhow::ensure!(
            bytes.len() <= MAX_AUTH_SOURCE_FILE_BYTES,
            "{label} exceeds the credential source size limit"
        );
        Ok(bytes)
    }

    pub(crate) fn read_locked_source_file(
        source: &File,
        components: &[&str],
        label: &str,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let Some((file_name_text, directory_names)) = components.split_last() else {
            anyhow::bail!("{label} has no source path components");
        };
        let mut directory = source.try_clone()?;
        for directory_name_text in directory_names {
            let directory_name = source_name(directory_name_text)?;
            let Some(stat) = source_entry_kind(&directory, &directory_name, label)? else {
                return Ok(None);
            };
            validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
            directory = open_source_directory_at_with_hook(
                &directory,
                &directory_name,
                &stat,
                label,
                false,
            )?;
        }
        let file_name = source_name(file_name_text)?;
        let Some(stat) = source_entry_kind(&directory, &file_name, label)? else {
            return Ok(None);
        };
        validate_owned_stat(&stat, label, SFlag::S_IFREG)?;
        let file = open_source_file_with_hook(&directory, &file_name, &stat, label, true)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut &file)
            .take((MAX_AUTH_SOURCE_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .with_context(|| format!("reading {label}"))?;
        anyhow::ensure!(
            bytes.len() <= MAX_AUTH_SOURCE_FILE_BYTES,
            "{label} exceeds the credential source size limit"
        );
        Ok(Some(bytes))
    }

    pub(crate) fn read_locked_source_file_bounded(
        source: &File,
        components: &[&str],
        label: &str,
        max_bytes: usize,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let Some((file_name_text, directory_names)) = components.split_last() else {
            anyhow::bail!("{label} has no source path components");
        };
        let mut directory = source.try_clone()?;
        for directory_name_text in directory_names {
            let directory_name = source_name(directory_name_text)?;
            let Some(stat) = source_entry_kind(&directory, &directory_name, label)? else {
                return Ok(None);
            };
            validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
            directory = open_source_directory_at_with_hook(
                &directory,
                &directory_name,
                &stat,
                label,
                false,
            )?;
        }
        let file_name = source_name(file_name_text)?;
        let Some(stat) = source_entry_kind(&directory, &file_name, label)? else {
            return Ok(None);
        };
        validate_owned_stat(&stat, label, SFlag::S_IFREG)?;
        let file = open_source_file_with_hook(&directory, &file_name, &stat, label, true)?;
        let read_limit = u64::try_from(max_bytes)
            .ok()
            .and_then(|limit| limit.checked_add(1))
            .ok_or_else(|| anyhow::anyhow!("{label} size limit overflows"))?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut &file)
            .take(read_limit)
            .read_to_end(&mut bytes)
            .with_context(|| format!("reading {label}"))?;
        anyhow::ensure!(
            bytes.len() <= max_bytes,
            "{label} exceeds its {max_bytes}-byte source size limit"
        );
        Ok(Some(bytes))
    }

    pub(crate) fn validate_locked_source_directory(
        source: &File,
        components: &[&str],
        label: &str,
    ) -> anyhow::Result<bool> {
        let mut directory = source.try_clone()?;
        for component_text in components {
            let component = source_name(component_text)?;
            let Some(stat) = source_entry_kind(&directory, &component, label)? else {
                return Ok(false);
            };
            validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
            directory =
                open_source_directory_at_with_hook(&directory, &component, &stat, label, false)?;
        }
        Ok(true)
    }

    pub(crate) fn read_source_path(path: &Path, label: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("source path has no parent: {}", path.display()))?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                anyhow::anyhow!("source path has no valid file name: {}", path.display())
            })?;
        let Some(source) = lock_source_dir(parent)? else {
            return Ok(None);
        };
        read_locked_source_file(&source.root, &[name], label)
    }

    fn write_private_file_at(
        directory: &File,
        name: &CStr,
        bytes: &[u8],
        label: &str,
    ) -> anyhow::Result<()> {
        let file = open_private_file(
            directory,
            name,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL,
            Mode::from_bits_truncate(0o600),
            label,
        )?;
        fchmod(&file, Mode::from_bits_truncate(0o600))
            .map_err(|error| nix_error(error, "restricting staged auth file"))?;
        let mut file = file;
        file.write_all(bytes)
            .with_context(|| format!("writing {label}"))?;
        file.sync_all()
            .with_context(|| format!("syncing {label}"))?;
        Ok(())
    }

    fn source_name(name: &str) -> anyhow::Result<CString> {
        CString::new(name).context("auth source name contains NUL")
    }

    pub(crate) fn copy_optional_source_file(
        source: &File,
        source_name_text: &str,
        destination: &File,
        destination_name_text: &str,
        label: &str,
    ) -> anyhow::Result<()> {
        let source_entry_name = source_name(source_name_text)?;
        let Some(stat) = source_entry_kind(source, &source_entry_name, label)? else {
            return Ok(());
        };
        validate_owned_stat(&stat, label, SFlag::S_IFREG)?;
        let bytes = read_source_file(source, &source_entry_name, &stat, label)?;
        let destination_name = source_name(destination_name_text)?;
        write_private_file_at(destination, &destination_name, &bytes, label)
    }

    struct CopyBudget {
        bytes: usize,
        entries: usize,
    }

    fn copy_tree(source: &File, destination: &File, label: &str) -> anyhow::Result<()> {
        let mut budget = CopyBudget {
            bytes: 0,
            entries: 0,
        };
        copy_tree_with_budget(source, destination, label, &mut budget)
    }

    fn copy_tree_with_budget(
        source: &File,
        destination: &File,
        label: &str,
        budget: &mut CopyBudget,
    ) -> anyhow::Result<()> {
        validate_directory(source, label, false)?;
        fchmod(destination, Mode::from_bits_truncate(0o700))
            .map_err(|error| nix_error(error, "restricting staged auth directory"))?;
        validate_directory(destination, "staged auth directory", true)?;
        let source_clone = source.try_clone()?;
        let mut entries = Dir::from_fd(source_clone.into())
            .map_err(|error| nix_error(error, "opening source auth directory entries"))?;
        let mut names = Vec::new();
        for entry in entries.iter() {
            let entry = entry.map_err(|error| nix_error(error, "reading source auth directory"))?;
            let name = entry.file_name();
            if name.to_bytes() != b"." && name.to_bytes() != b".." {
                names.push(name.to_owned());
            }
        }

        for name in names {
            budget.entries = budget.entries.saturating_add(1);
            anyhow::ensure!(
                budget.entries <= MAX_AUTH_SOURCE_TREE_ENTRIES,
                "{label} contains too many entries"
            );
            let entry_label = format!("{label}/{}", name.to_string_lossy());
            let stat = entry_stat(source, &name)?.ok_or_else(|| {
                anyhow::anyhow!("{entry_label} disappeared during secure source snapshot")
            })?;
            let kind = SFlag::from_bits_truncate(stat.st_mode);
            if kind.contains(SFlag::S_IFLNK) {
                anyhow::bail!("{entry_label} is a symlink; refusing to sync source auth state");
            }
            if kind.contains(SFlag::S_IFDIR) {
                validate_owned_stat(&stat, &entry_label, SFlag::S_IFDIR)?;
                mkdirat(
                    destination,
                    name.as_c_str(),
                    Mode::from_bits_truncate(0o700),
                )
                .or_else(ignore_eexist)
                .map_err(|error| nix_error(error, "creating staged auth subdirectory"))?;
                let child = open_directory_at(destination, &name, &entry_label)?;
                validate_directory(&child, &entry_label, true)?;
                let source_child = open_source_directory_at(source, &name, &stat, &entry_label)?;
                copy_tree_with_budget(&source_child, &child, &entry_label, budget)?;
                fsync_directory(&child)?;
            } else if kind.contains(SFlag::S_IFREG) {
                validate_owned_stat(&stat, &entry_label, SFlag::S_IFREG)?;
                let bytes = read_source_file(source, &name, &stat, &entry_label)?;
                budget.bytes = budget.bytes.saturating_add(bytes.len());
                anyhow::ensure!(
                    budget.bytes <= MAX_AUTH_SOURCE_TREE_BYTES,
                    "{label} exceeds the credential source size limit"
                );
                write_private_file_at(destination, &name, &bytes, &entry_label)?;
            } else {
                anyhow::bail!("{entry_label} is a special file; refusing to sync it");
            }
        }
        fsync_directory(destination)
    }

    pub(crate) fn copy_optional_source_tree(
        source: &File,
        source_name_text: &str,
        destination: &File,
        destination_name_text: &str,
        label: &str,
    ) -> anyhow::Result<()> {
        let source_entry_name = source_name(source_name_text)?;
        let Some(stat) = source_entry_kind(source, &source_entry_name, label)? else {
            return Ok(());
        };
        validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
        mkdirat(
            destination,
            source_entry_name.as_c_str(),
            Mode::from_bits_truncate(0o700),
        )
        .map_err(|error| nix_error(error, "creating staged auth tree"))?;
        let destination_name = source_name(destination_name_text)?;
        if destination_name != source_entry_name {
            renameat(
                destination,
                source_entry_name.as_c_str(),
                destination,
                destination_name.as_c_str(),
            )
            .map_err(|error| nix_error(error, "naming staged auth tree"))?;
        }
        let staged = open_directory_at(destination, &destination_name, label)?;
        let source_dir = open_source_directory_at(source, &source_entry_name, &stat, label)?;
        copy_tree(&source_dir, &staged, label)
    }

    pub(crate) fn open_directory_path(path: &Path) -> anyhow::Result<File> {
        let (parent, name, normalized) = open_parent(path, false)?;
        open_directory_at(
            &parent,
            name.as_c_str(),
            &format!("opening auth directory {}", normalized.display()),
        )
    }

    /// Allocate a source snapshot beneath a host-private parent. Missing
    /// parent components are created descriptor-relatively with `mkdirat`;
    /// parent and child creation both use no-follow checks. The returned
    /// directory guard owns descriptor-relative cleanup of the unique child
    /// path after workers release their snapshot clones.
    pub(crate) fn create_snapshot_directory(
        parent_path: &Path,
    ) -> anyhow::Result<SnapshotDirectory> {
        let (parent, target, normalized) = open_parent(parent_path, true)?;
        let parent_directory = match open_directory_at(
            &parent,
            target.as_c_str(),
            &format!("opening auth snapshot parent {}", normalized.display()),
        ) {
            Ok(directory) => directory,
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
            {
                mkdirat(&parent, target.as_c_str(), Mode::from_bits_truncate(0o700))
                    .or_else(ignore_eexist)
                    .map_err(|error| nix_error(error, "creating auth snapshot parent"))?;
                open_directory_at(
                    &parent,
                    target.as_c_str(),
                    &format!("opening auth snapshot parent {}", normalized.display()),
                )?
            }
            Err(error) => return Err(error),
        };
        validate_directory(&parent_directory, "auth snapshot parent", true)?;
        for _ in 0..128 {
            let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let name_text = format!(".jackin-auth-source-{}-{sequence}", std::process::id());
            let name = CString::new(name_text.as_str())?;
            match mkdirat(
                &parent_directory,
                name.as_c_str(),
                Mode::from_bits_truncate(0o700),
            ) {
                Ok(()) => {
                    let directory =
                        open_directory_at(&parent_directory, &name, "auth source snapshot")?;
                    validate_directory(&directory, "auth source snapshot", true)?;
                    drop(directory);
                    return Ok(SnapshotDirectory {
                        path: normalized.join(name_text),
                        parent: parent_directory,
                        name,
                    });
                }
                Err(Errno::EEXIST) => {}
                Err(error) => return Err(nix_error(error, "creating auth source snapshot")),
            }
        }
        anyhow::bail!("could not allocate a unique auth source snapshot")
    }

    fn lock_source_file(root: &File, timeout: Duration) -> anyhow::Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            match FileExt::try_lock(root) {
                Ok(()) => return Ok(()),
                Err(TryLockError::WouldBlock) => {
                    let now = Instant::now();
                    if now >= deadline {
                        anyhow::bail!(
                            "timed out waiting {timeout:?} for the source auth directory lock"
                        );
                    }
                    #[expect(
                        clippy::disallowed_methods,
                        reason = "source lock polling runs in blocking launch/prewarm/console validation workers or provisioning OS threads"
                    )]
                    std::thread::sleep(
                        SOURCE_LOCK_POLL.min(deadline.saturating_duration_since(now)),
                    );
                }
                Err(TryLockError::Error(error)) => {
                    return Err(error).context("locking source auth directory");
                }
            }
        }
    }

    fn lock_source_dir_with_timeout(
        path: &Path,
        timeout: Duration,
    ) -> anyhow::Result<Option<LockedSource>> {
        let (parent, name, normalized) = match open_parent(path, false) {
            Ok(value) => value,
            Err(error) if error.downcast_ref::<Errno>() == Some(&Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(error),
        };
        let Some(expected) = entry_stat(&parent, &name)? else {
            return Ok(None);
        };
        validate_owned_stat(
            &expected,
            &format!("source auth directory {}", normalized.display()),
            SFlag::S_IFDIR,
        )?;
        let fd = match openat(
            &parent,
            name.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => {
                return Err(nix_error(
                    error,
                    &format!("opening source auth directory {}", normalized.display()),
                ));
            }
        };
        let root = owned_fd(fd);
        validate_directory(&root, "source auth directory", false)?;
        let stat =
            fstat(&root).map_err(|error| nix_error(error, "statting source auth directory"))?;
        anyhow::ensure!(
            stat.st_uid == geteuid().as_raw(),
            "source auth directory is not owned by the current user"
        );
        ensure_same_source_identity(
            &expected,
            &stat,
            &format!("source auth directory {}", normalized.display()),
        )?;
        lock_source_file(&root, timeout)?;
        Ok(Some(LockedSource { root }))
    }

    pub(crate) fn lock_source_dir(path: &Path) -> anyhow::Result<Option<LockedSource>> {
        lock_source_dir_with_timeout(path, SOURCE_LOCK_TIMEOUT)
    }

    #[cfg(test)]
    pub(crate) fn lock_source_dir_for_test(
        path: &Path,
        timeout: Duration,
    ) -> anyhow::Result<Option<LockedSource>> {
        lock_source_dir_with_timeout(path, timeout)
    }

    fn new_stage(parent: &File, key: &str) -> anyhow::Result<(CString, File)> {
        for _ in 0..128 {
            let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = CString::new(format!(
                ".jackin-auth-stage-{key}-{}-{sequence}",
                std::process::id()
            ))?;
            match mkdirat(parent, name.as_c_str(), Mode::from_bits_truncate(0o700)) {
                Ok(()) => {
                    let directory = open_directory_at(parent, &name, "new auth stage")?;
                    validate_directory(&directory, "new auth stage", true)?;
                    return Ok((name, directory));
                }
                Err(Errno::EEXIST) => {}
                Err(error) => return Err(nix_error(error, "creating auth stage")),
            }
        }
        anyhow::bail!("could not allocate a unique auth stage")
    }

    fn new_previous(parent: &File, key: &str) -> anyhow::Result<CString> {
        for _ in 0..128 {
            let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = CString::new(format!(
                ".jackin-auth-previous-{key}-{}-{sequence}",
                std::process::id()
            ))?;
            if entry_stat(parent, &name)?.is_none() {
                return Ok(name);
            }
        }
        anyhow::bail!("could not allocate a unique auth previous directory")
    }

    fn new_journal_temporary(parent: &File, key: &str) -> anyhow::Result<CString> {
        for _ in 0..128 {
            let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = CString::new(format!(
                ".jackin-auth-journal-{key}-tmp-{}-{sequence}",
                std::process::id()
            ))?;
            if entry_stat(parent, &name)?.is_none() {
                return Ok(name);
            }
        }
        anyhow::bail!("could not allocate a unique temporary auth journal")
    }

    fn open_lock(parent: &File, key: &str) -> anyhow::Result<(CString, Arc<File>)> {
        let name = CString::new(format!(".jackin-auth-lock-{key}"))?;
        let mut file = None;
        for _ in 0..128 {
            match open_private_file(
                parent,
                &name,
                OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NONBLOCK,
                Mode::from_bits_truncate(0o600),
                "opening auth target lock",
            ) {
                Ok(candidate) => {
                    file = Some(candidate);
                    break;
                }
                Err(error)
                    if error
                        .chain()
                        .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
                {
                    std::thread::yield_now();
                }
                Err(error) => return Err(error),
            }
        }
        let file = file.ok_or_else(|| {
            anyhow::anyhow!("auth target lock parent disappeared during creation")
        })?;
        fchmod(&file, Mode::from_bits_truncate(0o600))
            .map_err(|error| nix_error(error, "restricting auth target lock"))?;
        FileExt::lock(&file).with_context(|| "locking auth target")?;
        Ok((name, Arc::new(file)))
    }

    fn target_lock(path: &Path, create_parent: bool) -> anyhow::Result<TargetLock> {
        let (parent, target, normalized) = open_parent(path, create_parent)?;
        let key = path_key(&normalized)?;
        let (_lock_name, lock) = open_lock(&parent, &key)?;
        let journal = CString::new(format!(".jackin-auth-journal-{key}"))?;
        let target_lock = TargetLock {
            parent,
            target,
            key,
            journal,
            _lock: lock,
        };
        recover(&target_lock)?;
        Ok(target_lock)
    }

    fn new_private_file_temporary(parent: &File) -> anyhow::Result<CString> {
        for _ in 0..128 {
            let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = CString::new(format!(
                ".jackin-auth-file-{}-{sequence}",
                std::process::id()
            ))?;
            if entry_stat(parent, &name)?.is_none() {
                return Ok(name);
            }
        }
        anyhow::bail!("could not allocate a unique private auth file")
    }

    fn file_target_lock(path: &Path) -> anyhow::Result<(File, CString, PathBuf, AuthMountLease)> {
        let (parent, target, normalized) = open_parent(path, false)?;
        let key = path_key(&normalized)?;
        let (_lock_name, lock) = open_lock(&parent, &key)?;
        Ok((parent, target, normalized, AuthMountLease { _lock: lock }))
    }

    fn entry_file_at(
        parent: &File,
        target: &CStr,
        normalized: &Path,
        label: &str,
    ) -> anyhow::Result<Option<File>> {
        let Some(expected) = entry_stat(parent, target)? else {
            return Ok(None);
        };
        validate_owned_stat(&expected, label, SFlag::S_IFREG)?;
        let file = owned_fd(
            openat(
                parent,
                target,
                OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| nix_error(error, label))?,
        );
        let actual = fstat(&file).map_err(|error| nix_error(error, label))?;
        validate_owned_stat(&actual, label, SFlag::S_IFREG)?;
        ensure_same_source_identity(
            &expected,
            &actual,
            &format!("{label} {}", normalized.display()),
        )?;
        Ok(Some(file))
    }

    pub(crate) fn replace_private_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        let (parent, target, normalized, _lease) = file_target_lock(path)?;
        if let Some(expected) = entry_stat(&parent, &target)? {
            let kind = SFlag::from_bits_truncate(expected.st_mode);
            anyhow::ensure!(
                kind.contains(SFlag::S_IFREG),
                "refusing to replace non-regular auth file at {}",
                normalized.display()
            );
            anyhow::ensure!(
                expected.st_uid == geteuid().as_raw(),
                "auth file at {} is not owned by the current user",
                normalized.display()
            );
            let existing = owned_fd(
                openat(
                    &parent,
                    target.as_c_str(),
                    OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|error| nix_error(error, "opening existing private auth file"))?,
            );
            let actual = fstat(&existing)
                .map_err(|error| nix_error(error, "statting existing private auth file"))?;
            validate_owned_stat(&actual, "existing private auth file", SFlag::S_IFREG)?;
            ensure_same_source_identity(&expected, &actual, "existing private auth file")?;
            let mut existing_bytes = Vec::new();
            Read::by_ref(&mut &existing)
                .read_to_end(&mut existing_bytes)
                .context("reading existing private auth file")?;
            if existing_bytes == bytes {
                fchmod(&existing, Mode::from_bits_truncate(0o600))
                    .map_err(|error| nix_error(error, "restricting private auth file"))?;
                existing.sync_all().context("syncing private auth file")?;
                return Ok(());
            }
        }
        let temporary = new_private_file_temporary(&parent)?;
        let result = (|| {
            write_private_file_at(&parent, &temporary, bytes, "private auth file")?;
            renameat(&parent, temporary.as_c_str(), &parent, target.as_c_str())
                .map_err(|error| nix_error(error, "publishing private auth file"))?;
            fsync_directory(&parent)
        })();
        if result.is_err() {
            let _ignored_cleanup =
                unlink_entry(&parent, &temporary, "removing failed private auth file");
        }
        result
    }

    pub(crate) fn create_private_file_if_absent(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        let (parent, target, normalized, _lease) = file_target_lock(path)?;
        let fd = match openat(
            &parent,
            target.as_c_str(),
            OFlag::O_WRONLY
                | OFlag::O_CREAT
                | OFlag::O_EXCL
                | OFlag::O_NOFOLLOW
                | OFlag::O_CLOEXEC
                | OFlag::O_NONBLOCK,
            Mode::from_bits_truncate(0o600),
        ) {
            Ok(fd) => fd,
            Err(Errno::EEXIST) => {
                let stat = entry_stat(&parent, &target)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "private auth file {} disappeared after create collision",
                        normalized.display()
                    )
                })?;
                validate_owned_stat(&stat, "existing private auth file", SFlag::S_IFREG)?;
                let file = owned_fd(
                    openat(
                        &parent,
                        target.as_c_str(),
                        OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|error| nix_error(error, "opening existing private auth file"))?,
                );
                let actual = fstat(&file)
                    .map_err(|error| nix_error(error, "statting existing private auth file"))?;
                validate_owned_stat(&actual, "existing private auth file", SFlag::S_IFREG)?;
                ensure_same_source_identity(&stat, &actual, "existing private auth file")?;
                return Ok(());
            }
            Err(error) => return Err(nix_error(error, "creating private auth file")),
        };
        let file = owned_fd(fd);
        let stat = fstat(&file).map_err(|error| nix_error(error, "statting private auth file"))?;
        validate_owned_stat(&stat, "private auth file", SFlag::S_IFREG)?;
        fchmod(&file, Mode::from_bits_truncate(0o600))
            .map_err(|error| nix_error(error, "restricting private auth file"))?;
        let mut file = file;
        file.write_all(bytes)
            .with_context(|| format!("writing private skeleton at {}", normalized.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing private skeleton at {}", normalized.display()))
    }

    pub(crate) fn remove_file(path: &Path) -> anyhow::Result<()> {
        let (parent, target, _, _lease) = match file_target_lock(path) {
            Ok(value) => value,
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        match unlinkat(&parent, target.as_c_str(), UnlinkatFlags::NoRemoveDir) {
            Ok(()) | Err(Errno::ENOENT) => Ok(()),
            Err(error) => Err(nix_error(error, "removing private auth file")),
        }
    }

    pub(crate) fn repair_file_permissions(path: &Path) -> anyhow::Result<()> {
        let (parent, target, normalized, _lease) = match file_target_lock(path) {
            Ok(value) => value,
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        super::maybe_inject_permission_repair_failure(super::PermissionRepairFailure::Stat)?;
        let Some(expected) = entry_stat(&parent, &target)? else {
            return Ok(());
        };
        validate_owned_stat(&expected, "credential file", SFlag::S_IFREG)?;
        anyhow::ensure!(
            expected.st_uid == geteuid().as_raw(),
            "credential file at {} is not owned by the current user",
            normalized.display()
        );
        super::maybe_inject_permission_repair_failure(super::PermissionRepairFailure::Chmod)?;
        let file = owned_fd(
            openat(
                &parent,
                target.as_c_str(),
                OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| nix_error(error, "opening credential file for permission repair"))?,
        );
        let actual = fstat(&file)
            .map_err(|error| nix_error(error, "statting credential file for permission repair"))?;
        validate_owned_stat(&actual, "credential file", SFlag::S_IFREG)?;
        ensure_same_source_identity(
            &expected,
            &actual,
            &format!("credential file {}", normalized.display()),
        )?;
        fchmod(&file, Mode::from_bits_truncate(0o600))
            .map_err(|error| nix_error(error, "chmod 0o600 on credential file"))?;
        super::maybe_inject_permission_repair_failure(super::PermissionRepairFailure::Verify)?;
        let verified = fstat(&file)
            .map_err(|error| nix_error(error, "verifying credential file permissions"))?;
        ensure_same_source_identity(
            &expected,
            &verified,
            &format!("credential file {}", normalized.display()),
        )?;
        anyhow::ensure!(
            verified.st_mode & 0o7777 == 0o600,
            "credential file at {} is not exactly mode 0600 after repair",
            normalized.display()
        );
        Ok(())
    }

    pub(crate) fn mount_file_present(path: &Path) -> anyhow::Result<bool> {
        let (parent, target, normalized) = match open_parent(path, false) {
            Ok(value) => value,
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        Ok(entry_file_at(&parent, &target, &normalized, "credential mount file")?.is_some())
    }

    pub(crate) fn mount_directory_present(path: &Path) -> anyhow::Result<bool> {
        let (parent, target, normalized) = match open_parent(path, false) {
            Ok(value) => value,
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let Some(expected) = entry_stat(&parent, &target)? else {
            return Ok(false);
        };
        validate_owned_stat(&expected, "credential mount directory", SFlag::S_IFDIR)?;
        let directory = open_directory_at(
            &parent,
            &target,
            &format!(
                "opening credential mount directory {}",
                normalized.display()
            ),
        )?;
        let actual = fstat(&directory)
            .map_err(|error| nix_error(error, "statting credential mount directory"))?;
        ensure_same_source_identity(&expected, &actual, "credential mount directory")?;
        Ok(true)
    }

    pub(crate) fn lock_mount_file(path: &Path) -> anyhow::Result<Option<AuthMountLease>> {
        let (parent, target, normalized, lease) = match file_target_lock(path) {
            Ok(value) => value,
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        if entry_file_at(&parent, &target, &normalized, "credential mount file")?.is_some() {
            Ok(Some(lease))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn lock_mount_directory(path: &Path) -> anyhow::Result<Option<AuthMountLease>> {
        let (parent, target, normalized, lease) = match file_target_lock(path) {
            Ok(value) => value,
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let Some(expected) = entry_stat(&parent, &target)? else {
            return Ok(None);
        };
        validate_owned_stat(&expected, "credential mount directory", SFlag::S_IFDIR)?;
        let directory = open_directory_at(
            &parent,
            &target,
            &format!(
                "opening credential mount directory {}",
                normalized.display()
            ),
        )?;
        let actual = fstat(&directory)
            .map_err(|error| nix_error(error, "statting credential mount directory"))?;
        ensure_same_source_identity(&expected, &actual, "credential mount directory")?;
        Ok(Some(lease))
    }

    #[cfg(test)]
    pub(crate) fn target_lock_key_for_test(path: &Path) -> anyhow::Result<String> {
        path_key(path)
    }

    fn target_present(target: &TargetLock) -> anyhow::Result<bool> {
        let Some(stat) = entry_stat(&target.parent, &target.target)? else {
            return Ok(false);
        };
        validate_owned_stat(&stat, "existing auth destination", SFlag::S_IFDIR)?;
        Ok(true)
    }

    fn write_journal(target: &TargetLock, journal: &SwapJournal) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec(journal).context("serializing auth swap journal")?;
        let temporary = new_journal_temporary(&target.parent, &target.key)?;
        let result = (|| {
            let file = open_private_file(
                &target.parent,
                &temporary,
                OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NONBLOCK,
                Mode::from_bits_truncate(0o600),
                "opening temporary auth swap journal",
            )?;
            let mut file = file;
            file.write_all(&bytes)
                .context("writing temporary auth swap journal")?;
            file.sync_all()
                .context("syncing temporary auth swap journal")?;

            let replacing = entry_stat(&target.parent, &target.journal)?.is_some();
            if replacing {
                let stat = entry_stat(&target.parent, &target.journal)?.ok_or_else(|| {
                    anyhow::anyhow!("auth swap journal disappeared during atomic rewrite")
                })?;
                validate_owned_stat(&stat, "auth swap journal", SFlag::S_IFREG)?;
                maybe_fail(FailurePoint::JournalRewrite)?;
            }
            renameat(
                &target.parent,
                temporary.as_c_str(),
                &target.parent,
                target.journal.as_c_str(),
            )
            .map_err(|error| nix_error(error, "publishing auth swap journal"))?;
            fsync_directory(&target.parent)
        })();
        if result.is_err() {
            let _ignored_cleanup = unlink_entry(
                &target.parent,
                &temporary,
                "removing failed temporary auth swap journal",
            );
        }
        result
    }

    fn read_journal(target: &TargetLock) -> anyhow::Result<Option<SwapJournal>> {
        let Some(stat) = entry_stat(&target.parent, &target.journal)? else {
            return Ok(None);
        };
        validate_owned_stat(&stat, "auth swap journal", SFlag::S_IFREG)?;
        let file = open_private_file(
            &target.parent,
            &target.journal,
            OFlag::O_RDONLY | OFlag::O_NONBLOCK,
            Mode::empty(),
            "opening auth swap journal",
        )?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut &file)
            .take((MAX_JOURNAL_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .context("reading auth swap journal")?;
        anyhow::ensure!(
            bytes.len() <= MAX_JOURNAL_BYTES,
            "auth swap journal is oversized"
        );
        let journal = serde_json::from_slice(&bytes).context("parsing auth swap journal")?;
        Ok(Some(journal))
    }

    fn unlink_entry(parent: &File, name: &CStr, label: &str) -> anyhow::Result<()> {
        match unlinkat(parent, name, UnlinkatFlags::NoRemoveDir) {
            Ok(()) => Ok(()),
            Err(Errno::ENOENT) => Ok(()),
            Err(error) => Err(nix_error(error, label)),
        }
    }

    /// How `remove_tree` must treat one directory entry, matched on exact
    /// `S_IFMT` bits. `SFlag::contains` on whole file-type flags over-matches
    /// (`S_IFLNK` contains the `S_IFREG` bit), which previously routed symlinks
    /// into the regular-file writability check — and `lstat` mode bits on a
    /// symlink are meaningless (Linux always reports `0777`, macOS `0755`), so
    /// legitimate Linux trees were rejected as group-writable.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum TreeEntryKind {
        Directory,
        Regular,
        Symlink,
        Special,
    }

    pub(crate) fn classify_tree_entry_for_removal(mode: mode_t) -> TreeEntryKind {
        let file_type = mode & SFlag::S_IFMT.bits();
        if file_type == SFlag::S_IFDIR.bits() {
            TreeEntryKind::Directory
        } else if file_type == SFlag::S_IFREG.bits() {
            TreeEntryKind::Regular
        } else if file_type == SFlag::S_IFLNK.bits() {
            TreeEntryKind::Symlink
        } else {
            TreeEntryKind::Special
        }
    }

    fn remove_tree(parent: &File, name: &CStr, label: &str) -> anyhow::Result<()> {
        let Some(stat) = entry_stat(parent, name)? else {
            return Ok(());
        };
        validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
        let directory = open_directory_at(parent, name, label)?;
        let clone = directory.try_clone()?;
        let mut entries = Dir::from_fd(clone.into())
            .map_err(|error| nix_error(error, "opening auth cleanup directory"))?;
        let mut names = Vec::new();
        for entry in entries.iter() {
            let entry =
                entry.map_err(|error| nix_error(error, "reading auth cleanup directory"))?;
            let entry_name = entry.file_name();
            if entry_name.to_bytes() != b"." && entry_name.to_bytes() != b".." {
                names.push(entry_name.to_owned());
            }
        }
        for entry_name in names {
            let Some(entry_stat) = entry_stat(&directory, &entry_name)? else {
                continue;
            };
            match classify_tree_entry_for_removal(entry_stat.st_mode) {
                TreeEntryKind::Directory => {
                    remove_tree(&directory, &entry_name, label)?;
                }
                TreeEntryKind::Regular => {
                    validate_owned_stat(&entry_stat, label, SFlag::S_IFREG)?;
                    unlink_entry(&directory, &entry_name, "removing auth file")?;
                }
                TreeEntryKind::Symlink => {
                    // `lstat` mode bits on a symlink carry no access meaning
                    // (Linux always reports 0777, macOS 0755), so only the
                    // link's ownership is checked. `unlinkat` never follows
                    // the link, so removing it cannot touch its target.
                    anyhow::ensure!(
                        entry_stat.st_uid == geteuid().as_raw(),
                        "{label} is not owned by the current user"
                    );
                    unlink_entry(&directory, &entry_name, "removing auth symlink")?;
                }
                TreeEntryKind::Special => {
                    anyhow::bail!("{label} contains a special file entry")
                }
            }
        }
        fsync_directory(&directory)?;
        unlinkat(parent, name, UnlinkatFlags::RemoveDir)
            .map_err(|error| nix_error(error, "removing auth directory"))?;
        fsync_directory(parent)
    }

    fn valid_transaction_name(name: &str, key: &str, kind: &str) -> bool {
        name.starts_with(&format!(".jackin-auth-{kind}-{key}-"))
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
    }

    fn previous_name(previous: Option<&CString>) -> anyhow::Result<&CStr> {
        previous
            .map(CString::as_c_str)
            .ok_or_else(|| anyhow::anyhow!("auth swap journal refers to a missing previous name"))
    }

    fn recover(target: &TargetLock) -> anyhow::Result<()> {
        let Some(journal) = read_journal(target)? else {
            cleanup_orphans(target)?;
            return Ok(());
        };
        anyhow::ensure!(
            journal.schema_version == JOURNAL_SCHEMA_VERSION,
            "unsupported auth swap journal schema"
        );
        anyhow::ensure!(
            journal.target == hex::encode(target.target.as_bytes()),
            "auth swap journal targets a different directory"
        );
        anyhow::ensure!(
            valid_transaction_name(&journal.stage, &target.key, "stage"),
            "auth swap journal contains an invalid stage name"
        );
        if let Some(previous) = &journal.previous {
            anyhow::ensure!(
                valid_transaction_name(previous, &target.key, "previous"),
                "auth swap journal contains an invalid previous name"
            );
        }
        let stage = CString::new(journal.stage.as_str())?;
        let previous = journal.previous.as_deref().map(CString::new).transpose()?;
        let target_exists = target_present(target)?;
        let previous_exists = previous
            .as_ref()
            .map(|name| entry_stat(&target.parent, name).map(|stat| stat.is_some()))
            .transpose()?
            .unwrap_or(false);

        match journal.phase {
            SwapPhase::Prepared => {
                if !target_exists && previous_exists {
                    renameat(
                        &target.parent,
                        previous_name(previous.as_ref())?,
                        &target.parent,
                        target.target.as_c_str(),
                    )
                    .map_err(|error| nix_error(error, "restoring prepared auth swap"))?;
                    fsync_directory(&target.parent)?;
                } else if target_exists && previous_exists {
                    remove_tree(
                        &target.parent,
                        previous_name(previous.as_ref())?,
                        "stale auth previous directory",
                    )?;
                }
            }
            SwapPhase::BackedUp => {
                if !target_exists && previous_exists {
                    renameat(
                        &target.parent,
                        previous_name(previous.as_ref())?,
                        &target.parent,
                        target.target.as_c_str(),
                    )
                    .map_err(|error| nix_error(error, "restoring backed-up auth swap"))?;
                    fsync_directory(&target.parent)?;
                } else if target_exists && previous_exists {
                    remove_tree(
                        &target.parent,
                        previous_name(previous.as_ref())?,
                        "completed auth previous directory",
                    )?;
                } else if !target_exists {
                    anyhow::bail!("auth swap journal has neither destination nor previous tree");
                }
            }
            SwapPhase::Installed => {
                if !target_exists && previous_exists {
                    renameat(
                        &target.parent,
                        previous_name(previous.as_ref())?,
                        &target.parent,
                        target.target.as_c_str(),
                    )
                    .map_err(|error| nix_error(error, "restoring lost installed auth swap"))?;
                    fsync_directory(&target.parent)?;
                } else if previous_exists {
                    remove_tree(
                        &target.parent,
                        previous_name(previous.as_ref())?,
                        "installed auth previous directory",
                    )?;
                }
            }
        }
        remove_tree(&target.parent, &stage, "orphaned auth stage")?;
        unlink_entry(
            &target.parent,
            &target.journal,
            "removing auth swap journal",
        )?;
        fsync_directory(&target.parent)?;
        cleanup_orphans(target)
    }

    fn cleanup_orphans(target: &TargetLock) -> anyhow::Result<()> {
        let clone = target.parent.try_clone()?;
        let mut entries = Dir::from_fd(clone.into())
            .map_err(|error| nix_error(error, "opening auth parent for orphan cleanup"))?;
        // Transaction names created by this implementation carry the target
        // identity. Pre-846f984 names do not, so there is no safe way to
        // attribute those legacy trees to this target; leave them untouched.
        let new_stage_prefix = format!(".jackin-auth-stage-{}-", target.key);
        let new_previous_prefix = format!(".jackin-auth-previous-{}-", target.key);
        let new_journal_temporary_prefix = format!(".jackin-auth-journal-{}-tmp-", target.key);
        let mut directories = Vec::new();
        let mut journal_temporaries = Vec::new();
        for entry in entries.iter() {
            let entry = entry.map_err(|error| nix_error(error, "reading auth parent"))?;
            let name = entry.file_name();
            let text = name.to_string_lossy();
            if text.starts_with(&new_stage_prefix) || text.starts_with(&new_previous_prefix) {
                directories.push(name.to_owned());
            } else if text.starts_with(&new_journal_temporary_prefix) {
                journal_temporaries.push(name.to_owned());
            }
        }
        for name in directories {
            remove_tree(&target.parent, &name, "orphaned auth swap directory")?;
        }
        for name in journal_temporaries {
            let stat = entry_stat(&target.parent, &name)?.ok_or_else(|| {
                anyhow::anyhow!("temporary auth swap journal disappeared during cleanup")
            })?;
            validate_owned_stat(&stat, "temporary auth swap journal", SFlag::S_IFREG)?;
            unlink_entry(
                &target.parent,
                &name,
                "removing orphaned temporary auth swap journal",
            )?;
        }
        Ok(())
    }

    fn publish(target: &TargetLock, stage: CString) -> anyhow::Result<()> {
        let has_target = target_present(target)?;
        let previous = has_target
            .then(|| new_previous(&target.parent, &target.key))
            .transpose()?;
        let journal = SwapJournal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            target: hex::encode(target.target.as_bytes()),
            stage: stage.to_string_lossy().into_owned(),
            previous: previous
                .as_ref()
                .map(|name| name.to_string_lossy().into_owned()),
            phase: SwapPhase::Prepared,
        };
        write_journal(target, &journal)?;
        maybe_fail(FailurePoint::Prepared)?;

        if let Some(previous) = &previous {
            renameat(
                &target.parent,
                target.target.as_c_str(),
                &target.parent,
                previous.as_c_str(),
            )
            .map_err(|error| nix_error(error, "moving previous auth directory"))?;
            fsync_directory(&target.parent)?;
            write_journal(
                target,
                &SwapJournal {
                    phase: SwapPhase::BackedUp,
                    ..journal.clone()
                },
            )?;
            maybe_fail(FailurePoint::Backup)?;
        }

        renameat(
            &target.parent,
            stage.as_c_str(),
            &target.parent,
            target.target.as_c_str(),
        )
        .map_err(|error| nix_error(error, "publishing staged auth directory"))?;
        fsync_directory(&target.parent)?;
        let installed = SwapJournal {
            phase: SwapPhase::Installed,
            ..journal
        };
        write_journal(target, &installed)?;
        maybe_fail(FailurePoint::Installed)?;

        if let Some(previous) = &previous {
            remove_tree(&target.parent, previous, "previous auth directory")?;
        }
        unlink_entry(
            &target.parent,
            &target.journal,
            "removing auth swap journal",
        )?;
        fsync_directory(&target.parent)
    }

    pub(crate) fn stage_auth_directory_with_locked_source<F>(
        target_dir: &Path,
        host_dir: &Path,
        source: Option<LockedSource>,
        populate: F,
    ) -> anyhow::Result<AuthProvisionOutcome>
    where
        F: FnOnce(&Path, &File, &File) -> anyhow::Result<()>,
    {
        let target = target_lock(target_dir, true).map_err(|error| {
            anyhow::anyhow!("opening auth target {}: {error:#}", target_dir.display())
        })?;
        let outcome = if let Some(source) = &source {
            let (stage, directory) = new_stage(&target.parent, &target.key)?;
            if let Err(error) = populate(host_dir, &source.root, &directory) {
                let _ignored_cleanup = remove_tree(&target.parent, &stage, "failed auth stage");
                return Err(error);
            }
            fsync_directory(&directory)?;
            drop(directory);
            publish(&target, stage)?;
            AuthProvisionOutcome::Synced
        } else {
            let (stage, directory) = new_stage(&target.parent, &target.key)?;
            fsync_directory(&directory)?;
            drop(directory);
            publish(&target, stage)?;
            AuthProvisionOutcome::HostMissing
        };
        Ok(outcome)
    }

    pub(crate) fn wipe_auth_directory(target_dir: &Path) -> anyhow::Result<()> {
        let target = match target_lock(target_dir, false) {
            Ok(target) => target,
            Err(error) if error.downcast_ref::<Errno>() == Some(&Errno::ENOENT) => return Ok(()),
            Err(error) => return Err(error),
        };
        if target_present(&target)? {
            remove_tree(&target.parent, &target.target, "auth destination")?;
        }
        Ok(())
    }

    pub(crate) fn snapshot_source(source: &File, snapshot: &File) -> anyhow::Result<()> {
        copy_tree(source, snapshot, "Hermes source snapshot")
    }
}

#[cfg(not(unix))]
mod auth_directory {
    use super::*;
    use std::fs::File;

    #[derive(Clone, Debug)]
    pub struct AuthMountLease;

    pub(crate) fn mount_file_present(path: &Path) -> anyhow::Result<bool> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(metadata.is_file(), "credential mount is not a regular file");
        Ok(true)
    }

    pub(crate) fn mount_directory_present(path: &Path) -> anyhow::Result<bool> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(metadata.is_dir(), "credential mount is not a directory");
        Ok(true)
    }

    pub(crate) fn lock_mount_file(path: &Path) -> anyhow::Result<Option<AuthMountLease>> {
        Ok(mount_file_present(path)?.then_some(AuthMountLease))
    }

    pub(crate) fn lock_mount_directory(path: &Path) -> anyhow::Result<Option<AuthMountLease>> {
        Ok(mount_directory_present(path)?.then_some(AuthMountLease))
    }

    pub(crate) fn read_source_path(path: &Path, label: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(
            !metadata.file_type().is_symlink() && metadata.is_file(),
            "{label} is not a regular source file"
        );
        Ok(Some(super::read_bounded_local_file(path)?))
    }

    pub(crate) fn stage_auth_directory<F>(
        target_dir: &Path,
        host_dir: &Path,
        populate: F,
    ) -> anyhow::Result<AuthProvisionOutcome>
    where
        F: FnOnce(&Path, &File, &File) -> anyhow::Result<()>,
    {
        let parent = target_dir.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let staged = tempfile::Builder::new()
            .prefix(".jackin-auth-stage-")
            .tempdir_in(parent)?;
        let staged_file = File::open(staged.path())?;
        let outcome = if host_dir.is_dir() {
            let source = File::open(host_dir)?;
            populate(host_dir, &source, &staged_file)?;
            AuthProvisionOutcome::Synced
        } else {
            AuthProvisionOutcome::HostMissing
        };
        std::fs::rename(staged.path(), target_dir)?;
        Ok(outcome)
    }

    pub(crate) fn wipe_auth_directory(target_dir: &Path) -> anyhow::Result<()> {
        if target_dir.exists() {
            std::fs::remove_dir_all(target_dir)?;
        }
        Ok(())
    }
}

pub use auth_directory::AuthMountLease;

pub(crate) fn mount_file_present(path: &Path) -> anyhow::Result<bool> {
    auth_directory::mount_file_present(path)
}

pub(crate) fn mount_directory_present(path: &Path) -> anyhow::Result<bool> {
    auth_directory::mount_directory_present(path)
}

pub(crate) fn admit_auth_mounts(
    auth: &super::ProvisionedAuth,
) -> anyhow::Result<(std::collections::BTreeSet<PathBuf>, Vec<AuthMountLease>)> {
    let mut requests = Vec::<(PathBuf, bool, Agent)>::new();
    for slot in auth.slots.values() {
        if !slot.forward_auth {
            continue;
        }
        let directory = matches!(slot.agent, Agent::Kimi | Agent::Hermes);
        for path in &slot.credential_paths {
            requests.push((path.clone(), directory, slot.agent));
            if !directory && let Some(parent) = path.parent() {
                requests.push((parent.to_path_buf(), true, slot.agent));
            }
        }
    }
    requests
        .sort_by(|left, right| (left.0.as_os_str(), left.1).cmp(&(right.0.as_os_str(), right.1)));
    requests.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);

    let mut paths = std::collections::BTreeSet::new();
    let mut leases = Vec::new();
    for (path, directory, agent) in requests {
        let lease = if directory {
            auth_directory::lock_mount_directory(&path)?
        } else {
            auth_directory::lock_mount_file(&path)?
        };
        let Some(lease) = lease else {
            if agent != Agent::Claude {
                anyhow::bail!(
                    "{agent} auth mount path disappeared before launch: {}",
                    path.display()
                );
            }
            continue;
        };
        paths.insert(path);
        leases.push(lease);
    }
    Ok((paths, leases))
}

impl RoleState {
    /// Provision `OpenCode`'s host-side `auth.json` per the chosen mode.
    ///
    /// Source: `~/.local/share/opencode/auth.json` (`XDG_DATA`).
    /// `OpenCode` stores provider credentials (e.g. Z.AI Coding Plan API
    /// keys) in this file.
    ///
    /// Follows the same semantics as `provision_amp_auth`.
    pub(super) fn provision_opencode_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_opencode_auth_from_path(
            auth_json,
            mode,
            &host_home.join(".local/share/opencode/auth.json"),
            None,
        )
    }

    pub(super) fn provision_opencode_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
        provider: Option<AiProvider>,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_opencode_auth_from_path(
            auth_json,
            mode,
            &source_dir.join("auth.json"),
            provider,
        )
    }

    fn provision_opencode_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
        provider: Option<AiProvider>,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        if mode == AuthForwardMode::Sync {
            reject_auth_path(auth_json)?;
            let Some(content) = read_source_text(host_auth_json, "OpenCode auth.json")? else {
                repair_permissions(auth_json)?;
                return Ok((
                    AuthProvisionOutcome::HostMissing,
                    private_file_exists(auth_json)?.then(|| auth_json.to_path_buf()),
                ));
            };
            if content.trim().is_empty() {
                // A present-but-blank ambient file is invalid input, not an
                // absent host login. Preserve the documented missing-file
                // behavior (an in-container login may survive), but never
                // carry stale role-state credentials across this invalidation.
                wipe_agent_file_state(auth_json, "OpenCode auth.json")?;
                return Ok((AuthProvisionOutcome::HostMissing, None));
            }
            let value = serde_json::from_str::<serde_json::Value>(&content)
                .map_err(|_| anyhow::anyhow!("OpenCode auth.json is malformed"))?;
            let (key, entry) = select_opencode_auth_entry(&value, provider).map_err(|reason| {
                anyhow::anyhow!("OpenCode auth.json cannot be selected safely: {reason}")
            })?;
            let mut selected = serde_json::Map::new();
            selected.insert(key.to_owned(), entry.clone());
            let selected = serde_json::to_vec(&serde_json::Value::Object(selected))
                .context("serializing selected OpenCode credential")?;
            write_private_bytes(auth_json, &selected)
                .context("writing selected OpenCode credential")?;
            return Ok((AuthProvisionOutcome::Synced, Some(auth_json.to_path_buf())));
        }
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "OpenCode auth.json",
            "OpenCode",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Grok's host-side `~/.grok/auth.json` per the chosen mode.
    ///
    /// The auth.json carries OAuth / OIDC tokens (from `grok login`) and is
    /// the handoff for the browser-based login flow. `GROK_DEPLOYMENT_KEY` or
    /// `XAI_API_KEY` in the env take precedence inside the CLI (per install
    /// script and docs); when present we still allow a Sync mount so any
    /// supplementary config or prior tokens are available, but ApiKey/Ignore
    /// correctly suppress the file to force env-only auth.
    pub(super) fn provision_grok_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_grok_auth_from_path(auth_json, mode, &host_home.join(".grok/auth.json"))
    }

    pub(super) fn provision_grok_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_grok_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    fn provision_grok_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Grok auth.json",
            "Grok",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Antigravity's host-side `settings.json` per the chosen mode.
    ///
    /// Source: `~/.gemini/antigravity-cli/settings.json`. Prefs only — the
    /// OAuth grant lives in the host Keychain singleton and cannot be
    /// synced, so Sync mode forwards preferences while real auth comes
    /// from `GEMINI_API_KEY` (`ApiKey` mode) or in-container login.
    pub(super) fn provision_antigravity_auth(
        settings_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_antigravity_auth_from_path(
            settings_json,
            mode,
            &host_home.join(".gemini/antigravity-cli/settings.json"),
        )
    }

    pub(super) fn provision_antigravity_auth_from_source_dir(
        settings_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_antigravity_auth_from_path(
            settings_json,
            mode,
            &source_dir.join("settings.json"),
        )
    }

    fn provision_antigravity_auth_from_path(
        settings_json: &Path,
        mode: AuthForwardMode,
        host_settings_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            settings_json,
            host_settings_json,
            mode,
            "Antigravity settings.json",
            "Antigravity",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Gemini CLI's host-side `~/.gemini/oauth_creds.json` per the
    /// chosen mode. Follows the same semantics as `provision_grok_auth`.
    pub(super) fn provision_gemini_auth(
        oauth_creds: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_gemini_auth_from_path(
            oauth_creds,
            mode,
            &host_home.join(".gemini/oauth_creds.json"),
        )
    }

    pub(super) fn provision_gemini_auth_from_source_dir(
        oauth_creds: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_gemini_auth_from_path(
            oauth_creds,
            mode,
            &source_dir.join("oauth_creds.json"),
        )
    }

    fn provision_gemini_auth_from_path(
        oauth_creds: &Path,
        mode: AuthForwardMode,
        host_oauth_creds: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            oauth_creds,
            host_oauth_creds,
            mode,
            "Gemini oauth_creds.json",
            "Gemini",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Cursor's host-side `~/.cursor/auth.json` per the chosen mode.
    /// Follows the same semantics as `provision_grok_auth`.
    pub(super) fn provision_cursor_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_cursor_auth_from_path(auth_json, mode, &host_home.join(".cursor/auth.json"))
    }

    pub(super) fn provision_cursor_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_cursor_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    fn provision_cursor_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Cursor auth.json",
            "Cursor",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision Muse's host-side `~/.config/muse/auth.json` per the chosen
    /// mode. Follows the same semantics as `provision_grok_auth`. The file
    /// carries identity fields; the secret itself stays in the host
    /// Keychain, so a synced file alone may still require in-container
    /// login or `META_API_KEY`.
    pub(super) fn provision_muse_auth(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_muse_auth_from_path(
            auth_json,
            mode,
            &host_home.join(".config/muse/auth.json"),
        )
    }

    pub(super) fn provision_muse_auth_from_source_dir(
        auth_json: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_muse_auth_from_path(auth_json, mode, &source_dir.join("auth.json"))
    }

    fn provision_muse_auth_from_path(
        auth_json: &Path,
        mode: AuthForwardMode,
        host_auth_json: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_file_credential(
            auth_json,
            host_auth_json,
            mode,
            "Muse auth.json",
            "Muse",
            true,
            true,
            true,
        )
    }
}

impl RoleState {
    /// Provision omp's host-side `~/.omp/agent/agent.db` (`SQLite`) per the
    /// chosen mode. Byte-oriented twin of `provision_grok_auth`: the store
    /// is binary, so the UTF-8 provisioner cannot be used.
    pub(super) fn provision_omp_auth(
        agent_db: &Path,
        mode: AuthForwardMode,
        host_home: &Path,
        provider: Option<AiProvider>,
        selector: Option<&ProfileSelector>,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        Self::provision_omp_auth_from_source_dir(
            agent_db,
            mode,
            &host_home.join(".omp"),
            provider,
            selector,
        )
    }

    pub(super) fn provision_omp_auth_from_source_dir(
        agent_db: &Path,
        mode: AuthForwardMode,
        source_dir: &Path,
        provider: Option<AiProvider>,
        selector: Option<&ProfileSelector>,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        #[cfg(unix)]
        if mode == AuthForwardMode::Sync {
            let content = match auth_directory::lock_source_dir(source_dir)? {
                Some(source) => {
                    let content = capture_omp_database_snapshot(&source.root, source_dir)?;
                    validate_omp_store_content(&content, provider, selector)?;
                    Some(content)
                }
                None => None,
            };
            return provision_single_blob_credential_from_content(
                agent_db,
                mode,
                content,
                "omp agent.db",
                "omp",
            );
        }
        #[cfg(not(unix))]
        if mode == AuthForwardMode::Sync {
            let content = capture_omp_database_snapshot_from_paths(source_dir)?;
            if let Some(content) = content.as_deref() {
                validate_omp_store_content(content, provider, selector)?;
            }
            return provision_single_blob_credential_from_content(
                agent_db,
                mode,
                content,
                "omp agent.db",
                "omp",
            );
        }
        Self::provision_omp_auth_from_path(agent_db, mode, &source_dir.join("agent/agent.db"))
    }

    fn provision_omp_auth_from_path(
        agent_db: &Path,
        mode: AuthForwardMode,
        host_agent_db: &Path,
    ) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
        provision_single_blob_credential(agent_db, host_agent_db, mode, "omp agent.db", "omp")
    }
}

#[cfg(unix)]
fn create_hermes_source_snapshot(
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
    pub(super) fn provision_hermes_auth(
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

    pub(super) fn provision_hermes_auth_from_source_dir(
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
fn provision_hermes_dir_credential(
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

fn provision_hermes_dir_credential_with_locked_source(
    target_dir: &Path,
    host_dir: &Path,
    mode: AuthForwardMode,
    #[cfg(unix)] source: Option<auth_directory::LockedSource>,
    #[cfg(not(unix))] _source: (),
) -> anyhow::Result<(AuthProvisionOutcome, bool)> {
    // Best-effort file set; the layout is unverified upstream.
    const SYNC_FILES: &[&str] = &["config.yaml", ".env", "auth.json"];

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
fn wipe_hermes_state(hermes_dir: &Path) -> anyhow::Result<()> {
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

fn read_source_bytes(path: &Path, label: &str) -> anyhow::Result<Option<Vec<u8>>> {
    auth_directory::read_source_path(path, label)
}

#[cfg(unix)]
fn capture_omp_database_snapshot(
    source: &std::fs::File,
    source_dir: &Path,
) -> anyhow::Result<Vec<u8>> {
    for _ in 0..OMP_SOURCE_CAPTURE_ATTEMPTS {
        let first = read_omp_source_pair(source)
            .with_context(|| format!("capturing OMP source {}", source_dir.display()))?;
        let second = read_omp_source_pair(source)
            .with_context(|| format!("capturing OMP source {}", source_dir.display()))?;
        if first == second {
            return materialize_omp_database(&first.database, first.wal.as_deref());
        }
    }
    anyhow::bail!(
        "OMP source {} changed during {} bounded credential snapshot attempts",
        source_dir.display(),
        OMP_SOURCE_CAPTURE_ATTEMPTS
    )
}

#[derive(Debug, Eq, PartialEq)]
struct OmpSourcePair {
    database: Vec<u8>,
    wal: Option<Vec<u8>>,
}

#[cfg(unix)]
fn read_omp_source_pair(source: &std::fs::File) -> anyhow::Result<OmpSourcePair> {
    let database = auth_directory::read_locked_source_file_bounded(
        source,
        &["agent", "agent.db"],
        "omp agent.db",
        MAX_OMP_SOURCE_FILE_BYTES,
    )?
    .ok_or_else(|| anyhow::anyhow!("OMP source has no agent/agent.db"))?;
    auth_directory::run_omp_after_database_read_hook();
    let wal = auth_directory::read_locked_source_file_bounded(
        source,
        &["agent", "agent.db-wal"],
        "omp agent.db-wal",
        MAX_OMP_SOURCE_FILE_BYTES,
    )?;
    Ok(OmpSourcePair { database, wal })
}

/// Fold committed WAL frames into `database`, returning a bounded standalone
/// image suitable for the existing single-file OMP bind mount.
///
/// Frame validation follows SQLite's WAL checksum chain as well as salts,
/// page numbers, and commit boundaries. A checksum discontinuity caused by a
/// stale old-generation suffix is ignored only after a valid current-generation
/// commit; malformed current-generation frames fail closed. An incomplete
/// trailing frame is ignored because it cannot contain a validated commit.
fn materialize_omp_database(database: &[u8], wal: Option<&[u8]>) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        database.len() <= MAX_OMP_SOURCE_FILE_BYTES,
        "omp agent.db exceeds the source size limit"
    );
    anyhow::ensure!(
        database.get(..16) == Some(b"SQLite format 3\0"),
        "omp agent.db is not a SQLite database"
    );
    let raw_page_size = u16::from_be_bytes(
        database
            .get(16..18)
            .ok_or_else(|| anyhow::anyhow!("omp agent.db header is truncated"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("omp agent.db page size is malformed"))?,
    );
    let page_size = if raw_page_size == 1 {
        65_536_usize
    } else {
        usize::from(raw_page_size)
    };
    anyhow::ensure!(
        page_size.is_power_of_two() && (512..=65_536).contains(&page_size),
        "omp agent.db page size is invalid"
    );
    anyhow::ensure!(
        !database.is_empty() && database.len().is_multiple_of(page_size),
        "omp agent.db has an invalid page layout"
    );
    let read_version = database[18];
    let write_version = database[19];
    anyhow::ensure!(
        (read_version == 1 && write_version == 1) || (read_version == 2 && write_version == 2),
        "omp agent.db has unsupported journal-mode header bytes"
    );

    let mut output = database.to_vec();
    if read_version == 2 {
        if let Some(wal) = wal.filter(|wal| !wal.is_empty()) {
            apply_committed_omp_wal(&mut output, page_size, wal)?;
        }
        // The merged image no longer needs a WAL sidecar. Mark it as a
        // rollback-journal database before the one-file mount is published.
        output[18] = 1;
        output[19] = 1;
    } else {
        anyhow::ensure!(
            wal.is_none_or(<[u8]>::is_empty),
            "rollback-journal omp agent.db has a non-empty WAL"
        );
    }
    anyhow::ensure!(
        output.len() <= MAX_OMP_SOURCE_FILE_BYTES,
        "materialized omp agent.db exceeds the source size limit"
    );
    Ok(output)
}

struct OmpWalCommit {
    frame_len: usize,
    last_frame_index: usize,
    page_count: usize,
}

fn apply_committed_omp_wal(
    database: &mut Vec<u8>,
    page_size: usize,
    wal: &[u8],
) -> anyhow::Result<()> {
    let database_pages = database.len() / page_size;
    let Some(commit) = validate_omp_wal(wal, page_size, database_pages)? else {
        return Ok(());
    };
    let output_len = commit
        .page_count
        .checked_mul(page_size)
        .ok_or_else(|| anyhow::anyhow!("materialized omp database size overflow"))?;
    anyhow::ensure!(
        output_len <= MAX_OMP_SOURCE_FILE_BYTES,
        "materialized omp agent.db exceeds the source size limit"
    );
    database.resize(output_len, 0);
    for index in 0..=commit.last_frame_index {
        let frame_at = 32_usize
            .checked_add(
                index
                    .checked_mul(commit.frame_len)
                    .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?,
            )
            .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?;
        let page_number = usize::try_from(read_omp_u32(wal, frame_at)?)
            .map_err(|_| anyhow::anyhow!("omp WAL frame page number is invalid"))?;
        if page_number > commit.page_count {
            continue;
        }
        let page_at = page_number
            .checked_sub(1)
            .and_then(|number| number.checked_mul(page_size))
            .ok_or_else(|| anyhow::anyhow!("omp WAL output offset overflow"))?;
        let page_end = page_at
            .checked_add(page_size)
            .ok_or_else(|| anyhow::anyhow!("omp WAL output offset overflow"))?;
        let frame_page_at = frame_at
            .checked_add(24)
            .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?;
        let frame_page_end = frame_page_at
            .checked_add(page_size)
            .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?;
        let frame_page = wal
            .get(frame_page_at..frame_page_end)
            .ok_or_else(|| anyhow::anyhow!("omp WAL frame page is truncated"))?;
        database
            .get_mut(page_at..page_end)
            .ok_or_else(|| anyhow::anyhow!("omp WAL page is outside the committed image"))?
            .copy_from_slice(frame_page);
    }
    let page_count = u32::try_from(commit.page_count)
        .map_err(|_| anyhow::anyhow!("omp WAL commit page count is invalid"))?;
    database[28..32].copy_from_slice(&page_count.to_be_bytes());
    let change_counter = read_omp_u32(database, 24)?.wrapping_add(1);
    database[24..28].copy_from_slice(&change_counter.to_be_bytes());
    database[92..96].copy_from_slice(&change_counter.to_be_bytes());
    Ok(())
}

fn validate_omp_wal(
    wal: &[u8],
    page_size: usize,
    database_pages: usize,
) -> anyhow::Result<Option<OmpWalCommit>> {
    anyhow::ensure!(
        wal.len() <= MAX_OMP_SOURCE_FILE_BYTES,
        "omp WAL exceeds the source size limit"
    );
    anyhow::ensure!(wal.len() >= 32, "omp WAL header is truncated");
    let magic = read_omp_u32(wal, 0)?;
    anyhow::ensure!(
        matches!(magic, 0x377F_0682 | 0x377F_0683)
            && read_omp_u32(wal, 4)? == 3_007_000
            && usize::try_from(read_omp_u32(wal, 8)?).ok() == Some(page_size),
        "omp WAL header does not match agent.db"
    );
    let checksum_little_endian = magic == 0x377F_0682;
    let header_checksum = omp_wal_checksum(&wal[..24], (0, 0), checksum_little_endian)?;
    anyhow::ensure!(
        header_checksum == (read_omp_u32(wal, 24)?, read_omp_u32(wal, 28)?),
        "omp WAL header checksum is invalid"
    );
    let salt_one = read_omp_u32(wal, 16)?;
    let salt_two = read_omp_u32(wal, 20)?;
    let frame_len = page_size
        .checked_add(24)
        .ok_or_else(|| anyhow::anyhow!("omp WAL frame size overflow"))?;
    let complete_frames = wal.len().saturating_sub(32) / frame_len;
    let maximum_pages = database_pages
        .checked_add(complete_frames)
        .ok_or_else(|| anyhow::anyhow!("omp WAL page count overflow"))?;
    let mut last_commit = None;
    let mut validated_current_frame = false;
    let mut previous_checksum = header_checksum;
    for index in 0..complete_frames {
        let frame_at = 32_usize
            .checked_add(
                index
                    .checked_mul(frame_len)
                    .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?,
            )
            .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?;
        let frame_header_end = frame_at
            .checked_add(8)
            .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?;
        let frame_page_at = frame_at
            .checked_add(24)
            .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?;
        let frame_page_end = frame_page_at
            .checked_add(page_size)
            .ok_or_else(|| anyhow::anyhow!("omp WAL frame offset overflow"))?;
        let checksum_after_header = omp_wal_checksum(
            wal.get(frame_at..frame_header_end)
                .ok_or_else(|| anyhow::anyhow!("omp WAL frame header is truncated"))?,
            previous_checksum,
            checksum_little_endian,
        )?;
        let computed_checksum = omp_wal_checksum(
            wal.get(frame_page_at..frame_page_end)
                .ok_or_else(|| anyhow::anyhow!("omp WAL frame page is truncated"))?,
            checksum_after_header,
            checksum_little_endian,
        )?;
        let stored_checksum = (
            read_omp_u32(wal, frame_at + 16)?,
            read_omp_u32(wal, frame_at + 20)?,
        );
        let salts_match = read_omp_u32(wal, frame_at + 8)? == salt_one
            && read_omp_u32(wal, frame_at + 12)? == salt_two;
        if !salts_match {
            // SQLite may reuse a checkpointed WAL without truncating it. Old
            // suffix frames have old salts and a checksum chain disconnected
            // from the current header/prefix. If their payload checksum does
            // follow the current chain, the salts themselves were corrupted
            // in a current frame, so fail closed instead of falling back to a
            // prior credential commit.
            anyhow::ensure!(
                computed_checksum != stored_checksum,
                "omp WAL frame salts do not match its header"
            );
            anyhow::ensure!(
                validated_current_frame,
                "omp WAL has an old-generation frame before any valid current-generation frame"
            );
            break;
        }
        let page_number = read_omp_u32(wal, frame_at)?;
        anyhow::ensure!(
            page_number > 0
                && usize::try_from(page_number).is_ok_and(|number| number <= maximum_pages),
            "omp WAL frame page number is invalid"
        );
        anyhow::ensure!(
            computed_checksum == stored_checksum,
            "omp WAL frame checksum is invalid"
        );
        previous_checksum = computed_checksum;
        validated_current_frame = true;
        let committed_pages = read_omp_u32(wal, frame_at + 4)?;
        if committed_pages > 0 {
            let committed_pages = usize::try_from(committed_pages)
                .map_err(|_| anyhow::anyhow!("omp WAL commit page count is invalid"))?;
            anyhow::ensure!(
                committed_pages > 0 && committed_pages <= maximum_pages,
                "omp WAL commit page count is invalid"
            );
            last_commit = Some((index, committed_pages));
        }
    }
    let Some((last_frame_index, page_count)) = last_commit else {
        return Ok(None);
    };
    Ok(Some(OmpWalCommit {
        frame_len,
        last_frame_index,
        page_count,
    }))
}

fn omp_wal_checksum(
    bytes: &[u8],
    mut checksum: (u32, u32),
    little_endian: bool,
) -> anyhow::Result<(u32, u32)> {
    anyhow::ensure!(
        bytes.len().is_multiple_of(8),
        "omp WAL checksum input has an invalid length"
    );
    for words in bytes.as_chunks::<8>().0 {
        let first: [u8; 4] = words[..4]
            .try_into()
            .map_err(|_| anyhow::anyhow!("omp WAL checksum word is malformed"))?;
        let second: [u8; 4] = words[4..]
            .try_into()
            .map_err(|_| anyhow::anyhow!("omp WAL checksum word is malformed"))?;
        let (first, second) = if little_endian {
            (u32::from_le_bytes(first), u32::from_le_bytes(second))
        } else {
            (u32::from_be_bytes(first), u32::from_be_bytes(second))
        };
        checksum.0 = checksum.0.wrapping_add(first).wrapping_add(checksum.1);
        checksum.1 = checksum.1.wrapping_add(second).wrapping_add(checksum.0);
    }
    Ok(checksum)
}

fn read_omp_u32(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| anyhow::anyhow!("omp SQLite offset overflow"))?;
    let value: [u8; 4] = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow::anyhow!("omp SQLite image is truncated"))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("omp SQLite integer is malformed"))?;
    Ok(u32::from_be_bytes(value))
}

fn validate_omp_store_content(
    content: &[u8],
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> anyhow::Result<()> {
    let snapshot = tempfile::tempdir().context("creating OMP source snapshot")?;
    std::fs::create_dir_all(snapshot.path().join("agent"))?;
    std::fs::write(snapshot.path().join("agent/agent.db"), content)?;
    validate_store_source_dir(
        Agent::Omp,
        provider,
        selector,
        snapshot.path(),
        snapshot.path(),
    )
    .map_err(anyhow::Error::from)
}

/// Bounded fallback for platforms without descriptor-relative Unix traversal.
#[cfg(not(unix))]
fn capture_omp_database_snapshot_from_paths(source_dir: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    for _ in 0..OMP_SOURCE_CAPTURE_ATTEMPTS {
        let Some(first) = read_omp_source_pair_from_paths(source_dir)? else {
            return Ok(None);
        };
        let Some(second) = read_omp_source_pair_from_paths(source_dir)? else {
            continue;
        };
        if first == second {
            return materialize_omp_database(&first.database, first.wal.as_deref()).map(Some);
        }
    }
    anyhow::bail!(
        "OMP source changed during {} bounded credential snapshot attempts",
        OMP_SOURCE_CAPTURE_ATTEMPTS
    )
}

#[cfg(not(unix))]
fn read_omp_source_pair_from_paths(source_dir: &Path) -> anyhow::Result<Option<OmpSourcePair>> {
    let Some(database) = read_source_bytes(&source_dir.join("agent/agent.db"), "omp agent.db")?
    else {
        return Ok(None);
    };
    anyhow::ensure!(
        database.len() <= MAX_OMP_SOURCE_FILE_BYTES,
        "omp agent.db exceeds its source size limit"
    );
    let wal = read_source_bytes(&source_dir.join("agent/agent.db-wal"), "omp agent.db-wal")?;
    if let Some(wal) = wal.as_ref() {
        anyhow::ensure!(
            wal.len() <= MAX_OMP_SOURCE_FILE_BYTES,
            "omp WAL exceeds its source size limit"
        );
    }
    Ok(Some(OmpSourcePair { database, wal }))
}

fn read_source_text(path: &Path, label: &str) -> anyhow::Result<Option<String>> {
    let Some(bytes) = read_source_bytes(path, label)? else {
        return Ok(None);
    };
    let text = String::from_utf8(bytes)
        .with_context(|| format!("{label} is not valid UTF-8: {}", path.display()))?;
    Ok(Some(text))
}

fn private_file_exists(path: &Path) -> anyhow::Result<bool> {
    auth_directory::mount_file_present(path)
}

/// Byte-oriented twin of [`provision_single_file_credential`] for binary
/// single-file stores (omp's `SQLite` `agent.db`). Same outcome/mount
/// contract; an empty host file counts as host-missing.
fn provision_single_blob_credential(
    target: &Path,
    host_path: &Path,
    mode: AuthForwardMode,
    label: &str,
    agent_name: &str,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    let content = if mode == AuthForwardMode::Sync {
        read_source_bytes(host_path, &format!("{agent_name} {label}"))?
    } else {
        None
    };
    provision_single_blob_credential_from_content(target, mode, content, label, agent_name)
}

fn provision_single_blob_credential_from_content(
    target: &Path,
    mode: AuthForwardMode,
    content: Option<Vec<u8>>,
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
                write_private_bytes(target, &content).with_context(|| {
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
fn provision_single_file_credential(
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
fn provision_single_file_credential_with_content(
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
fn wipe_agent_file_state(path: &Path, label: &str) -> anyhow::Result<()> {
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
fn wipe_kimi_state(kimi_dir: &Path) -> anyhow::Result<()> {
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

/// Copy the host's `.claude.json` into the container state, or write `{}`
/// if the host file doesn't exist.
fn copy_host_claude_json(host_path: &Path, dest_path: &Path) -> anyhow::Result<()> {
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
fn wipe_claude_state(account_json: &Path, credentials_json: &Path) -> anyhow::Result<()> {
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
fn read_host_credentials(host_home: &Path) -> anyhow::Result<Option<String>> {
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
fn locked_claude_credentials(
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
fn read_host_credentials_from_claude_config_dir(
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
fn read_claude_keychain(service: &str) -> anyhow::Result<Option<String>> {
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

/// Reject symlinks at `path` to prevent a compromised role from
/// redirecting host-side writes to arbitrary files.
///
/// The role's `.claude/` directory is mounted read-write into the
/// container, so an role could replace `.credentials.json` with a
/// symlink.  Without this check, the next `write_private_file` or
/// `repair_permissions` call would follow the symlink and overwrite
/// or chmod the target on the host.
fn reject_symlink(path: &Path) -> anyhow::Result<()> {
    // Use symlink_metadata (lstat) — regular metadata() follows symlinks.
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        anyhow::ensure!(
            !meta.file_type().is_symlink(),
            "refusing to write through symlink at {}; \
             this may indicate a compromised role state — \
             remove the symlink and retry",
            path.display()
        );
    }
    Ok(())
}

/// Reject symlink traversal through the destination's existing parent
/// directories as well as at the final path. Missing ancestors are allowed so
/// callers can create a new private tree after this check.
fn reject_auth_path(path: &Path) -> anyhow::Result<()> {
    reject_symlink(path)?;
    let mut ancestor = path.parent();
    while let Some(current) = ancestor {
        if !is_platform_root_alias(current) {
            match std::fs::symlink_metadata(current) {
                Ok(meta) => {
                    anyhow::ensure!(
                        !meta.file_type().is_symlink(),
                        "refusing to use auth path through symlink at {}; remove the symlink and retry",
                        current.display()
                    );
                    anyhow::ensure!(
                        meta.is_dir(),
                        "refusing to use auth path through non-directory {}; remove it and retry",
                        current.display()
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        ancestor = current.parent();
    }
    Ok(())
}

/// macOS exposes these root directories as immutable platform aliases (for
/// example `/var` → `/private/var`). They are outside Jackin-owned state and
/// must not make every otherwise-safe temporary test or data path fail.
#[cfg(target_os = "macos")]
fn is_platform_root_alias(path: &Path) -> bool {
    matches!(path, p if p == Path::new("/etc")
        || p == Path::new("/tmp")
        || p == Path::new("/var"))
}

#[cfg(not(target_os = "macos"))]
const fn is_platform_root_alias(_path: &Path) -> bool {
    false
}

/// Write a file with restricted permissions (`0o600` on Unix) since it
/// may contain authentication credentials.
///
/// Rejects symlinks to prevent a compromised role from redirecting
/// writes to arbitrary host paths.  Uses `tempfile::NamedTempFile` to
/// create an unpredictable temp file (opened with `O_EXCL`, so a
/// pre-planted symlink at the temp path is impossible), then renames
/// it to the destination — closing the TOCTOU window entirely.
fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    write_private_bytes(path, content.as_bytes())
}

/// Write raw bytes to `path` with `0o600` permissions, symlink-safe and atomic.
fn write_private_bytes(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::replace_private_file(path, content)
    }

    #[cfg(not(unix))]
    {
        reject_auth_path(path)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Create `path` with `content` at `0o600` only when it does not yet exist.
///
/// Race-free via `O_CREAT|O_EXCL`; on `EEXIST` (file already present)
/// the function returns `Ok(())` and leaves the existing content
/// untouched. Use when a process-private skeleton must be seeded
/// before a downstream consumer (e.g. the Claude CLI) may persist
/// real state into the same path.
pub(super) fn create_private_file_if_absent(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::create_private_file_if_absent(path, content)
    }

    #[cfg(not(unix))]
    {
        use anyhow::Context;
        reject_auth_path(path)?;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "auth file provisioning is called from spawn_blocking during launch"
        )]
        match opts.open(path) {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(content)
                    .with_context(|| format!("writing private skeleton at {}", path.display()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(anyhow::Error::new(error)
                .context(format!("creating private skeleton at {}", path.display()))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermissionRepairFailure {
    Stat,
    Chmod,
    Verify,
}

#[cfg(test)]
thread_local! {
    static PERMISSION_REPAIR_FAILURE: std::cell::Cell<Option<PermissionRepairFailure>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
struct PermissionRepairFailureGuard;

#[cfg(test)]
impl Drop for PermissionRepairFailureGuard {
    fn drop(&mut self) {
        PERMISSION_REPAIR_FAILURE.with(|failure| failure.set(None));
    }
}

#[cfg(test)]
fn inject_permission_repair_failure(
    failure: PermissionRepairFailure,
) -> PermissionRepairFailureGuard {
    PERMISSION_REPAIR_FAILURE.with(|injected| injected.set(Some(failure)));
    PermissionRepairFailureGuard
}

fn maybe_inject_permission_repair_failure(stage: PermissionRepairFailure) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        if PERMISSION_REPAIR_FAILURE.with(std::cell::Cell::get) == Some(stage) {
            anyhow::bail!("injected credential permission repair failure at {stage:?}");
        }
    }
    #[cfg(not(test))]
    let _ = stage;
    Ok(())
}

/// Tighten permissions on an existing credential file to `0o600`.
///
/// Missing files are allowed because callers use this helper on optional
/// credentials. Any failure while inspecting, chmod-ing, or verifying an
/// existing path is returned so launch provisioning fails closed rather than
/// continuing with potentially exposed credentials.
fn repair_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::repair_file_permissions(path)
    }

    #[cfg(not(unix))]
    {
        reject_auth_path(path)?;
        let _ = path;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
