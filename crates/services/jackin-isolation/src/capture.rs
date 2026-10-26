// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use jackin_core::{CommandRunner, ContainerHandle, ContainerState, DockerApi};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub run_id: String,
    pub round: u64,
    pub generation: u64,
    pub commit_id: String,
    pub tree_id: String,
    pub source_digest: String,
    pub parent: Option<String>,
    pub manifest_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceManifestEntry {
    pub path: String,
    pub kind: String,
    pub mode: u32,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceManifest {
    pub entries: Vec<SourceManifestEntry>,
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceCompare {
    Match,
    Changed,
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub run_id: String,
    pub round: u64,
    pub generation: u64,
    pub parent: Option<String>,

    pub expected_container_id: String,

    pub copy_size_limit_bytes: u64,

    pub deadline_unix_secs: Option<u64>,
}

fn hex_digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn file_digest(path: &Path) -> anyhow::Result<String> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read source file at {}", path.display()))?;
    Ok(hex_digest(&bytes))
}

struct TreeWalk {
    entries: Vec<SourceManifestEntry>,
    total_bytes: u64,
}

fn walk_source_tree(
    root: &Path,
    exclusions: &[String],
    size_limit_bytes: u64,
) -> anyhow::Result<TreeWalk> {
    let mut walk = TreeWalk {
        entries: Vec::new(),
        total_bytes: 0,
    };
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let abs = root.join(&rel);
        let mut names: Vec<std::ffi::OsString> = std::fs::read_dir(&abs)
            .with_context(|| format!("list source at {}", abs.display()))?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<_, _>>()
            .with_context(|| format!("list source at {}", abs.display()))?;
        names.sort();
        for name in names {
            let rel_path = rel.join(&name);
            let display = rel_path.to_string_lossy().replace('\\', "/");
            if rel.components().count() == 0 && name == ".git" {
                continue;
            }
            if exclusions.iter().any(|x| x == &display) {
                continue;
            }
            let abs_path = root.join(&rel_path);
            let meta = std::fs::symlink_metadata(&abs_path)
                .with_context(|| format!("inspect source entry at {}", abs_path.display()))?;
            let file_type = meta.file_type();
            if file_type.is_symlink() {
                let target = std::fs::read_link(&abs_path)
                    .with_context(|| format!("read symlink at {}", abs_path.display()))?;
                let resolved = if target.is_absolute() {
                    target.clone()
                } else {
                    abs_path.parent().unwrap_or(root).join(&target)
                };
                anyhow::ensure!(
                    resolved.starts_with(root),
                    "refusing capture of symlink {} pointing outside the source root",
                    abs_path.display(),
                );
                walk.entries.push(SourceManifestEntry {
                    path: display,
                    kind: "symlink".to_owned(),
                    mode: 0o777,
                    digest: hex_digest(target.to_string_lossy().as_bytes()),
                });
                continue;
            }
            if file_type.is_dir() {
                anyhow::ensure!(
                    !display.contains(".."),
                    "refusing capture of traversal path {display:?}"
                );
                stack.push(rel_path);
                continue;
            }
            if !file_type.is_file() {
                let kind = if file_type.is_fifo() {
                    "fifo"
                } else if file_type.is_socket() {
                    "socket"
                } else if file_type.is_block_device() {
                    "block-device"
                } else if file_type.is_char_device() {
                    "char-device"
                } else {
                    "special"
                };
                anyhow::bail!(
                    "refusing capture of unsupported file type {kind} at {}",
                    abs_path.display()
                );
            }
            let len = meta.len();
            walk.total_bytes = walk.total_bytes.saturating_add(len);
            let total_bytes = walk.total_bytes;
            anyhow::ensure!(
                walk.total_bytes <= size_limit_bytes,
                "refusing capture: source exceeds the configured copy-size limit \
                 ({total_bytes} > {size_limit_bytes} bytes)",
            );
            let executable = meta.mode() & 0o111 != 0;
            walk.entries.push(SourceManifestEntry {
                path: display,
                kind: "file".to_owned(),
                mode: if executable { 0o755 } else { 0o644 },
                digest: file_digest(&abs_path)?,
            });
        }
    }
    walk.entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(walk)
}

pub fn digest_source_tree(
    root: &Path,
    exclusions: &[String],
    size_limit_bytes: u64,
) -> anyhow::Result<(String, SourceManifest)> {
    let walk = walk_source_tree(root, exclusions, size_limit_bytes)?;
    let mut hasher = Sha256::new();
    for entry in &walk.entries {
        hasher.update(entry.path.as_bytes());
        hasher.update([0]);
        hasher.update(entry.kind.as_bytes());
        hasher.update([0]);
        hasher.update(entry.mode.to_be_bytes());
        hasher.update([0]);
        hasher.update(entry.digest.as_bytes());
        hasher.update([0]);
    }
    let manifest = SourceManifest {
        entries: walk.entries,
        exclusions: exclusions.to_vec(),
    };
    Ok((hex::encode(hasher.finalize()), manifest))
}

fn copy_entries(src_root: &Path, staging: &Path, manifest: &SourceManifest) -> anyhow::Result<()> {
    let mut by_path: BTreeMap<&str, &SourceManifestEntry> = BTreeMap::new();
    for entry in &manifest.entries {
        by_path.insert(entry.path.as_str(), entry);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in &manifest.entries {
        let dest = staging.join(&entry.path);
        if let Some(parent) = dest.parent()
            && !dirs.iter().any(|d| *d == parent)
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create staging dir at {}", parent.display()))?;
            dirs.push(parent.to_path_buf());
        }
        match entry.kind.as_str() {
            "file" => {
                let bytes = std::fs::read(src_root.join(&entry.path)).with_context(|| {
                    format!("read source file for staging at {}", dest.display())
                })?;
                std::fs::write(&dest, &bytes)
                    .with_context(|| format!("write staging file at {}", dest.display()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(entry.mode))
                        .with_context(|| format!("set staging mode at {}", dest.display()))?;
                }
            }
            "symlink" => {
                #[cfg(unix)]
                {
                    let link_target = std::fs::read_link(src_root.join(&entry.path))
                        .with_context(|| format!("read source symlink at {}", dest.display()))?;
                    std::os::unix::fs::symlink(&link_target, &dest)
                        .with_context(|| format!("recreate symlink at {}", dest.display()))?;
                }
            }
            other => {
                anyhow::bail!("unsupported manifest entry kind {other:?} during staging copy");
            }
        }
    }
    Ok(())
}

pub fn capture_stable_copy(
    src_root: &Path,
    staging: &Path,
    exclusions: &[String],
    size_limit_bytes: u64,
) -> anyhow::Result<(String, SourceManifest)> {
    anyhow::ensure!(
        !staging.exists(),
        "staging directory at {} must be empty before capture; refusing to mix copies",
        staging.display(),
    );
    let (before, manifest) = digest_source_tree(src_root, exclusions, size_limit_bytes)?;
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create staging parent at {}", parent.display()))?;
    }
    std::fs::create_dir(staging)
        .with_context(|| format!("create staging dir at {}", staging.display()))?;
    let copy_result = copy_entries(src_root, staging, &manifest);
    let (after, _) = digest_source_tree(src_root, exclusions, size_limit_bytes)?;
    if copy_result.is_err() || before != after {
        drop(std::fs::remove_dir_all(staging));
        if let Err(e) = copy_result {
            return Err(e).with_context(|| {
                format!(
                    "capture copy from {} failed; incomplete staging copy discarded",
                    src_root.display()
                )
            });
        }
        anyhow::bail!(
            "source at {} changed during capture copy (write barrier not proven); \
             incomplete staging copy discarded",
            src_root.display()
        );
    }
    Ok((after, manifest))
}

fn checkpoint_git_opts() -> jackin_core::RunOptions {
    jackin_core::RunOptions {
        quiet: true,
        extra_env: vec![
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("GIT_AUTHOR_NAME".into(), "jackin-workflows".into()),
            (
                "GIT_AUTHOR_EMAIL".into(),
                "jackin-workflows@localhost".into(),
            ),
            ("GIT_COMMITTER_NAME".into(), "jackin-workflows".into()),
            (
                "GIT_COMMITTER_EMAIL".into(),
                "jackin-workflows@localhost".into(),
            ),
        ],
        ..Default::default()
    }
}

pub async fn commit_capture_tree(
    runner: &mut impl CommandRunner,
    checkpoint_repo: &Path,
    staging: &Path,
    message: &str,
) -> anyhow::Result<(String, String)> {
    let repo = checkpoint_repo.to_string_lossy().into_owned();
    let work = staging.to_string_lossy().into_owned();
    let opts = checkpoint_git_opts();
    let host_cfg = [
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.attributesFile=/dev/null",
        "-c",
        "core.autocrlf=false",
        "-c",
        "commit.gpgsign=false",
    ];
    if !checkpoint_repo.join("HEAD").exists() {
        let mut init_args: Vec<&str> = vec![host_cfg[0], host_cfg[1]];
        init_args.extend(["--git-dir", &repo, "init", "-q"]);
        runner
            .run("git", &init_args, None, &opts)
            .await
            .with_context(|| format!("init host-owned checkpoint repository at {repo}"))?;
    }
    let mut add_args: Vec<&str> = vec![host_cfg[0], host_cfg[1], host_cfg[2], host_cfg[3]];
    add_args.extend(["--git-dir", &repo, "--work-tree", &work, "add", "-A"]);
    runner
        .run("git", &add_args, None, &opts)
        .await
        .with_context(|| format!("stage capture bytes from {work}"))?;
    let mut commit_args: Vec<&str> = vec![
        host_cfg[0],
        host_cfg[1],
        host_cfg[2],
        host_cfg[3],
        host_cfg[4],
        host_cfg[5],
        host_cfg[6],
        host_cfg[7],
    ];
    commit_args.extend([
        "--git-dir",
        &repo,
        "--work-tree",
        &work,
        "commit",
        "-q",
        "-m",
        message,
    ]);
    runner
        .run("git", &commit_args, None, &opts)
        .await
        .with_context(|| format!("commit capture in {repo}"))?;
    let commit = runner
        .capture_with_options(
            "git",
            &["--git-dir", &repo, "rev-parse", "HEAD"],
            None,
            &opts,
        )
        .await
        .with_context(|| format!("read capture commit in {repo}"))?;
    let tree = runner
        .capture_with_options(
            "git",
            &["--git-dir", &repo, "rev-parse", "HEAD^{tree}"],
            None,
            &opts,
        )
        .await
        .with_context(|| format!("read capture tree in {repo}"))?;
    Ok((commit.trim().to_owned(), tree.trim().to_owned()))
}

pub fn compare_source(
    src_root: &Path,
    manifest: &SourceManifest,
    size_limit_bytes: u64,
) -> SourceCompare {
    match digest_source_tree(src_root, &manifest.exclusions, size_limit_bytes) {
        Ok((_digest, current)) => {
            if current.entries == manifest.entries {
                SourceCompare::Match
            } else {
                SourceCompare::Changed
            }
        }
        Err(e) => SourceCompare::Unknown(format!("{e:#}")),
    }
}

async fn confirm_paused(docker: &impl DockerApi, handle: &ContainerHandle) -> anyhow::Result<()> {
    for _ in 0..8 {
        match docker.inspect_container_by_id(handle).await {
            ContainerState::Paused => return Ok(()),
            ContainerState::InspectUnavailable(reason) => {
                anyhow::bail!("cannot confirm worker pause: {reason}")
            }
            _ => tokio::task::yield_now().await,
        }
    }
    anyhow::bail!("backend did not report the worker paused; refusing capture")
}

pub async fn capture_checkpoint(
    docker: &impl DockerApi,
    handle: &ContainerHandle,
    container_state_dir: &Path,
    src_bind_path: &Path,
    staging_parent: &Path,
    checkpoint_repo: &Path,
    request: &CaptureRequest,
) -> anyhow::Result<Checkpoint> {
    anyhow::ensure!(
        handle.id() == request.expected_container_id,
        "capture refuses container {}: live ID does not match the recorded workflow identity",
        handle.name(),
    );
    let staging = staging_parent.join(format!(
        "capture-{}-{}-{}",
        request.run_id, request.round, request.generation
    ));
    let hold_id = format!(
        "checkpoint:{}:{}:{}",
        request.run_id, request.round, request.generation
    );

    crate::holds::hold_work(
        container_state_dir,
        &hold_id,
        &format!(
            "checkpoint capture for run {} round {} generation {}",
            request.run_id, request.round, request.generation
        ),
        None,
    )?;

    if let Err(e) = docker.pause_container_by_id(handle).await {
        return Err(e)
            .with_context(|| format!("pause worker {} for checkpoint capture", handle.name()));
    }
    let paused_capture = async {
        confirm_paused(docker, handle).await?;

        let (stable_digest, _) =
            digest_source_tree(src_bind_path, &[], request.copy_size_limit_bytes)?;
        let (stable_again, _) =
            digest_source_tree(src_bind_path, &[], request.copy_size_limit_bytes)?;
        anyhow::ensure!(
            stable_digest == stable_again,
            "source at {} is still changing; capture rejected, work preserved",
            src_bind_path.display(),
        );

        let (digest, manifest) =
            capture_stable_copy(src_bind_path, &staging, &[], request.copy_size_limit_bytes)?;

        let message = format!(
            "jackin checkpoint run={} round={} generation={} digest={digest}",
            request.run_id, request.round, request.generation
        );
        let mut host_runner = HostGitRunner;
        let (commit_id, tree_id) =
            commit_capture_tree(&mut host_runner, checkpoint_repo, &staging, &message).await?;
        Ok::<_, anyhow::Error>((digest, manifest, commit_id, tree_id))
    }
    .await;

    let resume_result = docker.unpause_container_by_id(handle).await;
    let (digest, manifest, commit_id, tree_id) = match (paused_capture, resume_result) {
        (Ok(done), Ok(())) => done,
        (Err(capture_err), Ok(())) => {
            return Err(capture_err);
        }
        (Ok(_), Err(resume_err)) => {
            return Err(resume_err).with_context(|| {
                format!(
                    "captured checkpoint but failed to resume worker {}; \
                     source and local checkpoint are preserved; recover the pending resume",
                    handle.name()
                )
            });
        }
        (Err(capture_err), Err(resume_err)) => {
            return Err(capture_err).context(format!(
                "capture failed and resume also failed ({resume_err:#}); \
                 source is preserved; recover the pending resume"
            ));
        }
    };

    let state = docker.inspect_container_by_id(handle).await;
    anyhow::ensure!(
        matches!(state, ContainerState::Running),
        "worker {} did not resume to running after capture (state: {}); \
         checkpoint is saved locally",
        handle.name(),
        state.short_label(),
    );

    if let Some(deadline) = request.deadline_unix_secs {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(u64::MAX, |d| d.as_secs());
        anyhow::ensure!(
            now <= deadline,
            "worker deadline expired during capture; agent must stop, \
             source and checkpoint are preserved"
        );
    }

    let manifest_path = checkpoint_repo
        .join("manifests")
        .join(format!("{commit_id}.json"));
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create checkpoint manifest dir at {}", parent.display()))?;
    }
    let manifest_body = serde_json::to_vec_pretty(&manifest)?;
    std::fs::write(&manifest_path, &manifest_body)
        .with_context(|| format!("save checkpoint manifest at {}", manifest_path.display()))?;
    drop(std::fs::remove_dir_all(&staging));
    Ok(Checkpoint {
        run_id: request.run_id.clone(),
        round: request.round,
        generation: request.generation,
        commit_id,
        tree_id,
        source_digest: digest,
        parent: request.parent.clone(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
    })
}

struct HostGitRunner;

impl CommandRunner for HostGitRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &jackin_core::RunOptions,
    ) -> anyhow::Result<()> {
        let mut runner = jackin_docker::shell_runner::ShellRunner::default();
        runner.run(program, args, cwd, opts).await
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        let mut runner = jackin_docker::shell_runner::ShellRunner::default();
        runner.capture(program, args, cwd).await
    }

    async fn capture_with_options(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &jackin_core::RunOptions,
    ) -> anyhow::Result<String> {
        let mut runner = jackin_docker::shell_runner::ShellRunner::default();
        runner.capture_with_options(program, args, cwd, opts).await
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        let mut runner = jackin_docker::shell_runner::ShellRunner::default();
        runner.capture_secret(program, args, cwd).await
    }
}

#[cfg(test)]
mod tests;
