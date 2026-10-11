// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
