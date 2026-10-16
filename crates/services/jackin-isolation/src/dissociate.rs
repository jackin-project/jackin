// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Post-clone dissociation for workflow worker clones.
//!
//! A bare `git clone --local` hardlinks object files to the source
//! repository and records an `objects/info/alternates` pointer. A worker
//! with write access to such a clone could corrupt the host repository
//! through the shared inodes, and the alternates file keeps the clone
//! readable only through host-owned paths. Workflow clones must be fully
//! independent: no alternates, no hardlinked objects.
//!
//! Git worktrees are the opposite shape (they intentionally share `.git`
//! with the host repository) and must NOT be used for workflow workers.

use anyhow::Context;
use jackin_core::CommandRunner;
use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Remove the alternates pointer and rewrite borrowed objects so the clone
/// owns every byte. Afterwards [`verify_no_hardlinked_objects`] must pass.
pub async fn dissociate_clone(
    runner: &mut impl CommandRunner,
    clone_path: &Path,
) -> anyhow::Result<()> {
    let clone = clone_path.to_string_lossy().into_owned();
    let alternates = clone_path
        .join(".git")
        .join("objects")
        .join("info")
        .join("alternates");
    match std::fs::remove_file(&alternates) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e)
                .with_context(|| format!("remove clone alternates at {}", alternates.display()));
        }
    }
    // Rewrite every object into fresh inodes. `-l` keeps the repack local to
    // this repository so no other object store is consulted.
    runner
        .run(
            "git",
            &["-C", &clone, "repack", "-a", "-d", "-l"],
            None,
            &jackin_core::RunOptions::default(),
        )
        .await
        .with_context(|| format!("dissociate workflow clone at {clone} via git repack"))?;
    // Repack leaves freshly-packed loose objects packed, but any surviving
    // loose hardlink is rewritten below so the clone owns its bytes.
    break_remaining_hardlinks(clone_path)
        .with_context(|| format!("break hardlinked objects in workflow clone at {clone}"))?;
    Ok(())
}

fn object_identity(path: &Path) -> anyhow::Result<(u64, u64)> {
    let meta =
        std::fs::metadata(path).with_context(|| format!("stat object at {}", path.display()))?;
    if !meta.is_file() {
        anyhow::bail!("non-file object at {}", path.display());
    }
    Ok((meta.dev(), meta.ino()))
}

fn collect_object_identities(git_dir: &Path) -> anyhow::Result<HashSet<(u64, u64)>> {
    let objects = git_dir.join("objects");
    let mut out = HashSet::new();
    let mut stack = vec![objects];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(e).with_context(|| format!("list objects at {}", dir.display()));
            }
        };
        for entry in entries {
            let entry =
                entry.with_context(|| format!("read objects entry at {}", dir.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("inspect objects entry at {}", path.display()))?;
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if file_type.is_file() {
                out.insert(object_identity(&path)?);
            }
        }
    }
    Ok(out)
}

/// Rewrite any clone object file that still shares an inode with the host
/// object store, so the clone owns every byte it can write.
fn break_remaining_hardlinks(clone_path: &Path) -> anyhow::Result<usize> {
    let clone_git = clone_path.join(".git");
    let objects = clone_git.join("objects");
    let mut stack = vec![objects];
    let mut rewritten = 0usize;
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(e).with_context(|| format!("list clone objects at {}", dir.display()));
            }
        };
        for entry in entries {
            let entry =
                entry.with_context(|| format!("read clone objects entry at {}", dir.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("inspect clone objects entry at {}", path.display()))?;
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let meta = std::fs::metadata(&path)
                .with_context(|| format!("stat clone object at {}", path.display()))?;
            if meta.nlink() > 1 {
                let bytes = std::fs::read(&path)
                    .with_context(|| format!("read clone object at {}", path.display()))?;
                let tmp = path.with_extension("jackin-dissociate-tmp");
                std::fs::write(&tmp, &bytes)
                    .with_context(|| format!("stage dissociated object at {}", tmp.display()))?;
                std::fs::rename(&tmp, &path)
                    .with_context(|| format!("replace hardlinked object at {}", path.display()))?;
                rewritten += 1;
            }
        }
    }
    Ok(rewritten)
}

/// Verify the clone shares no object inode with the host repository and
/// records no alternates pointer.
pub fn verify_no_hardlinked_objects(host_repo: &Path, clone_path: &Path) -> anyhow::Result<()> {
    let alternates = clone_path
        .join(".git")
        .join("objects")
        .join("info")
        .join("alternates");
    anyhow::ensure!(
        !alternates.exists(),
        "workflow clone at {} still records an alternates pointer to the host repository",
        clone_path.display(),
    );
    let host_git = host_repo.join(".git");
    let clone_git = clone_path.join(".git");
    let host_objects = collect_object_identities(&host_git)?;
    let clone_objects = collect_object_identities(&clone_git)?;
    let shared: Vec<(u64, u64)> = host_objects.intersection(&clone_objects).copied().collect();
    anyhow::ensure!(
        shared.is_empty(),
        "workflow clone at {} shares {} object inode(s) with host repository at {}",
        clone_path.display(),
        shared.len(),
        host_repo.display(),
    );
    // Belt and suspenders: no pack may reference an alternate store either.
    let info_dir = clone_git.join("objects").join("info");
    if let Ok(entries) = std::fs::read_dir(&info_dir) {
        let mut names: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        names.sort();
        for path in &names {
            if path.file_name().is_some_and(|n| n == "alternates") {
                anyhow::bail!(
                    "workflow clone at {} still records an alternates pointer",
                    clone_path.display(),
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
