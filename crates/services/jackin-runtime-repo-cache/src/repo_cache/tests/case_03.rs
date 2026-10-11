// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn repo_lock_reacquires_in_the_same_process_without_deadlocking() {
    let dir = tempdir().unwrap();
    let lock_path = dir.path().join("the-architect.locks/default.repo.lock");
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();

    let first = acquire_repo_lock_blocking(&lock_path).expect("first acquisition");
    let second = acquire_repo_lock_blocking(&lock_path).expect("re-acquisition must not block");
    assert!(
        Arc::ptr_eq(&first.0, &second.0),
        "a same-process re-acquire must share the first open file description"
    );

    // Dropping every handle releases the flock and clears the registry entry,
    // so the next launch in this process takes a fresh lock rather than
    // reviving a dead one.
    drop(second);
    drop(first);
    let third = acquire_repo_lock_blocking(&lock_path).expect("acquisition after release");
    drop(third);
}

#[test]
fn repo_lock_paths_are_independent() {
    let dir = tempdir().unwrap();
    let one = dir.path().join("one.repo.lock");
    let two = dir.path().join("two.repo.lock");
    let held_one = acquire_repo_lock_blocking(&one).expect("first role");
    let held_two = acquire_repo_lock_blocking(&two).expect("second role");
    assert!(!Arc::ptr_eq(&held_one.0, &held_two.0));
}
