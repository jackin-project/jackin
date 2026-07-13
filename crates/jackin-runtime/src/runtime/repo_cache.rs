#![allow(
    clippy::too_many_lines,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
//! Role-repo resolution: clone or update from git, validate, cache under `~/.jackin/roles/`.
//!
//! Typed errors (`RepoError`) allow callers to downcast and produce
//! operator-friendly messages without substring-matching free text. Not
//! responsible for Dockerfile validation or manifest parsing — those are
//! handled in `repo.rs` and `repo_contract.rs` after the cache is warm.

use crate::instance::runtime_slug;
use anyhow::Context;
use fs4::FileExt;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_core::{CommandRunner, RunOptions};
use jackin_manifest::repo::{CachedRepo, validate_role_repo};
#[cfg(test)]
use std::io::IsTerminal;
use std::time::{Duration, SystemTime};

use super::identity::try_capture;

/// Typed errors raised by role-repo resolution.
///
/// Surfaced through `anyhow::Error` chains so the editor can downcast and
/// pick a friendly translation without substring-matching free text. Add
/// new variants here together with their `friendly_role_resolution_error`
/// arm in `console::tui::input::editor`.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    /// `git clone` failed for any reason — host unreachable, auth required,
    /// repo missing, server-side error. The original anyhow chain is kept
    /// as the `#[source]` so `--debug` still surfaces it.
    #[error("repository is not available or cannot be accessed")]
    CloneFailed(#[source] anyhow::Error),

    /// Cached repo's `origin` remote points at a different URL than the
    /// configured source and the operator declined removal.
    #[error("cached role repo remote mismatch — aborting")]
    RemoteMismatch,

    /// `validate_role_repo` rejected the cloned repo. The inner typed
    /// error carries the structural reason; `friendly_role_resolution_error`
    /// matches on its variants for richer messages.
    #[error("invalid role repo: {0}")]
    InvalidRoleRepo(#[from] jackin_manifest::repo::RoleRepoValidationError),
}

/// Extract `owner/repo` from a git remote URL.
pub(super) fn parse_repo_name(url: &str) -> Option<String> {
    let url = url.trim();
    let stripped = url.strip_suffix(".git").unwrap_or(url);
    // HTTPS: https://github.com/owner/repo
    if let Some(rest) = stripped
        .strip_prefix("https://")
        .or_else(|| stripped.strip_prefix("http://"))
    {
        return rest.find('/').map(|i| rest[i + 1..].to_string());
    }
    // SSH: git@github.com:owner/repo
    stripped.rsplit_once(':').map(|(_, p)| p.to_owned())
}

pub(super) fn repo_matches(expected: &str, actual: &str) -> bool {
    match (parse_repo_name(expected), parse_repo_name(actual)) {
        (Some(expected_repo), Some(actual_repo)) => expected_repo == actual_repo,
        _ => expected.trim() == actual.trim(),
    }
}

fn role_cache_root(paths: &JackinPaths, selector: &RoleSelector) -> std::path::PathBuf {
    selector.namespace.as_ref().map_or_else(
        || paths.roles_dir.join(&selector.name),
        |namespace| paths.roles_dir.join(namespace).join(&selector.name),
    )
}

fn migrate_legacy_default_cache(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
) -> anyhow::Result<()> {
    let root = role_cache_root(paths, selector);
    if cached_repo.repo_dir != root.join("default")
        || cached_repo.repo_dir.exists()
        || !root.join(".git").is_dir()
    {
        return Ok(());
    }

    let parent = root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("role cache path has no parent: {}", root.display()))?;
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("role");
    let suffix = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let legacy_root = parent.join(format!(".{root_name}.legacy-cache-{suffix}"));

    std::fs::rename(&root, &legacy_root).with_context(|| {
        format!(
            "failed to move legacy role cache {} before migrating to default/",
            root.display()
        )
    })?;
    std::fs::create_dir_all(&root)?;

    let legacy_branches = legacy_root.join("branches");
    if legacy_branches.exists() {
        std::fs::rename(&legacy_branches, root.join("branches")).with_context(|| {
            format!(
                "failed to move legacy branch cache {}",
                legacy_branches.display()
            )
        })?;
    }

    std::fs::rename(&legacy_root, &cached_repo.repo_dir).with_context(|| {
        format!(
            "failed to move legacy role cache into {}",
            cached_repo.repo_dir.display()
        )
    })?;

    Ok(())
}

fn role_root_has_unexpected_entries(root: &std::path::Path) -> anyhow::Result<bool> {
    if !root.exists() {
        return Ok(false);
    }

    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() != "branches" {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Get the current branch name for a git directory.
pub(super) async fn git_branch(
    dir: &std::path::Path,
    runner: &mut impl CommandRunner,
) -> Option<String> {
    let dir_str = dir.display().to_string();
    try_capture(
        runner,
        "git",
        &["-C", &dir_str, "rev-parse", "--abbrev-ref", "HEAD"],
    )
    .await
}

/// Resolve the role repository: clone if missing, pull if already present.
/// Returns the validated repo metadata and cached repo paths.
/// Prompt the user to confirm cached-repo removal when running in an
/// interactive terminal.  Returns `true` when the user accepts.
#[cfg(test)]
fn confirm_repo_removal_interactive() -> anyhow::Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    Ok(dialoguer::Confirm::new()
        .with_prompt("Remove the cached repo and re-clone from the configured source?")
        .default(false)
        .interact()?)
}

#[cfg(test)]
async fn resolve_agent_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    debug: bool,
    branch_override: Option<&str>,
) -> anyhow::Result<(
    CachedRepo,
    jackin_manifest::repo::ValidatedRoleRepo,
    std::fs::File,
)> {
    resolve_agent_repo_with(
        paths,
        selector,
        git_url,
        runner,
        RepoResolveOptions::interactive(debug).with_branch(branch_override),
        confirm_repo_removal_interactive,
    )
    .await
}

/// Resolve a role repo into the cache, registering it on success.
///
/// Two paths:
/// 1. Cache hit (`.git` already exists): delegate to
///    `resolve_agent_repo_with` which fetch+merges.
/// 2. Cache miss: clone into a temp dir under `data_dir`, validate,
///    `rename` into the cache, then return the validated repo so the
///    caller can persist registration once the install succeeds.
pub async fn register_agent_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    debug: bool,
) -> anyhow::Result<(CachedRepo, jackin_manifest::repo::ValidatedRoleRepo)> {
    let normalized = normalize_github_url(git_url);
    let git_url = normalized.as_str();
    let cached_repo = CachedRepo::new(paths, selector);
    let legacy_root = role_cache_root(paths, selector);
    if cached_repo.repo_dir.join(".git").is_dir() || legacy_root.join(".git").is_dir() {
        let (cached_repo, validated_repo, _lock_file) = resolve_agent_repo_with(
            paths,
            selector,
            git_url,
            runner,
            RepoResolveOptions::interactive(debug),
            || Ok(false),
        )
        .await?;
        return Ok((cached_repo, validated_repo));
    }

    let repo_parent = cached_repo.repo_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "role repo path has no parent: {}",
            cached_repo.repo_dir.display()
        )
    })?;
    std::fs::create_dir_all(repo_parent)?;
    std::fs::create_dir_all(&paths.data_dir)?;

    let role_root = role_cache_root(paths, selector);
    if !cached_repo.repo_dir.exists()
        && !role_root.join(".git").is_dir()
        && role_root_has_unexpected_entries(&role_root)?
    {
        anyhow::bail!(
            "cached role path exists but is not a git repository: {}",
            role_root.display()
        );
    }

    if cached_repo.repo_dir.exists() {
        anyhow::bail!(
            "cached role path exists but is not a git repository: {}",
            cached_repo.repo_dir.display()
        );
    }

    let lock_path = paths
        .data_dir
        .join(format!("{}.repo.lock", runtime_slug(selector)));
    let lock_file = std::fs::File::create(&lock_path)?;
    FileExt::lock(&lock_file)
        .map_err(|e| anyhow::anyhow!("failed to acquire repo lock for {}: {e}", selector.key()))?;

    let temp_dir = tempfile::Builder::new()
        .prefix("role-resolve-")
        .tempdir_in(&paths.data_dir)?;
    let temp_repo = temp_dir.path().join("repo");
    let temp_repo_path = temp_repo.display().to_string();
    let git_run_opts = RunOptions {
        quiet: !debug,
        ..RunOptions::default()
    };
    runner
        .run(
            "git",
            &["clone", git_url, &temp_repo_path],
            None,
            &git_run_opts,
        )
        .await
        .map_err(RepoError::CloneFailed)?;

    let validated_repo = validate_role_repo(&temp_repo).map_err(RepoError::InvalidRoleRepo)?;
    // Install the repo into the cache before persisting registration: if
    // rename fails the role stays unregistered and the user can retry from a
    // clean state. Persisting first would leave config.toml referencing a
    // role that has no on-disk repo, surfacing as "failed to install" once
    // and then silently registered every load thereafter.
    std::fs::rename(&temp_repo, &cached_repo.repo_dir).with_context(|| {
        format!(
            "failed to install role repository at {}",
            cached_repo.repo_dir.display()
        )
    })?;

    Ok((cached_repo, validated_repo))
}

/// Normalize SSH-form GitHub URLs to HTTPS so containers without an
/// SSH key can still clone a role repo when the operator's
/// `[roles.<name>].git` is in the SCP/`ssh://` form.
///
/// Non-GitHub URLs (e.g. self-hosted gitlab) pass through unchanged —
/// substituting their SSH form for HTTPS would risk hitting an
/// HTTPS endpoint that doesn't exist on the operator's SCM.
pub fn normalize_github_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return format!("https://github.com/{rest}");
    }
    if let Some(rest) = url.strip_prefix("ssh://git@github.com/") {
        return format!("https://github.com/{rest}");
    }
    url.to_owned()
}

/// Build the argument list for `git clone`, optionally scoped to a single branch.
/// `git clone -b <branch>` fetches only that branch, making the clone faster and
/// leaving the working tree on the right branch without a separate checkout step.
fn clone_args<'a>(git_url: &'a str, dest: &'a str, branch: Option<&'a str>) -> Vec<&'a str> {
    branch.map_or_else(
        || vec!["clone", git_url, dest],
        |b| vec!["clone", "-b", b, git_url, dest],
    )
}

/// Whether the git subprocess may surface interactive prompts (SSH
/// passphrase, credential helper, branch-divergence questions) on the
/// caller's terminal. `NonInteractive` closes stdin and sets
/// `GIT_TERMINAL_PROMPT=0` so a hanging credential helper cannot freeze
/// the TUI under `jackin console`.
#[derive(Clone, Copy)]
enum GitInteractivity {
    Interactive,
    NonInteractive,
}

pub(super) struct RepoResolveOptions {
    debug: bool,
    branch_override: Option<String>,
    git_interactivity: GitInteractivity,
    refresh_ttl: Option<Duration>,
}

impl RepoResolveOptions {
    pub(super) const fn interactive(debug: bool) -> Self {
        Self {
            debug,
            branch_override: None,
            git_interactivity: GitInteractivity::Interactive,
            refresh_ttl: None,
        }
    }

    /// `debug` is intentionally absent — non-interactive callers run
    /// from inside the TUI alt-screen, where streaming git output via
    /// `--debug` would corrupt the render. Diagnostics go through the
    /// buffered `jackin_diagnostics::debug_log!` channel instead.
    pub(super) const fn non_interactive() -> Self {
        Self {
            debug: false,
            branch_override: None,
            git_interactivity: GitInteractivity::NonInteractive,
            refresh_ttl: None,
        }
    }

    pub(super) fn with_branch(mut self, branch: Option<&str>) -> Self {
        self.branch_override = branch.map(str::to_owned);
        self
    }

    pub(super) const fn with_refresh_ttl(mut self, ttl: Duration) -> Self {
        self.refresh_ttl = Some(ttl);
        self
    }
}

fn fetch_head_age(repo_dir: &std::path::Path) -> Option<Duration> {
    let modified = std::fs::metadata(repo_dir.join(".git").join("FETCH_HEAD"))
        .and_then(|metadata| metadata.modified())
        .ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn fetch_fresh_within_ttl(repo_dir: &std::path::Path, ttl: Duration) -> Option<Duration> {
    if ttl.is_zero() {
        return None;
    }
    let age = fetch_head_age(repo_dir)?;
    (age < ttl).then_some(age)
}

pub(super) async fn resolve_agent_repo_with(
    paths: &JackinPaths,
    selector: &RoleSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    opts: RepoResolveOptions,
    confirm_removal: impl FnOnce() -> anyhow::Result<bool>,
) -> anyhow::Result<(
    CachedRepo,
    jackin_manifest::repo::ValidatedRoleRepo,
    std::fs::File,
)> {
    let normalized = normalize_github_url(git_url);
    let git_url = normalized.as_str();
    let cached_repo = opts.branch_override.as_deref().map_or_else(
        || CachedRepo::new(paths, selector),
        |branch| CachedRepo::for_branch(paths, selector, branch),
    );
    let repo_parent = cached_repo.repo_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "role repo path has no parent: {}",
            cached_repo.repo_dir.display()
        )
    })?;
    std::fs::create_dir_all(repo_parent)?;

    // Short-lived lock around git operations on the shared repo directory.
    // Multiple `jackin load` commands may run in parallel for the same
    // role class (spawning clones); the lock serializes only the git
    // clone/fetch/merge so they don't race on the same working tree.
    // The lock is released as soon as the git section completes.
    //
    // Lock path mirrors the repo_dir path under data_dir so that a branch
    // named `feat/my-pr` and one named `feat-my-pr` (slug collision) always
    // get distinct lock files. The roles_dir prefix is stripped to produce
    // the relative path; `std::fs::create_dir_all` creates intermediate dirs.
    let selector_base = role_cache_root(paths, selector);
    let rel = cached_repo
        .repo_dir
        .strip_prefix(&selector_base)
        .unwrap_or_else(|_| std::path::Path::new(""));
    // Build lock path: data_dir/<container>.locks/<rel>.repo.lock
    let lock_path = {
        let rel_str = rel.to_string_lossy();
        let file_name = if rel_str.is_empty() {
            "default.repo.lock".to_owned()
        } else {
            format!("{rel_str}.repo.lock")
        };
        paths
            .data_dir
            .join(format!("{}.locks", runtime_slug(selector)))
            .join(file_name)
    };
    std::fs::create_dir_all(lock_path.parent().unwrap_or(&paths.data_dir))?;
    let lock_file = std::fs::File::create(&lock_path)?;
    FileExt::lock(&lock_file)
        .map_err(|e| anyhow::anyhow!("failed to acquire repo lock for {}: {e}", selector.key()))?;

    let non_interactive = matches!(opts.git_interactivity, GitInteractivity::NonInteractive)
        || jackin_diagnostics::rich_surface_active();
    let git_run_opts = RunOptions {
        quiet: !opts.debug,
        extra_env: if non_interactive {
            vec![("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned())]
        } else {
            Vec::new()
        },
        null_stdin: non_interactive,
        ..RunOptions::default()
    };

    if opts.branch_override.is_none() {
        migrate_legacy_default_cache(paths, selector, &cached_repo)?;
    }

    let repo_path = cached_repo.repo_dir.display().to_string();
    if cached_repo.repo_dir.join(".git").is_dir() {
        let remote_url = runner
            .capture(
                "git",
                &["-C", &repo_path, "remote", "get-url", "origin"],
                None,
            )
            .await?;
        if !repo_matches(git_url, &remote_url) {
            // TUI alt-screen corruption: `eprintln!` from inside a
            // console session paints over the render.
            jackin_diagnostics::debug_log!(
                "repo_cache",
                "cached role repo remote mismatch: expected={git_url:?} \
                 found={remote_url:?} path={}",
                cached_repo.repo_dir.display()
            );

            if confirm_removal()? {
                std::fs::remove_dir_all(&cached_repo.repo_dir)?;
                let clone_args = clone_args(git_url, &repo_path, opts.branch_override.as_deref());
                runner
                    .run("git", &clone_args, None, &git_run_opts)
                    .await
                    .map_err(RepoError::CloneFailed)?;
                let validated_repo = validate_role_repo(&cached_repo.repo_dir)
                    .map_err(RepoError::InvalidRoleRepo)?;
                return Ok((cached_repo, validated_repo, lock_file));
            }

            return Err(anyhow::Error::new(RepoError::RemoteMismatch));
        }

        let status = runner
            .capture(
                "git",
                &[
                    "-C",
                    &repo_path,
                    "status",
                    "--porcelain",
                    "--untracked-files=normal",
                ],
                None,
            )
            .await?;
        anyhow::ensure!(
            status.is_empty(),
            "cached role repo contains local changes or extra files: {}. Remove the cached repo or clean it before loading.",
            cached_repo.repo_dir.display()
        );

        let fresh_fetch_age = opts.refresh_ttl.and_then(|ttl| {
            opts.branch_override
                .is_none()
                .then(|| fetch_fresh_within_ttl(&cached_repo.repo_dir, ttl))
                .flatten()
        });
        if let Some(age) = fresh_fetch_age {
            jackin_diagnostics::debug_log!(
                "repo_cache",
                "skipping role repo fetch for {}: FETCH_HEAD is {}s old",
                selector.key(),
                age.as_secs()
            );
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact(
                    "repo_refresh_skipped",
                    &format!(
                        "role repo fetch skipped: FETCH_HEAD is {}s old",
                        age.as_secs()
                    ),
                );
            }
        } else {
            // Fetch + merge instead of pull to avoid "Cannot fast-forward to
            // multiple branches" errors that occur with `git pull` when the
            // remote has multiple branches. When a branch is pinned via
            // `--branch`, use it directly; otherwise derive from HEAD.
            let branch = match opts.branch_override.as_deref() {
                Some(branch) => branch.to_owned(),
                None => git_branch(&cached_repo.repo_dir, runner)
                    .await
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "could not determine current branch of cached role repo at {}",
                            cached_repo.repo_dir.display()
                        )
                    })?,
            };
            runner
                .run(
                    "git",
                    &["-C", &repo_path, "fetch", "origin", &branch],
                    None,
                    &git_run_opts,
                )
                .await?;
            let ff_result = runner
                .run(
                    "git",
                    &["-C", &repo_path, "merge", "--ff-only", "FETCH_HEAD"],
                    None,
                    &git_run_opts,
                )
                .await;
            if ff_result.is_err() {
                // Route through buffered debug channel so the TUI alt-screen
                // is not corrupted when this fires under `jackin console`.
                jackin_diagnostics::debug_log!(
                    "repo_cache",
                    "cached role branch diverged (remote may have been force-pushed) — resetting to origin/{branch}"
                );
                runner
                    .run(
                        "git",
                        &["-C", &repo_path, "reset", "--hard", "FETCH_HEAD"],
                        None,
                        &git_run_opts,
                    )
                    .await?;
            }
        }
    } else {
        let clone_args = clone_args(git_url, &repo_path, opts.branch_override.as_deref());
        runner
            .run("git", &clone_args, None, &git_run_opts)
            .await
            .map_err(RepoError::CloneFailed)?;
    }

    let validated_repo =
        validate_role_repo(&cached_repo.repo_dir).map_err(RepoError::InvalidRoleRepo)?;

    // Return the repo lock so the caller can hold it until the build
    // context (a snapshot copy of the repo) is created.  This prevents
    // a parallel load from fast-forwarding the shared repo between
    // validation and context creation.
    Ok((cached_repo, validated_repo, lock_file))
}

#[cfg(test)]
mod tests;
