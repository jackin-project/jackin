//! Git pull helpers for workspace repos, extracted to sibling module.

use std::path::Path;

use jackin_config;

pub(crate) enum GitPullResult {
    Success { src: String, stdout: String },
    Failure { src: String, stderr: String },
    SpawnError { src: String, error: std::io::Error },
    JoinError { src: String },
}

#[cfg(test)]
pub(crate) fn pull_workspace_repos_with_git(
    workspace: &jackin_config::ResolvedWorkspace,
    debug: bool,
    git_program: &Path,
) -> Vec<GitPullResult> {
    pull_git_sources_with_git(git_pull_sources(workspace), debug, git_program, true)
}

pub(crate) fn git_pull_sources(workspace: &jackin_config::ResolvedWorkspace) -> Vec<String> {
    let mut sources = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for mount in &workspace.mounts {
        if Path::new(&mount.src).join(".git").exists() && seen.insert(mount.src.clone()) {
            sources.push(mount.src.clone());
        }
    }
    sources
}

pub(crate) fn pull_git_sources_with_git(
    sources: Vec<String>,
    debug: bool,
    git_program: &Path,
    print_starts: bool,
) -> Vec<GitPullResult> {
    let mut pulls = Vec::new();

    for src in sources {
        if debug {
            jackin_diagnostics::active_debug("git_pull", &format!("git pull in {src}"));
            if jackin_diagnostics::active_run().is_none() {
                tracing::debug!(src, "git pull in workspace");
            }
        }
        if print_starts {
            let src_display = jackin_diagnostics::shorten_home(&src);
            tracing::info!(src = src_display.as_str(), "pulling workspace");
            eprintln!("  Pulling {src_display} …");
        }
        let git_program = git_program.to_path_buf();
        pulls.push((
            src.clone(),
            std::thread::spawn(move || {
                let mut command = std::process::Command::new(git_program);
                command
                    .args(["-C", &src, "pull"])
                    .env("GIT_TERMINAL_PROMPT", "0")
                    .stdin(std::process::Stdio::null());
                #[expect(
                    clippy::disallowed_methods,
                    reason = "git pull runs on a dedicated OS thread, not the launch render runtime thread"
                )]
                match command.output() {
                    Ok(out) if out.status.success() => GitPullResult::Success {
                        src,
                        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                    },
                    Ok(out) => GitPullResult::Failure {
                        src,
                        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                    },
                    Err(error) => GitPullResult::SpawnError { src, error },
                }
            }),
        ));
    }

    pulls
        .into_iter()
        .map(|(src, handle)| handle.join().unwrap_or(GitPullResult::JoinError { src }))
        .collect()
}

pub(crate) fn print_git_pull_results(results: &[GitPullResult]) {
    for result in results {
        match result {
            GitPullResult::Success { stdout, .. } => {
                print_git_pull_stdout(stdout);
            }
            GitPullResult::Failure { src, stderr } => {
                tracing::warn!(src, stderr = stderr.trim(), "git pull failed");
                eprintln!("  Warning: git pull failed in {}: {}", src, stderr.trim());
            }
            GitPullResult::SpawnError { src, error } => {
                tracing::warn!(src, %error, "git pull spawn error");
                eprintln!("  Warning: could not run git pull in {src}: {error}");
            }
            GitPullResult::JoinError { src } => {
                tracing::warn!(src, "git pull thread panicked");
                eprintln!("  Warning: git pull thread panicked in {src}");
            }
        }
    }
}

fn print_git_pull_stdout(stdout: &str) {
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        eprintln!("    {trimmed}");
    }
}

pub(crate) fn record_git_pull_results(results: &[GitPullResult]) -> (usize, usize) {
    let mut ok = 0;
    let mut failed = 0;
    for result in results {
        match result {
            GitPullResult::Success { src, stdout } => {
                ok += 1;
                jackin_diagnostics::active_debug(
                    "git_pull",
                    &format!("git pull in {src} succeeded: {}", stdout.trim()),
                );
            }
            GitPullResult::Failure { src, stderr } => {
                failed += 1;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("git_pull", &format!("git pull failed in {src}"));
                }
                jackin_diagnostics::active_debug(
                    "git_pull",
                    &format!("git pull in {src} failed: {}", stderr.trim()),
                );
            }
            GitPullResult::SpawnError { src, error } => {
                failed += 1;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact(
                        "git_pull",
                        &format!("could not run git pull in {src}: {error}"),
                    );
                }
            }
            GitPullResult::JoinError { src } => {
                failed += 1;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("git_pull", &format!("git pull thread panicked in {src}"));
                }
            }
        }
    }
    (ok, failed)
}
