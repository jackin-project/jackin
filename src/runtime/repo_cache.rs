use crate::docker::{CommandRunner, RunOptions};
use crate::instance::primary_container_name;
use crate::paths::JackinPaths;
use crate::repo::{CachedRepo, validate_role_repo};
use crate::selector::RoleSelector;
use anyhow::Context;
use fs2::FileExt;
use std::io::IsTerminal;

use super::identity::try_capture;

/// Map an anyhow error from `validate_role_repo` into a typed
/// `RepoError::InvalidRoleRepo` when any link in the chain uses the
/// validator's `invalid role repo: ` prefix; pass anything else through
/// unchanged.
///
/// The validator currently emits anyhow strings; this function is the
/// single substring match left over after the typed-error refactor and
/// lives here so it's co-located with the variant it produces.
fn map_validate_error(err: anyhow::Error) -> anyhow::Error {
    for cause in err.chain() {
        if let Some(detail) = cause.to_string().strip_prefix("invalid role repo: ") {
            return anyhow::Error::new(RepoError::InvalidRoleRepo {
                detail: detail.to_string(),
            });
        }
    }
    err
}

/// Typed errors raised by role-repo resolution.
///
/// Surfaced through `anyhow::Error` chains so the editor can downcast and
/// pick a friendly translation without substring-matching free text. Add
/// new variants here together with their `friendly_role_resolution_error`
/// arm in `console::manager::input::editor`.
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

    /// `validate_role_repo` rejected the cloned repo. `detail` is the
    /// validator's message with the `invalid role repo: ` prefix stripped.
    #[error("invalid role repo: {detail}")]
    InvalidRoleRepo { detail: String },
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
    stripped.rsplit_once(':').map(|(_, p)| p.to_string())
}

pub(super) fn repo_matches(expected: &str, actual: &str) -> bool {
    match (parse_repo_name(expected), parse_repo_name(actual)) {
        (Some(expected_repo), Some(actual_repo)) => expected_repo == actual_repo,
        _ => expected.trim() == actual.trim(),
    }
}

/// Derive a short repository name from a git remote URL (e.g. `jackin-project/jackin`).
pub(super) fn git_repo_name(
    dir: &std::path::Path,
    runner: &mut impl CommandRunner,
) -> Option<String> {
    let dir_str = dir.display().to_string();
    let url = try_capture(
        runner,
        "git",
        &["-C", &dir_str, "remote", "get-url", "origin"],
    )?;
    parse_repo_name(&url)
}

/// Get the current branch name for a git directory.
pub(super) fn git_branch(dir: &std::path::Path, runner: &mut impl CommandRunner) -> Option<String> {
    let dir_str = dir.display().to_string();
    try_capture(
        runner,
        "git",
        &["-C", &dir_str, "rev-parse", "--abbrev-ref", "HEAD"],
    )
}

/// Check whether a path is inside a git work tree.
pub(super) fn is_git_dir(dir: &std::path::Path, runner: &mut impl CommandRunner) -> bool {
    let dir_str = dir.display().to_string();
    try_capture(
        runner,
        "git",
        &["-C", &dir_str, "rev-parse", "--is-inside-work-tree"],
    )
    .is_some()
}

/// Resolve the role repository: clone if missing, pull if already present.
/// Returns the validated repo metadata and cached repo paths.
/// Prompt the user to confirm cached-repo removal when running in an
/// interactive terminal.  Returns `true` when the user accepts.
pub(super) fn confirm_repo_removal_interactive() -> anyhow::Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    Ok(dialoguer::Confirm::new()
        .with_prompt("Remove the cached repo and re-clone from the configured source?")
        .default(false)
        .interact()?)
}

pub(super) fn resolve_agent_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    debug: bool,
) -> anyhow::Result<(CachedRepo, crate::repo::ValidatedRoleRepo, std::fs::File)> {
    resolve_agent_repo_with(
        paths,
        selector,
        git_url,
        runner,
        debug,
        confirm_repo_removal_interactive,
    )
}

/// Resolve a role repo into the cache, registering it on success.
///
/// Two paths:
/// 1. Cache hit (`.git` already exists): delegate to
///    `resolve_agent_repo_with` which fetch+merges, then run
///    `persist_registration` if validation passes.
/// 2. Cache miss: clone into a temp dir under `data_dir`, validate,
///    `rename` into the cache, then run `persist_registration`. Rename
///    happens before persist so a failed rename leaves the role
///    *un-registered* (clean state) rather than registered without an
///    on-disk repo (broken state).
///
/// `persist_registration` is the single commit point — it must be
/// idempotent so retries after a transient failure are safe.
pub(super) fn register_agent_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    debug: bool,
    persist_registration: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<(CachedRepo, crate::repo::ValidatedRoleRepo)> {
    let cached_repo = CachedRepo::new(paths, selector);
    if cached_repo.repo_dir.join(".git").is_dir() {
        let (cached_repo, validated_repo, _lock_file) =
            resolve_agent_repo_with(paths, selector, git_url, runner, debug, || Ok(false))?;
        persist_registration()?;
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

    if cached_repo.repo_dir.exists() {
        anyhow::bail!(
            "cached role path exists but is not a git repository: {}",
            cached_repo.repo_dir.display()
        );
    }

    let lock_path = paths
        .data_dir
        .join(format!("{}.repo.lock", primary_container_name(selector)));
    let lock_file = std::fs::File::create(&lock_path)?;
    lock_file
        .lock_exclusive()
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
        .map_err(RepoError::CloneFailed)?;

    let validated_repo = validate_role_repo(&temp_repo).map_err(map_validate_error)?;
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
    persist_registration().with_context(|| {
        format!(
            "role repo installed at {} but registration could not be persisted",
            cached_repo.repo_dir.display()
        )
    })?;

    Ok((cached_repo, validated_repo))
}

pub(super) fn resolve_agent_repo_with(
    paths: &JackinPaths,
    selector: &RoleSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    debug: bool,
    confirm_removal: impl FnOnce() -> anyhow::Result<bool>,
) -> anyhow::Result<(CachedRepo, crate::repo::ValidatedRoleRepo, std::fs::File)> {
    let cached_repo = CachedRepo::new(paths, selector);
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
    let lock_path = paths
        .data_dir
        .join(format!("{}.repo.lock", primary_container_name(selector)));
    std::fs::create_dir_all(&paths.data_dir)?;
    let lock_file = std::fs::File::create(&lock_path)?;
    lock_file
        .lock_exclusive()
        .map_err(|e| anyhow::anyhow!("failed to acquire repo lock for {}: {e}", selector.key()))?;

    let git_run_opts = RunOptions {
        quiet: !debug,
        ..RunOptions::default()
    };

    let repo_path = cached_repo.repo_dir.display().to_string();
    if cached_repo.repo_dir.join(".git").is_dir() {
        let remote_url = runner.capture(
            "git",
            &["-C", &repo_path, "remote", "get-url", "origin"],
            None,
        )?;
        if !repo_matches(git_url, &remote_url) {
            // Route diagnostics through the buffered debug channel rather
            // than `eprintln!` — the latter corrupts the alt-screen render
            // when called from inside the TUI session. Operators with
            // `--debug` still get the full expected/found/path trio.
            crate::debug_log!(
                "repo_cache",
                "cached role repo remote mismatch: expected={git_url:?} \
                 found={remote_url:?} path={}",
                cached_repo.repo_dir.display()
            );

            if confirm_removal()? {
                std::fs::remove_dir_all(&cached_repo.repo_dir)?;
                runner
                    .run("git", &["clone", git_url, &repo_path], None, &git_run_opts)
                    .map_err(RepoError::CloneFailed)?;
                let validated_repo =
                    validate_role_repo(&cached_repo.repo_dir).map_err(map_validate_error)?;
                return Ok((cached_repo, validated_repo, lock_file));
            }

            return Err(anyhow::Error::new(RepoError::RemoteMismatch));
        }

        let status = runner.capture(
            "git",
            &[
                "-C",
                &repo_path,
                "status",
                "--porcelain",
                "--ignored=matching",
                "--untracked-files=all",
            ],
            None,
        )?;
        anyhow::ensure!(
            status.is_empty(),
            "cached role repo contains local changes or extra files: {}. Remove the cached repo or clean it before loading.",
            cached_repo.repo_dir.display()
        );

        // Fetch + merge instead of pull to avoid "Cannot fast-forward to
        // multiple branches" errors that occur with `git pull` when the
        // remote has multiple branches.
        let branch = git_branch(&cached_repo.repo_dir, runner).ok_or_else(|| {
            anyhow::anyhow!(
                "could not determine current branch of cached role repo at {}",
                cached_repo.repo_dir.display()
            )
        })?;
        runner.run(
            "git",
            &["-C", &repo_path, "fetch", "origin", &branch],
            None,
            &git_run_opts,
        )?;
        runner.run(
            "git",
            &["-C", &repo_path, "merge", "--ff-only", "FETCH_HEAD"],
            None,
            &git_run_opts,
        )?;
    } else {
        runner
            .run("git", &["clone", git_url, &repo_path], None, &git_run_opts)
            .map_err(RepoError::CloneFailed)?;
    }

    let validated_repo = validate_role_repo(&cached_repo.repo_dir).map_err(map_validate_error)?;

    // Return the repo lock so the caller can hold it until the build
    // context (a snapshot copy of the repo) is created.  This prevents
    // a parallel load from fast-forwarding the shared repo between
    // validation and context creation.
    Ok((cached_repo, validated_repo, lock_file))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::FakeRunner;
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::RoleSelector;
    use tempfile::tempdir;

    #[test]
    fn parse_repo_name_extracts_owner_repo_from_ssh_url() {
        assert_eq!(
            parse_repo_name("git@github.com:jackin-project/jackin.git"),
            Some("jackin-project/jackin".to_string())
        );
    }

    #[test]
    fn parse_repo_name_extracts_owner_repo_from_https_url() {
        assert_eq!(
            parse_repo_name("https://github.com/jackin-project/jackin.git"),
            Some("jackin-project/jackin".to_string())
        );
    }

    #[test]
    fn parse_repo_name_handles_url_without_git_suffix() {
        assert_eq!(
            parse_repo_name("https://github.com/jackin-project/jackin"),
            Some("jackin-project/jackin".to_string())
        );
        assert_eq!(
            parse_repo_name("git@github.com:jackin-project/jackin"),
            Some("jackin-project/jackin".to_string())
        );
    }

    #[test]
    fn resolve_agent_repo_rejects_cached_repo_with_wrong_remote() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner =
            FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_string()]);
        let error = resolve_agent_repo(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            false,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("cached role repo remote mismatch")
        );
    }

    #[test]
    fn resolve_agent_repo_recovers_when_user_confirms_removal() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        // The capture queue provides: 1) the wrong remote URL, then 2) a
        // successful clone response (empty output).  After the user confirms,
        // the function removes the stale dir and re-clones.
        let mut runner = FakeRunner::with_capture_queue([
            "git@github.com:evil/agent-smith.git".to_string(),
            String::new(), // clone output
        ]);

        // Simulate what `git clone` would produce on disk: recreate the repo
        // files when the clone command is captured by FakeRunner.
        let repo_dir_clone = repo_dir;
        runner.side_effects.push((
            "clone".to_string(),
            Box::new(move || {
                std::fs::create_dir_all(repo_dir_clone.join(".git")).unwrap();
                std::fs::write(
                    repo_dir_clone.join("Dockerfile"),
                    "FROM projectjackin/construct:trixie\n",
                )
                .unwrap();
                std::fs::write(
                    repo_dir_clone.join("jackin.role.toml"),
                    r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
                )
                .unwrap();
            }),
        ));

        let result = resolve_agent_repo_with(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            false,
            || Ok(true), // user confirms removal
        );

        assert!(result.is_ok(), "expected recovery to succeed: {result:?}");
        assert!(
            runner.recorded.iter().any(|c| c.contains("clone")),
            "expected a git clone after removal"
        );
    }

    #[test]
    fn resolve_agent_repo_aborts_when_user_declines_removal() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner =
            FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_string()]);
        let error = resolve_agent_repo_with(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            false,
            || Ok(false), // user declines
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("cached role repo remote mismatch")
        );
        // The cached repo directory should still exist
        assert!(repo_dir.join(".git").is_dir());
    }

    #[test]
    fn resolve_agent_repo_rejects_cached_repo_with_local_changes() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner = FakeRunner::with_capture_queue([
            "git@github.com:jackin-project/jackin-agent-smith.git".to_string(),
            "?? scratch.txt".to_string(),
        ]);
        let error = resolve_agent_repo(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            false,
        )
        .unwrap_err();

        assert!(error.to_string().contains("contains local changes"));
    }

    #[test]
    fn resolve_agent_repo_uses_run_for_clone_after_recovery() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner =
            FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_string()]);
        let repo_dir_clone = repo_dir;
        runner.side_effects.push((
            "clone".to_string(),
            Box::new(move || {
                std::fs::create_dir_all(repo_dir_clone.join(".git")).unwrap();
                std::fs::write(
                    repo_dir_clone.join("Dockerfile"),
                    "FROM projectjackin/construct:trixie\n",
                )
                .unwrap();
                std::fs::write(
                    repo_dir_clone.join("jackin.role.toml"),
                    r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
                )
                .unwrap();
            }),
        ));

        let result = resolve_agent_repo_with(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            false,
            || Ok(true),
        );

        assert!(result.is_ok(), "expected recovery to succeed: {result:?}");
        assert!(runner.run_recorded.iter().any(|call| {
            call.contains("git clone https://github.com/jackin-project/jackin-agent-smith.git")
        }));
    }

    #[test]
    fn resolve_agent_repo_uses_run_for_pull_on_clean_repo() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = RoleSelector::new(None, "agent-smith");
        let repo_dir = paths.roles_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner = FakeRunner::with_capture_queue([
            "git@github.com:jackin-project/jackin-agent-smith.git".to_string(),
            String::new(),      // git status --porcelain (clean)
            "main".to_string(), // git rev-parse --abbrev-ref HEAD
        ]);

        let result = resolve_agent_repo(
            &paths,
            &selector,
            "https://github.com/jackin-project/jackin-agent-smith.git",
            &mut runner,
            false,
        );

        assert!(
            result.is_ok(),
            "expected clean repo update to succeed: {result:?}"
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|call| call.contains("git -C") && call.contains("fetch origin")),
            "expected a git fetch: {:?}",
            runner.run_recorded
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|call| call.contains("git -C") && call.contains("merge --ff-only")),
            "expected a git merge --ff-only: {:?}",
            runner.run_recorded
        );
    }

    /// Materialise a valid role repo at `repo_dir` — `.git`, `Dockerfile`,
    /// and `jackin.role.toml` are all required by `validate_role_repo`.
    fn seed_valid_role_repo(repo_dir: &std::path::Path) {
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();
    }

    /// Find the `repo` subdir under the first `role-resolve-*` temp dir
    /// `register_agent_repo` creates inside `data_dir`. Used inside
    /// `git clone` side-effect callbacks where the path isn't known
    /// until the function runs.
    fn first_temp_role_repo(data_dir: &std::path::Path) -> std::path::PathBuf {
        std::fs::read_dir(data_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("role-resolve-"))
            })
            .expect("role registration temp dir should exist before git clone side-effect")
            .join("repo")
    }

    #[test]
    fn register_agent_repo_cleans_up_temp_dir_on_validate_failure() {
        // When validation rejects the cloned repo (here: no Dockerfile,
        // no jackin.role.toml — just a `.git` dir), `register_agent_repo`
        // must NOT rename the temp dir into the cache and must NOT call
        // `persist_registration`. The temp dir is cleaned up by tempfile's
        // Drop, so the only assertion is that the cache slot is empty.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let selector = RoleSelector::new(None, "agent-broken");
        let cached_dir = paths.roles_dir.join("agent-broken");

        let data_dir = paths.data_dir.clone();
        let mut runner = FakeRunner::default();
        runner.side_effects.push((
            "git clone".to_string(),
            // Materialise a `.git` dir but skip the manifest files so
            // `validate_role_repo` rejects the clone.
            Box::new(move || {
                let temp_repo = first_temp_role_repo(&data_dir);
                std::fs::create_dir_all(temp_repo.join(".git")).unwrap();
            }),
        ));

        let persist_called = std::cell::Cell::new(false);
        let err = register_agent_repo(
            &paths,
            &selector,
            "https://github.com/example/agent-broken.git",
            &mut runner,
            false,
            || {
                persist_called.set(true);
                Ok(())
            },
        )
        .unwrap_err();

        assert!(
            err.downcast_ref::<RepoError>()
                .is_some_and(|e| matches!(e, RepoError::InvalidRoleRepo { .. })),
            "expected RepoError::InvalidRoleRepo, got {err:?}"
        );
        assert!(
            !persist_called.get(),
            "persist must not run on validate failure"
        );
        assert!(
            !cached_dir.exists(),
            "cache slot must remain empty when validate fails: {}",
            cached_dir.display()
        );
    }

    #[test]
    fn register_agent_repo_aborts_when_persist_registration_fails() {
        // Persist runs after rename, so a persist failure leaves the
        // cache populated but registration un-persisted. Verify the
        // error surfaces with the diagnostic context that points the
        // operator at the inconsistency.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let selector = RoleSelector::new(None, "agent-persist-fail");
        let cached_dir = paths.roles_dir.join("agent-persist-fail");

        let data_dir = paths.data_dir.clone();
        let mut runner = FakeRunner::default();
        runner.side_effects.push((
            "git clone".to_string(),
            Box::new(move || seed_valid_role_repo(&first_temp_role_repo(&data_dir))),
        ));

        let err = register_agent_repo(
            &paths,
            &selector,
            "https://github.com/example/agent-persist-fail.git",
            &mut runner,
            false,
            || anyhow::bail!("simulated config write failure"),
        )
        .unwrap_err();

        let chain = format!("{err:?}");
        assert!(
            chain.contains("registration could not be persisted"),
            "error chain must surface persist failure: {chain}"
        );
        assert!(
            cached_dir.join(".git").is_dir(),
            "cache must be populated before persist runs (rename-then-persist invariant)",
        );
    }

    #[test]
    fn register_agent_repo_rejects_stale_non_git_directory() {
        // A pre-existing directory at the cache slot that is *not* a
        // git repo must bail rather than overwrite or skip — the
        // operator likely has unsynced work there.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let selector = RoleSelector::new(None, "agent-stale");
        let cached_dir = paths.roles_dir.join("agent-stale");
        std::fs::create_dir_all(&cached_dir).unwrap();
        std::fs::write(cached_dir.join("README"), "operator's lost work\n").unwrap();

        let mut runner = FakeRunner::default();
        let err = register_agent_repo(
            &paths,
            &selector,
            "https://github.com/example/agent-stale.git",
            &mut runner,
            false,
            || Ok(()),
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("cached role path exists but is not a git repository"),
            "expected stale-non-git bail, got: {err}"
        );
        // Operator's file remains untouched.
        assert_eq!(
            std::fs::read_to_string(cached_dir.join("README")).unwrap(),
            "operator's lost work\n"
        );
    }
}
