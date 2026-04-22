use crate::docker::{CommandRunner, RunOptions};
use crate::instance::primary_container_name;
use crate::paths::JackinPaths;
use crate::repo::{CachedRepo, validate_agent_repo};
use crate::selector::ClassSelector;
use fs2::FileExt;
use owo_colors::OwoColorize;
use std::io::IsTerminal;

use super::identity::try_capture;

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

/// Resolve the agent repository: clone if missing, pull if already present.
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
    selector: &ClassSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    debug: bool,
) -> anyhow::Result<(CachedRepo, crate::repo::ValidatedAgentRepo, std::fs::File)> {
    resolve_agent_repo_with(
        paths,
        selector,
        git_url,
        runner,
        debug,
        confirm_repo_removal_interactive,
    )
}

pub(super) fn resolve_agent_repo_with(
    paths: &JackinPaths,
    selector: &ClassSelector,
    git_url: &str,
    runner: &mut impl CommandRunner,
    debug: bool,
    confirm_removal: impl FnOnce() -> anyhow::Result<bool>,
) -> anyhow::Result<(CachedRepo, crate::repo::ValidatedAgentRepo, std::fs::File)> {
    let cached_repo = CachedRepo::new(paths, selector);
    let repo_parent = cached_repo.repo_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "agent repo path has no parent: {}",
            cached_repo.repo_dir.display()
        )
    })?;
    std::fs::create_dir_all(repo_parent)?;

    // Short-lived lock around git operations on the shared repo directory.
    // Multiple `jackin load` commands may run in parallel for the same
    // agent class (spawning clones); the lock serializes only the git
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
            let repo_display = cached_repo.repo_dir.display();
            eprintln!(
                "{} cached agent repo remote does not match configured source",
                "error:".red().bold()
            );
            eprintln!("  expected: {}", git_url.green());
            eprintln!("  found:    {}", remote_url.yellow());
            eprintln!();
            eprintln!("To fix this, remove the cached repo and try again:");
            eprintln!("  rm -rf {repo_display}");
            eprintln!();

            if confirm_removal()? {
                std::fs::remove_dir_all(&cached_repo.repo_dir)?;
                runner.run("git", &["clone", git_url, &repo_path], None, &git_run_opts)?;
                let validated_repo = validate_agent_repo(&cached_repo.repo_dir)?;
                return Ok((cached_repo, validated_repo, lock_file));
            }

            anyhow::bail!("cached agent repo remote mismatch — aborting");
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
            "cached agent repo contains local changes or extra files: {}. Remove the cached repo or clean it before loading.",
            cached_repo.repo_dir.display()
        );

        // Fetch + merge instead of pull to avoid "Cannot fast-forward to
        // multiple branches" errors that occur with `git pull` when the
        // remote has multiple branches.
        let branch =
            git_branch(&cached_repo.repo_dir, runner).unwrap_or_else(|| "main".to_string());
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
        runner.run("git", &["clone", git_url, &repo_path], None, &git_run_opts)?;
    }

    let validated_repo = validate_agent_repo(&cached_repo.repo_dir)?;

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
    use crate::selector::ClassSelector;
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
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
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
                .contains("cached agent repo remote mismatch")
        );
    }

    #[test]
    fn resolve_agent_repo_recovers_when_user_confirms_removal() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
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
        let repo_dir_clone = repo_dir.clone();
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
                    repo_dir_clone.join("jackin.agent.toml"),
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
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
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
                .contains("cached agent repo remote mismatch")
        );
        // The cached repo directory should still exist
        assert!(repo_dir.join(".git").is_dir());
    }

    #[test]
    fn resolve_agent_repo_rejects_cached_repo_with_local_changes() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
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
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let mut runner =
            FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_string()]);
        let repo_dir_clone = repo_dir.clone();
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
                    repo_dir_clone.join("jackin.agent.toml"),
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
        let selector = ClassSelector::new(None, "agent-smith");
        let repo_dir = paths.agents_dir.join("agent-smith");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        std::fs::write(
            repo_dir.join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
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
}
