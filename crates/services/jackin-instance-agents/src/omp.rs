// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! OMP snapshot credential provisioning.

use jackin_instance_credentials::AuthProvisionOutcome;

use anyhow::Context;
use jackin_config::{
    AiProvider, AuthForwardMode, MAX_STANDALONE_DATABASE_BYTES, OmpSelectedAccount, OmpSelector,
    OmpSnapshot, ProfileSelector,
};

use std::path::{Path, PathBuf};

use crate::{provision_single_blob_credential, provision_single_blob_credential_from_content};
use jackin_instance_credentials::auth_directory;
use zeroize::Zeroizing;

/// Provision omp's host-side `~/.omp/agent/agent.db` (`SQLite`) per the
/// chosen mode. Byte-oriented twin of `provision_grok_auth`: the store
/// is binary, so the UTF-8 provisioner cannot be used.
pub fn provision_omp_auth(
    agent_db: &Path,
    mode: AuthForwardMode,
    host_home: &Path,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_omp_auth_from_source_dir(agent_db, mode, &host_home.join(".omp"), provider, selector)
}

pub fn provision_omp_auth_from_source_dir(
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
                let content = capture_omp_database_snapshot(&source.root, provider, selector)?;
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
        let content = capture_omp_database_snapshot_from_paths(source_dir, provider, selector)?;
        return provision_single_blob_credential_from_content(
            agent_db,
            mode,
            content,
            "omp agent.db",
            "omp",
        );
    }
    provision_omp_auth_from_path(agent_db, mode, &source_dir.join("agent/agent.db"))
}

fn provision_omp_auth_from_path(
    agent_db: &Path,
    mode: AuthForwardMode,
    host_agent_db: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_single_blob_credential(agent_db, host_agent_db, mode, "omp agent.db", "omp")
}

pub fn read_source_bytes(path: &Path, label: &str) -> anyhow::Result<Option<Vec<u8>>> {
    auth_directory::read_source_path(path, label)
}

#[cfg(unix)]
pub fn capture_omp_database_snapshot(
    source: &std::fs::File,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let Some(mut snapshot) = OmpSnapshot::capture_from_root(source)? else {
        anyhow::bail!("OMP credential source is unavailable");
    };
    capture_omp_snapshot_bytes(&mut snapshot, provider, selector)
}

#[cfg(unix)]
pub fn validate_omp_source_selection(
    source: &std::fs::File,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> anyhow::Result<()> {
    let Some(mut snapshot) = OmpSnapshot::capture_from_root(source)? else {
        anyhow::bail!("OMP credential source is unavailable");
    };
    drop(select_omp_snapshot_account(
        &mut snapshot,
        provider,
        selector,
    )?);
    Ok(())
}

pub(crate) fn select_omp_snapshot_account<'a>(
    snapshot: &'a mut OmpSnapshot,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> anyhow::Result<OmpSelectedAccount<'a>> {
    // The current launch contract copies the whole OMP database. Until a
    // selected-row-only SQLite image is supported, more than one usable row
    // would expose a sibling account to the role and is therefore unavailable.
    if snapshot.accounts().len() != 1 {
        anyhow::bail!("OMP account selection is missing or ambiguous");
    }
    let selector = selector.map(|selector| OmpSelector {
        entry: selector.entry.clone(),
        profile: selector.profile.clone(),
    });
    snapshot
        .select(provider.map(AiProvider::slug), selector.as_ref())
        .map_err(anyhow::Error::new)
}

pub(crate) fn capture_omp_snapshot_bytes(
    snapshot: &mut OmpSnapshot,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let selected = select_omp_snapshot_account(snapshot, provider, selector)?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(MAX_STANDALONE_DATABASE_BYTES));
    selected
        .write_standalone_database(&mut *bytes)
        .map_err(anyhow::Error::new)?;
    Ok(bytes)
}

/// Bounded fallback for platforms without descriptor-relative Unix traversal.
#[cfg(not(unix))]
pub fn capture_omp_database_snapshot_from_paths(
    source_dir: &Path,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> anyhow::Result<Option<Zeroizing<Vec<u8>>>> {
    let Some(mut snapshot) = OmpSnapshot::capture_from_directory(source_dir)? else {
        return Ok(None);
    };
    capture_omp_snapshot_bytes(&mut snapshot, provider, selector).map(Some)
}

pub fn read_source_text(path: &Path, label: &str) -> anyhow::Result<Option<String>> {
    let Some(bytes) = read_source_bytes(path, label)? else {
        return Ok(None);
    };
    let text = String::from_utf8(bytes)
        .with_context(|| format!("{label} is not valid UTF-8: {}", path.display()))?;
    Ok(Some(text))
}

pub fn private_file_exists(path: &Path) -> anyhow::Result<bool> {
    auth_directory::mount_file_present(path)
}
