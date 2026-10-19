use super::local_version_override;

#[test]
fn local_build_defaults_to_package_version() {
    assert_eq!(
        local_version_override(None, false, "0.6.0-dev").as_deref(),
        Some("0.6.0-dev")
    );
}

#[test]
fn ci_build_keeps_git_stamp_path() {
    assert_eq!(local_version_override(None, true, "0.6.0-dev"), None);
}

#[test]
fn explicit_override_wins_in_ci_and_local_builds() {
    assert_eq!(
        local_version_override(Some("custom".to_owned()), false, "0.6.0-dev").as_deref(),
        Some("custom")
    );
    assert_eq!(
        local_version_override(Some("custom".to_owned()), true, "0.6.0-dev").as_deref(),
        Some("custom")
    );
}

#[test]
fn nested_caller_watches_real_git_inputs_without_missing_packed_refs() -> TestResult {
    let temp = tempfile::tempdir()?;
    let root = temp.path().canonicalize()?;
    git(&root, &["init", "--initial-branch=main"])?;
    let caller = root.join("crates/apps/jackin");
    std::fs::create_dir_all(&caller)?;
    let paths = super::git_watch_paths(&caller);
    assert_eq!(paths, vec![root.join(".git/HEAD"), root.join(".git/refs")]);
    assert!(paths.iter().all(|path| path.exists()));
    assert!(!root.join("crates/.git").exists());
    Ok(())
}

#[test]
fn packed_refs_is_watched_only_when_present() -> TestResult {
    let temp = committed_repository()?;
    let root = temp.path().canonicalize()?;
    assert!(!root.join(".git/packed-refs").exists());
    git(&root, &["pack-refs", "--all", "--prune"])?;
    let paths = super::git_watch_paths(&root);
    assert_eq!(
        paths,
        vec![
            root.join(".git/HEAD"),
            root.join(".git/refs"),
            root.join(".git/packed-refs")
        ]
    );
    assert!(paths.iter().all(|path| path.exists()));
    Ok(())
}

#[test]
fn linked_worktree_uses_private_head_and_common_refs() -> TestResult {
    let temp = committed_repository()?;
    let root = temp.path().canonicalize()?;
    let linked = root.join("linked");
    git(&root, &["worktree", "add", "-b", "linked", "linked"])?;
    assert!(linked.join(".git").is_file());
    let caller = linked.join("crates/services/runtime");
    std::fs::create_dir_all(&caller)?;
    let paths = super::git_watch_paths(&caller);
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], root.join(".git/worktrees/linked/HEAD"));
    assert_eq!(paths[1], root.join(".git/refs"));
    assert!(paths.iter().all(|path| path.exists()));
    Ok(())
}

#[test]
fn detached_head_remains_watched_across_commit_changes() -> TestResult {
    let temp = committed_repository()?;
    let root = temp.path().canonicalize()?;
    git(&root, &["checkout", "--detach"])?;
    let paths = super::git_watch_paths(&root);
    let before = std::fs::read(&paths[0])?;
    git(
        &root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "next",
        ],
    )?;
    assert_eq!(super::git_watch_paths(&root), paths);
    assert_ne!(std::fs::read(&paths[0])?, before);
    Ok(())
}

#[test]
fn non_git_source_has_no_nonexistent_watch_inputs() -> TestResult {
    let temp = tempfile::tempdir()?;
    assert!(super::git_watch_paths(temp.path()).is_empty());
    Ok(())
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn committed_repository() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    git(temp.path(), &["init", "--initial-branch=main"])?;
    git(
        temp.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    )?;
    Ok(temp)
}

fn git(directory: &std::path::Path, args: &[&str]) -> TestResult {
    let output = jackin_process::exec_sync(
        &jackin_process::ExecRequest::new("git", args.iter().copied()).cwd(directory),
    )?;
    assert!(
        output.success,
        "git fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
