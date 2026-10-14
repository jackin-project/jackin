#![expect(
    clippy::too_many_lines,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]

use anyhow::Context;
use fs4::FileExt;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_core::{Clock, CommandRunner, RunOptions, SystemClock};
use jackin_instance::runtime_slug;
use jackin_manifest::repo::{CachedRepo, validate_role_repo};
#[cfg(test)]
use std::io::IsTerminal;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use jackin_runtime_identity::identity::try_capture;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("repository is not available or cannot be accessed")]
    CloneFailed(#[source] anyhow::Error),

    #[error("cached role repo remote mismatch — aborting")]
    RemoteMismatch,

    #[error("invalid role repo: {0}")]
    InvalidRoleRepo(#[from] jackin_manifest::repo::RoleRepoValidationError),
}

#[derive(Clone, Debug)]
pub struct RepoLock(
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "guard only: the flock is released when the last handle is dropped"
        )
    )]
    Arc<std::fs::File>,
);

type RepoLockGate = std::sync::Mutex<std::sync::Weak<std::fs::File>>;

static REPO_LOCK_GATES: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Arc<RepoLockGate>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn repo_lock_gate(path: &std::path::Path) -> Arc<RepoLockGate> {
    let mut gates = REPO_LOCK_GATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    gates.retain(|_, gate| {
        Arc::strong_count(gate) > 1
            || gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .strong_count()
                > 0
    });
    Arc::clone(gates.entry(path.to_path_buf()).or_default())
}

fn acquire_repo_lock_blocking(path: &std::path::Path) -> anyhow::Result<RepoLock> {
    let gate = repo_lock_gate(path);
    let mut held = gate
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = held.upgrade() {
        return Ok(RepoLock(existing));
    }
    let file = std::fs::File::create(path)?;
    FileExt::lock(&file)?;
    let handle = Arc::new(file);
    *held = Arc::downgrade(&handle);
    Ok(RepoLock(handle))
}

async fn acquire_repo_lock(path: &std::path::Path) -> anyhow::Result<RepoLock> {
    let owned = path.to_path_buf();
    jackin_telemetry::spawn::joined_blocking(move || acquire_repo_lock_blocking(&owned))
        .await
        .context("repo lock acquisition task failed")?
}

pub fn parse_repo_name(url: &str) -> Option<String> {
    let url = url.trim();
    let stripped = url.strip_suffix(".git").unwrap_or(url);

    if let Some(rest) = stripped
        .strip_prefix("https://")
        .or_else(|| stripped.strip_prefix("http://"))
    {
        return rest.find('/').map(|i| rest[i + 1..].to_string());
    }

    stripped.rsplit_once(':').map(|(_, p)| p.to_owned())
}

pub fn repo_matches(expected: &str, actual: &str) -> bool {
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

pub async fn git_branch(dir: &std::path::Path, runner: &mut impl CommandRunner) -> Option<String> {
    let dir_str = dir.display().to_string();
    try_capture(
        runner,
        "git",
        &["-C", &dir_str, "rev-parse", "--abbrev-ref", "HEAD"],
    )
    .await
}

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
    RepoLock,
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
    let _lock_file = acquire_repo_lock(&lock_path)
        .await
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

    std::fs::rename(&temp_repo, &cached_repo.repo_dir).with_context(|| {
        format!(
            "failed to install role repository at {}",
            cached_repo.repo_dir.display()
        )
    })?;

    Ok((cached_repo, validated_repo))
}

pub fn normalize_github_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return format!("https://github.com/{rest}");
    }
    if let Some(rest) = url.strip_prefix("ssh://git@github.com/") {
        return format!("https://github.com/{rest}");
    }
    url.to_owned()
}

fn clone_args<'a>(git_url: &'a str, dest: &'a str, branch: Option<&'a str>) -> Vec<&'a str> {
    branch.map_or_else(
        || vec!["clone", git_url, dest],
        |b| vec!["clone", "-b", b, git_url, dest],
    )
}

#[derive(Clone, Copy, Debug)]
enum GitInteractivity {
    Interactive,
    NonInteractive,
}

#[derive(Debug)]
pub struct RepoResolveOptions {
    debug: bool,
    branch_override: Option<String>,
    git_interactivity: GitInteractivity,
    refresh_ttl: Option<Duration>,
    clock: Arc<dyn Clock>,
}

impl RepoResolveOptions {
    pub fn interactive(debug: bool) -> Self {
        Self {
            debug,
            branch_override: None,
            git_interactivity: GitInteractivity::Interactive,
            refresh_ttl: None,
            clock: Arc::new(SystemClock),
        }
    }

    pub fn non_interactive() -> Self {
        Self {
            debug: false,
            branch_override: None,
            git_interactivity: GitInteractivity::NonInteractive,
            refresh_ttl: None,
            clock: Arc::new(SystemClock),
        }
    }

    #[must_use]
    pub fn with_branch(mut self, branch: Option<&str>) -> Self {
        self.branch_override = branch.map(str::to_owned);
        self
    }

    #[must_use]
    pub const fn with_refresh_ttl(mut self, ttl: Duration) -> Self {
        self.refresh_ttl = Some(ttl);
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }
}

fn fetch_head_age_at(repo_dir: &std::path::Path, now: SystemTime) -> Option<Duration> {
    let modified = std::fs::metadata(repo_dir.join(".git").join("FETCH_HEAD"))
        .and_then(|metadata| metadata.modified())
        .ok()?;
    now.duration_since(modified).ok()
}

fn fetch_fresh_within_ttl_at(
    repo_dir: &std::path::Path,
    ttl: Duration,
    now: SystemTime,
) -> Option<Duration> {
    if ttl.is_zero() {
        return None;
    }
    let age = fetch_head_age_at(repo_dir, now)?;
    (age < ttl).then_some(age)
}

fn fetch_fresh_within_ttl_with_clock(
    repo_dir: &std::path::Path,
    ttl: Duration,
    clock: &dyn Clock,
) -> Option<Duration> {
    fetch_fresh_within_ttl_at(repo_dir, ttl, clock.now_system())
}

pub async fn resolve_agent_repo_with(
    paths: &JackinPaths,
    selector: &RoleSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    opts: RepoResolveOptions,
    confirm_removal: impl FnOnce() -> anyhow::Result<bool>,
) -> anyhow::Result<(
    CachedRepo,
    jackin_manifest::repo::ValidatedRoleRepo,
    RepoLock,
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

    let selector_base = role_cache_root(paths, selector);
    let rel = cached_repo
        .repo_dir
        .strip_prefix(&selector_base)
        .unwrap_or_else(|_| std::path::Path::new(""));

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
    let lock_file = acquire_repo_lock(&lock_path)
        .await
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
            jackin_telemetry::cache::decision(
                jackin_telemetry::schema::enums::CacheName::RoleRepository,
                jackin_telemetry::schema::enums::CacheResult::Stale,
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
        if !status.is_empty() {
            jackin_telemetry::cache::decision(
                jackin_telemetry::schema::enums::CacheName::RoleRepository,
                jackin_telemetry::schema::enums::CacheResult::Stale,
            );
            anyhow::bail!(
                "cached role repo contains local changes or extra files: {}. Remove the cached repo or clean it before loading.",
                cached_repo.repo_dir.display()
            );
        }

        let fresh_fetch_age = opts.refresh_ttl.and_then(|ttl| {
            opts.branch_override
                .is_none()
                .then(|| {
                    fetch_fresh_within_ttl_with_clock(&cached_repo.repo_dir, ttl, &*opts.clock)
                })
                .flatten()
        });
        if let Some(age) = fresh_fetch_age {
            jackin_telemetry::cache::decision(
                jackin_telemetry::schema::enums::CacheName::RoleRepository,
                jackin_telemetry::schema::enums::CacheResult::Hit,
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
                let _warning = jackin_telemetry::record_recovered_degradation();
                runner
                    .run(
                        "git",
                        &["-C", &repo_path, "reset", "--hard", "FETCH_HEAD"],
                        None,
                        &git_run_opts,
                    )
                    .await?;
            }
            jackin_telemetry::cache::decision(
                jackin_telemetry::schema::enums::CacheName::RoleRepository,
                jackin_telemetry::schema::enums::CacheResult::Reuse,
            );
        }
    } else {
        jackin_telemetry::cache::decision(
            jackin_telemetry::schema::enums::CacheName::RoleRepository,
            jackin_telemetry::schema::enums::CacheResult::Miss,
        );
        let clone_args = clone_args(git_url, &repo_path, opts.branch_override.as_deref());
        runner
            .run("git", &clone_args, None, &git_run_opts)
            .await
            .map_err(RepoError::CloneFailed)?;
    }

    let validated_repo =
        validate_role_repo(&cached_repo.repo_dir).map_err(RepoError::InvalidRoleRepo)?;

    Ok((cached_repo, validated_repo, lock_file))
}

#[cfg(test)]
mod tests;
