// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `repo_cache`.
use super::*;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_test_support::{FakeRunner, first_temp_role_repo, seed_valid_role_repo};
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn normalize_github_url_rewrites_scp_form() {
    assert_eq!(
        normalize_github_url("git@github.com:jackin-project/jackin.git"),
        "https://github.com/jackin-project/jackin.git"
    );
}

#[test]
fn normalize_github_url_rewrites_ssh_url_form() {
    assert_eq!(
        normalize_github_url("ssh://git@github.com/jackin-project/jackin.git"),
        "https://github.com/jackin-project/jackin.git"
    );
}

#[test]
fn normalize_github_url_passes_https_through_unchanged() {
    assert_eq!(
        normalize_github_url("https://github.com/jackin-project/jackin.git"),
        "https://github.com/jackin-project/jackin.git"
    );
}

#[test]
fn normalize_github_url_leaves_non_github_urls_alone() {
    // Non-GitHub SSH URLs must NOT be rewritten — substituting an
    // HTTPS URL would risk hitting an endpoint that doesn't exist
    // on the operator's SCM.
    assert_eq!(
        normalize_github_url("git@gitlab.example.com:team/repo.git"),
        "git@gitlab.example.com:team/repo.git"
    );
    assert_eq!(
        normalize_github_url("ssh://git@gitlab.example.com/team/repo.git"),
        "ssh://git@gitlab.example.com/team/repo.git"
    );
}

#[test]
fn normalize_github_url_handles_missing_git_suffix() {
    assert_eq!(
        normalize_github_url("git@github.com:jackin-project/jackin"),
        "https://github.com/jackin-project/jackin"
    );
    assert_eq!(
        normalize_github_url("ssh://git@github.com/jackin-project/jackin"),
        "https://github.com/jackin-project/jackin"
    );
}

#[test]
fn repo_matches_cross_protocol_for_same_owner_repo() {
    // After SSH→HTTPS normalize, a repo cloned years ago via SSH
    // and a config that now says HTTPS must agree at the
    // remote-URL match check.
    assert!(repo_matches(
        "https://github.com/jackin-project/jackin.git",
        "git@github.com:jackin-project/jackin.git"
    ));
    assert!(repo_matches(
        "git@github.com:jackin-project/jackin.git",
        "https://github.com/jackin-project/jackin.git"
    ));
}

#[test]
fn parse_repo_name_extracts_owner_repo_from_ssh_url() {
    assert_eq!(
        parse_repo_name("git@github.com:jackin-project/jackin.git"),
        Some("jackin-project/jackin".to_owned())
    );
}

#[test]
fn parse_repo_name_extracts_owner_repo_from_https_url() {
    assert_eq!(
        parse_repo_name("https://github.com/jackin-project/jackin.git"),
        Some("jackin-project/jackin".to_owned())
    );
}

#[test]
fn parse_repo_name_handles_url_without_git_suffix() {
    assert_eq!(
        parse_repo_name("https://github.com/jackin-project/jackin"),
        Some("jackin-project/jackin".to_owned())
    );
    assert_eq!(
        parse_repo_name("git@github.com:jackin-project/jackin"),
        Some("jackin-project/jackin".to_owned())
    );
}

#[tokio::test]
async fn resolve_agent_repo_rejects_cached_repo_with_wrong_remote() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner =
        FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_owned()]);
    let error = resolve_agent_repo(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        false,
        None,
    )
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("cached role repo remote mismatch")
    );
}

#[tokio::test]
async fn resolve_agent_repo_recovers_when_user_confirms_removal() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    // The capture queue provides: 1) the wrong remote URL, then 2) a
    // successful clone response (empty output).  After the user confirms,
    // the function removes the stale dir and re-clones.
    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:evil/agent-smith.git".to_owned(),
        String::new(), // clone output
    ]);

    // Simulate what `git clone` would produce on disk: recreate the repo
    // files when the clone command is captured by FakeRunner.
    let repo_dir_clone = repo_dir;
    runner.side_effects.push((
        "clone".to_owned(),
        Box::new(move || {
            std::fs::create_dir_all(repo_dir_clone.join(".git")).unwrap();
            std::fs::write(
                repo_dir_clone.join("Dockerfile"),
                "FROM projectjackin/construct:0.1-trixie\n",
            )
            .unwrap();
            std::fs::write(
                repo_dir_clone.join("jackin.role.toml"),
                r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
        RepoResolveOptions::interactive(false),
        || Ok(true), // user confirms removal
    )
    .await;

    result.expect("expected recovery to succeed");
    assert!(
        runner.recorded.iter().any(|c| c.contains("clone")),
        "expected a git clone after removal"
    );
}

#[tokio::test]
async fn resolve_agent_repo_aborts_when_user_declines_removal() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner =
        FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_owned()]);
    let error = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false),
        || Ok(false), // user declines
    )
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("cached role repo remote mismatch")
    );
    // The cached repo directory should still exist
    assert!(repo_dir.join(".git").is_dir());
}

#[tokio::test]
async fn resolve_agent_repo_rejects_cached_repo_with_local_changes() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        "?? scratch.txt".to_owned(),
    ]);
    let error = resolve_agent_repo(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        false,
        None,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("contains local changes"));
}

#[tokio::test]
async fn resolve_agent_repo_uses_run_for_clone_after_recovery() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner =
        FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_owned()]);
    let repo_dir_clone = repo_dir;
    runner.side_effects.push((
        "clone".to_owned(),
        Box::new(move || {
            std::fs::create_dir_all(repo_dir_clone.join(".git")).unwrap();
            std::fs::write(
                repo_dir_clone.join("Dockerfile"),
                "FROM projectjackin/construct:0.1-trixie\n",
            )
            .unwrap();
            std::fs::write(
                repo_dir_clone.join("jackin.role.toml"),
                r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
        RepoResolveOptions::interactive(false),
        || Ok(true),
    )
    .await;

    result.expect("expected recovery to succeed");
    assert!(runner.run_recorded.iter().any(|call| {
        call.contains("git clone https://github.com/jackin-project/jackin-agent-smith.git")
    }));
}

#[tokio::test]
async fn resolve_agent_repo_uses_run_for_pull_on_clean_repo() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),     // git status --porcelain (clean)
        "main".to_owned(), // git rev-parse --abbrev-ref HEAD
    ]);

    let result = resolve_agent_repo(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        false,
        None,
    )
    .await;

    result.expect("expected clean repo update to succeed");
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

#[tokio::test]
async fn resolve_agent_repo_skips_fetch_when_fetch_head_is_fresh() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    seed_valid_role_repo(&repo_dir);
    std::fs::write(repo_dir.join(".git/FETCH_HEAD"), "fresh\n").unwrap();

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
    ]);

    let result = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false).with_refresh_ttl(Duration::from_mins(1)),
        || Ok(false),
    )
    .await;

    result.expect("expected fresh cached repo to validate");
    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|call| call.contains("fetch origin")),
        "fresh FETCH_HEAD should skip fetch: {:?}",
        runner.run_recorded
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("rev-parse --abbrev-ref")),
        "fresh FETCH_HEAD should skip branch lookup: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn resolve_agent_repo_fetches_when_fetch_head_is_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    seed_valid_role_repo(&repo_dir);

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);

    let result = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false).with_refresh_ttl(Duration::from_mins(1)),
        || Ok(false),
    )
    .await;

    result.expect("expected missing FETCH_HEAD path to fetch");
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|call| call.contains("fetch origin main")),
        "missing FETCH_HEAD should fetch: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn resolve_agent_repo_fetches_when_refresh_ttl_is_zero() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    seed_valid_role_repo(&repo_dir);
    std::fs::write(repo_dir.join(".git/FETCH_HEAD"), "fresh\n").unwrap();

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);

    let result = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false).with_refresh_ttl(Duration::ZERO),
        || Ok(false),
    )
    .await;

    result.expect("expected zero TTL path to fetch");
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|call| call.contains("fetch origin main")),
        "zero TTL should fetch: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn resolve_agent_repo_fetches_when_fetch_head_is_expired() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    seed_valid_role_repo(&repo_dir);
    std::fs::write(repo_dir.join(".git/FETCH_HEAD"), "expired\n").unwrap();
    tokio::time::sleep(Duration::from_millis(2)).await;

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);

    let result = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false).with_refresh_ttl(Duration::from_nanos(1)),
        || Ok(false),
    )
    .await;

    result.expect("expected expired FETCH_HEAD path to fetch");
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|call| call.contains("fetch origin main")),
        "expired FETCH_HEAD should fetch: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn resolve_agent_repo_fetches_branch_override_without_branch_lookup() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::for_branch(&paths, &selector, "feature").repo_dir;
    seed_valid_role_repo(&repo_dir);
    std::fs::write(repo_dir.join(".git/FETCH_HEAD"), "fresh\n").unwrap();

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
    ]);

    let result = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false)
            .with_branch(Some("feature"))
            .with_refresh_ttl(Duration::from_mins(1)),
        || Ok(false),
    )
    .await;

    result.expect("expected branch override path to fetch");
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|call| call.contains("fetch origin feature")),
        "branch override should fetch despite fresh FETCH_HEAD: {:?}",
        runner.run_recorded
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("rev-parse --abbrev-ref")),
        "branch override should not ask git for HEAD branch: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn resolve_agent_repo_migrates_legacy_root_repo_to_default_sibling_layout() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let legacy_root = paths.roles_dir.join("agent-smith");
    seed_valid_role_repo(&legacy_root);
    std::fs::create_dir_all(legacy_root.join("branches/feat/caveman-all-install")).unwrap();
    std::fs::write(
        legacy_root.join("branches/feat/caveman-all-install/README.md"),
        "branch cache\n",
    )
    .unwrap();

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),     // git status --porcelain (clean)
        "main".to_owned(), // git rev-parse --abbrev-ref HEAD
    ]);

    let (cached_repo, _, _) = resolve_agent_repo(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        false,
        None,
    )
    .await
    .unwrap();

    assert_eq!(cached_repo.repo_dir, legacy_root.join("default"));
    assert!(legacy_root.join("default/.git").is_dir());
    assert!(!legacy_root.join(".git").exists());
    assert!(
        legacy_root
            .join("branches/feat/caveman-all-install/README.md")
            .is_file()
    );
}

#[tokio::test]
async fn register_agent_repo_cleans_up_temp_dir_on_validate_failure() {
    // When validation rejects the cloned repo (here: no Dockerfile,
    // no jackin.role.toml — just a `.git` dir), `register_agent_repo`
    // must NOT rename the temp dir into the cache and must NOT call
    // `persist_registration`. The temp dir is cleaned up by tempfile's
    // Drop, so the only assertion is that the cache slot is empty.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let selector = RoleSelector::new(None, "agent-broken");
    let cached_dir = CachedRepo::new(&paths, &selector).repo_dir;

    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        // Materialise a `.git` dir but skip the manifest files so
        // `validate_role_repo` rejects the clone.
        Box::new(move || {
            let temp_repo = first_temp_role_repo(&data_dir);
            std::fs::create_dir_all(temp_repo.join(".git")).unwrap();
        }),
    ));

    let err = register_agent_repo(
        &paths,
        &selector,
        "https://github.com/example/agent-broken.git",
        &mut runner,
        false,
    )
    .await
    .unwrap_err();

    assert!(
        err.downcast_ref::<RepoError>()
            .is_some_and(|e| matches!(e, RepoError::InvalidRoleRepo(_))),
        "expected RepoError::InvalidRoleRepo, got {err:?}"
    );
    assert!(
        !cached_dir.exists(),
        "cache slot must remain empty when validate fails: {}",
        cached_dir.display()
    );
}

#[tokio::test]
async fn register_agent_repo_installs_valid_repo_into_cache() {
    // The repo helper clones and validates into the cache, leaving
    // persistence to the caller.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let selector = RoleSelector::new(None, "agent-persist-ok");
    let cached_dir = CachedRepo::new(&paths, &selector).repo_dir;

    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || seed_valid_role_repo(&first_temp_role_repo(&data_dir))),
    ));

    let _repo = register_agent_repo(
        &paths,
        &selector,
        "https://github.com/example/agent-persist-ok.git",
        &mut runner,
        false,
    )
    .await
    .expect("repo registration should succeed");
    assert!(
        cached_dir.join(".git").is_dir(),
        "cache must be populated after successful registration",
    );
}

#[tokio::test]
async fn register_agent_repo_rejects_stale_non_git_directory() {
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
    )
    .await
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

#[test]
fn fetch_head_age_at_is_deterministic() {
    use jackin_core::ManualClock;
    let dir = tempdir().unwrap();
    let git = dir.path().join(".git");
    std::fs::create_dir_all(&git).unwrap();
    let fetch_head = git.join("FETCH_HEAD");
    std::fs::write(&fetch_head, "deadbeef\n").unwrap();
    let modified = std::fs::metadata(&fetch_head).unwrap().modified().unwrap();
    let clock = Arc::new(ManualClock::with_system_base(modified));
    clock.advance(Duration::from_mins(2));
    let age = fetch_head_age_at(dir.path(), clock.now_system()).expect("age");
    assert_eq!(age, Duration::from_mins(2));
    let options = RepoResolveOptions::interactive(false).with_clock(clock);
    assert_eq!(
        fetch_fresh_within_ttl_with_clock(dir.path(), Duration::from_mins(1), &*options.clock,),
        None,
        "stale beyond ttl"
    );
    assert_eq!(
        fetch_fresh_within_ttl_with_clock(dir.path(), Duration::from_mins(3), &*options.clock,),
        Some(Duration::from_mins(2)),
        "fresh within ttl"
    );
}
