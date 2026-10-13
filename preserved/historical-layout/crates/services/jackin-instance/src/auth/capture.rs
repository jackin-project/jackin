// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Locked-source capture for descriptor-selected credential sources.

use anyhow::Context;
use jackin_config::{AiProvider, ProfileSelector};
use jackin_core::Agent;
use std::path::Path;

use crate::auth::{
    SelectedSourceDirectory, auth_directory, capture_omp_database_snapshot,
    claude_source_missing_error, host_home_is_real, read_claude_keychain,
    select_opencode_auth_entry, validate_kimi_locked_source, validate_store_source_dir,
    write_snapshot_bytes,
};

pub(crate) fn create_source_snapshot_dir(parent: &Path) -> anyhow::Result<SelectedSourceDirectory> {
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
pub(crate) fn capture_locked_source(
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
            let content = capture_omp_database_snapshot(&source.root, provider, selector)?;
            write_snapshot_bytes(
                snapshot_root,
                Path::new("agent/agent.db"),
                content.as_slice(),
            )?;
            Ok(())
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
pub(crate) fn capture_locked_claude_source(
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
pub(crate) fn capture_locked_single_file_source(
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
pub(crate) fn capture_locked_opencode_source(
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
