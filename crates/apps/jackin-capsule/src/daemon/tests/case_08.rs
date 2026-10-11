// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn transient_pull_request_failure_clears_open_dialog_loading_state() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    arm_pending_pr_lookup(&mut mux, "feat/x", 7);

    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::TransientFailure,
        now,
    );

    assert!(changed, "dialog loading state changed");
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::GitHubContext { copied: false, .. })
    ));
    assert_eq!(
        mux.pr_watch.pull_request_context_branch.as_deref(),
        Some("feat/x")
    );
    assert!(mux.pr_watch.pull_request_context.is_none());
    assert!(!mux.pull_request_context_loading());
    assert!(
        !mux.pr_watch
            .pull_request_context_cache
            .contains_key("feat/x"),
        "transient failure must not cache a no-PR result"
    );
}

#[test]
fn apply_git_branch_context_loaded_drops_stale_request() {
    let mut mux = test_mux(24, 100);
    mux.pr_watch.git_branch_lookup.request_id = 4;
    mux.pr_watch.git_branch_lookup.in_flight = true;
    let changed = mux.apply_git_branch_context_loaded(
        2,
        GitContext::Branch {
            name: branch("feat/x"),
            head: None,
        },
        Instant::now(),
    );
    assert!(!changed);
    assert!(
        mux.pr_watch.git_branch_lookup.in_flight,
        "stale id leaves in_flight"
    );
    assert!(mux.pr_watch.pull_request_context_branch.is_none());
}

#[test]
fn apply_git_branch_context_bumps_pr_request_id_on_branch_change() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/a"));
    mux.launch_env.workdir_context.gh_available = false;
    let id_before = mux.pr_watch.pull_request_lookup.request_id;
    let _ = mux.apply_git_branch_context(Some("feat/b"), now);
    assert_eq!(
        mux.pr_watch.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "branch change must bump request_id so stale gh worker responses are rejected"
    );
}

#[test]
fn apply_git_context_bumps_pr_request_id_on_same_branch_head_change() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/a"));
    mux.pr_watch.pull_request_context_head = Some(oid('1'));
    mux.pr_watch.pull_request_context = Some(Arc::new(pull_request_fixture(455)));
    mux.pr_watch.pull_request_context_cache.insert(
        branch("feat/a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: Some(oid('1')),
            pull_request: Some(Arc::new(pull_request_fixture(455))),
        },
    );
    mux.launch_env.workdir_context.gh_available = false;
    let id_before = mux.pr_watch.pull_request_lookup.request_id;

    let changed = mux.apply_git_context(
        GitContext::Branch {
            name: branch("feat/a"),
            head: Some(oid('2')),
        },
        now,
    );

    assert!(
        changed,
        "visible PR context must clear on same-branch HEAD change"
    );
    assert_eq!(
        mux.pr_watch.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "HEAD change must bump request_id so stale gh worker responses are rejected"
    );
    assert!(
        mux.pr_watch.pull_request_context.is_none(),
        "old PR cache must not stay visible for the new HEAD"
    );
}

#[test]
fn purge_expired_pull_request_cache_entries_drops_old_entries() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    let ttl = PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL * 2;
    mux.pr_watch.pull_request_context_cache.insert(
        branch("feat/fresh"),
        PullRequestContextCacheEntry {
            checked_at: now.checked_sub(Duration::from_secs(10)).unwrap(),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(1))),
        },
    );
    mux.pr_watch.pull_request_context_cache.insert(
        branch("feat/old"),
        PullRequestContextCacheEntry {
            checked_at: now
                .checked_sub(ttl)
                .unwrap()
                .checked_sub(Duration::from_secs(1))
                .unwrap(),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(2))),
        },
    );
    mux.purge_expired_pull_request_cache_entries(now);
    assert!(
        mux.pr_watch
            .pull_request_context_cache
            .contains_key("feat/fresh")
    );
    assert!(
        !mux.pr_watch
            .pull_request_context_cache
            .contains_key("feat/old")
    );
}

#[test]
fn pull_request_cache_fresh_at_strict_boundary() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    // Just-fresh: at the boundary minus 1 ms.
    mux.pr_watch.pull_request_context_cache.insert(
        branch("branch-a"),
        PullRequestContextCacheEntry {
            checked_at: now
                .checked_sub(PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL)
                .unwrap()
                + Duration::from_millis(1),
            head: None,
            pull_request: None,
        },
    );
    // Just-stale: at the boundary plus 1 ms.
    mux.pr_watch.pull_request_context_cache.insert(
        branch("branch-b"),
        PullRequestContextCacheEntry {
            checked_at: now
                .checked_sub(PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL)
                .unwrap()
                .checked_sub(Duration::from_millis(1))
                .unwrap(),
            head: None,
            pull_request: None,
        },
    );
    assert!(mux.pull_request_cache_is_fresh("branch-a", now));
    assert!(!mux.pull_request_cache_is_fresh("branch-b", now));
}

#[test]
fn pull_request_cache_fresh_requires_matching_head() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_context_head = Some(oid('2'));
    mux.pr_watch.pull_request_context_cache.insert(
        branch("branch-a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: Some(oid('1')),
            pull_request: None,
        },
    );

    assert!(
        !mux.pull_request_cache_is_fresh("branch-a", now),
        "a cached no-PR answer from an older HEAD must not suppress a fresh lookup"
    );
}

#[test]
fn pull_request_force_refresh_bypasses_fresh_no_pr_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_context_cache.insert(
        branch("branch-a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );

    assert!(mux.pull_request_cache_blocks_lookup(
        "branch-a",
        now,
        PullRequestLookupMode::RespectCache
    ));
    assert!(!mux.pull_request_cache_blocks_lookup(
        "branch-a",
        now,
        PullRequestLookupMode::ForceRefresh
    ));
}

#[test]
fn git_branch_context_keeps_current_pr_while_refreshing_same_branch() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_context_branch = Some(branch("feature/current"));
    mux.pr_watch.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    mux.pr_watch.pull_request_lookup.in_flight = true;
    mux.pr_watch.pull_request_context_cache.insert(
        branch("feature/current"),
        PullRequestContextCacheEntry {
            checked_at: now
                .checked_sub(PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL)
                .unwrap(),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(436))),
        },
    );

    assert!(!mux.apply_git_branch_context(Some("feature/current"), now));
    assert_eq!(
        mux.pr_watch
            .pull_request_context
            .as_deref()
            .map(|pr| pr.number),
        Some(436)
    );
}

#[test]
fn cached_pull_request_stays_visible_during_forced_dialog_refresh() {
    let mut mux = test_mux(24, 100);
    mux.pr_watch.pull_request_context_branch = Some(branch("feature/current"));
    mux.pr_watch.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    mux.pr_watch.pull_request_lookup.in_flight = true;
    // Exercise the real dialog-open path so a future refactor that
    // skips force_spawn (or routes through a different dispatcher)
    // is caught here instead of by silent UX regression.
    mux.launch_env.workdir_context.gh_available = false;
    mux.open_github_context_dialog(Instant::now());

    let view = mux.github_context_view();

    assert!(matches!(
        view.status,
        PullRequestStatus::Loaded(pr) if pr.number == 436
    ));
    assert!(
        !mux.pull_request_context_loading(),
        "known PR details should remain visible while a forced refresh runs in the background"
    );
}

#[test]
fn open_github_context_dialog_force_spawns_when_gh_available() {
    let mut mux = test_mux(24, 100);
    mux.launch_env.workdir_context.gh_available = true;
    mux.launch_env.workdir_context.is_git_repo = true;
    mux.launch_env.workdir_context.default_branch = Some("main".to_owned());
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    let id_before = mux.pr_watch.pull_request_lookup.request_id;

    mux.open_github_context_dialog(Instant::now());

    assert!(
        mux.pr_watch.pull_request_lookup.in_flight,
        "dialog-open must fire a real worker spawn when gh_available is true"
    );
    assert_eq!(
        mux.pr_watch.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "force-spawn must bump request_id"
    );
}

#[test]
fn open_github_context_dialog_force_spawns_when_startup_missed_gh() {
    let mut mux = test_mux(24, 100);
    mux.launch_env.workdir_context.gh_available = false;
    mux.launch_env.workdir_context.is_git_repo = true;
    mux.launch_env.workdir_context.default_branch = Some("main".to_owned());
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    let id_before = mux.pr_watch.pull_request_lookup.request_id;

    mux.open_github_context_dialog(Instant::now());

    assert!(
        mux.pr_watch.pull_request_lookup.in_flight,
        "manual refresh must schedule a background lookup even when startup marked gh unavailable"
    );
    assert_eq!(
        mux.pr_watch.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "manual refresh should not need a synchronous gh availability probe"
    );
    assert!(
        !mux.launch_env.workdir_context.gh_available,
        "gh availability flips only after the background lookup succeeds"
    );
}

#[test]
fn background_pull_request_success_marks_gh_available_after_startup_miss() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.launch_env.workdir_context.gh_available = false;
    mux.launch_env.workdir_context.is_git_repo = true;
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    mux.pr_watch.pull_request_lookup.request_id = 7;
    mux.pr_watch.pull_request_lookup.in_flight = true;

    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(436)))),
        now,
    );

    assert!(changed);
    assert!(
        mux.launch_env.workdir_context.gh_available,
        "successful background gh lookup should unblock later conservative refreshes"
    );
}

#[test]
fn open_github_context_dialog_bypasses_fresh_no_pr_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.launch_env.workdir_context.gh_available = true;
    mux.launch_env.workdir_context.is_git_repo = true;
    mux.launch_env.workdir_context.default_branch = Some("main".to_owned());
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    mux.pr_watch.pull_request_context_cache.insert(
        branch("feat/x"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );

    mux.open_github_context_dialog(now);

    assert!(
        mux.pr_watch.pull_request_lookup.in_flight,
        "manual dialog open must refresh even when a recent background lookup saw no PR"
    );
    assert!(
        mux.pull_request_context_loading(),
        "dialog should show resolving while the forced refresh is in flight"
    );
}
