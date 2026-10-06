// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Fallback capture, snapshot hashing, and bounded reads without file locks.

use std::path::Path;

use jackin_instance_credentials::read_bounded_local_file;

use jackin_instance_credentials::{
    MAX_AUTH_SOURCE_TREE_BYTES, MAX_AUTH_SOURCE_TREE_ENTRIES, write_private_bytes,
};

#[cfg(not(unix))]
pub fn capture_unixless_source(
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
            let bytes = capture_omp_database_snapshot_from_paths(source_dir, provider, selector)?
                .ok_or_else(|| anyhow::anyhow!("omp agent.db is missing"))?;
            write_snapshot_bytes(snapshot_root, Path::new("agent/agent.db"), bytes.as_slice())?;
            Ok(())
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
pub fn capture_unixless_single_file_source(
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

pub fn write_snapshot_bytes(root: &Path, relative: &Path, bytes: &[u8]) -> anyhow::Result<()> {
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

pub fn snapshot_content_revision(root: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    let mut budget = SnapshotHashBudget::default();
    hash_snapshot_tree(root, Path::new(""), &mut digest, &mut budget)?;
    Ok(hex::encode(digest.finalize()))
}

#[derive(Debug, Default)]
pub struct SnapshotHashBudget {
    bytes: usize,
    entries: usize,
}

pub fn hash_snapshot_tree(
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

#[cfg(not(unix))]
pub fn copy_unixless_source_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let mut budget = SnapshotHashBudget::default();
    copy_unixless_source_tree_inner(source, destination, &mut budget)
}

#[cfg(not(unix))]
pub fn copy_unixless_source_tree_inner(
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
