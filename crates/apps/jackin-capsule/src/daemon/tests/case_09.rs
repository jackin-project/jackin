// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn apply_git_context_head_change_schedules_fresh_pr_lookup() {
    // gh_available=true so the spawn path runs end-to-end; we assert
    // in_flight=true after the head flip to prove the maybe_spawn at
    // the tail of `apply_git_context` fires (not just request_id bump).
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.launch_env.workdir_context.gh_available = true;
    mux.launch_env.workdir_context.is_git_repo = true;
    mux.launch_env.workdir_context.default_branch = Some("main".to_owned());
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/a"));
    mux.pr_watch.pull_request_context_head = Some(oid('1'));

    mux.apply_git_context(
        GitContext::Branch {
            name: branch("feat/a"),
            head: Some(oid('2')),
        },
        now,
    );

    assert!(
        mux.pr_watch.pull_request_lookup.in_flight,
        "head flip must schedule a fresh gh worker via maybe_spawn"
    );
}

#[test]
fn apply_pull_request_context_loaded_refuses_head_mismatch() {
    // Defense-in-depth: request_id matched but mux.head drifted
    // between spawn and apply. The result MUST NOT overwrite
    // pull_request_context or land in the cache against the new head.
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_lookup.request_id = 9;
    mux.pr_watch.pull_request_lookup.in_flight = true;
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    mux.pr_watch.pull_request_context_head = Some(oid('a'));

    let changed = mux.apply_pull_request_context_loaded(
        9,
        Some(branch("feat/x")),
        Some(oid('b')),
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(777)))),
        now,
    );

    assert!(
        mux.pr_watch.pull_request_context.is_none(),
        "head-drift result must not be assigned to visible context"
    );
    assert!(
        !mux.pr_watch
            .pull_request_context_cache
            .contains_key("feat/x"),
        "head-drift result must not poison the cache"
    );
    assert!(
        !changed || mux.dialog_top().is_none(),
        "head-drift apply only flips loading state; no PR data assigned"
    );
}

#[test]
fn apply_pull_request_context_loaded_refuses_head_drift_none_to_some() {
    // Spawn-time head was None (e.g. mid-write HEAD), apply-time
    // mux.head resolved to Some. Drift guard must refuse the spawn
    // payload — its data is keyed against the absent-head state.
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_lookup.request_id = 11;
    mux.pr_watch.pull_request_lookup.in_flight = true;
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    mux.pr_watch.pull_request_context_head = Some(oid('c'));

    let _unused = mux.apply_pull_request_context_loaded(
        11,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(778)))),
        now,
    );

    assert!(
        mux.pr_watch.pull_request_context.is_none(),
        "None→Some head drift refused"
    );
    assert!(
        !mux.pr_watch
            .pull_request_context_cache
            .contains_key("feat/x")
    );
}

#[test]
fn apply_pull_request_context_loaded_refuses_head_drift_some_to_none() {
    // Inverse: spawn captured a head, apply-time mux.head was
    // cleared (e.g. HEAD became unreadable between spawn and apply).
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_lookup.request_id = 13;
    mux.pr_watch.pull_request_lookup.in_flight = true;
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    mux.pr_watch.pull_request_context_head = None;

    let _unused = mux.apply_pull_request_context_loaded(
        13,
        Some(branch("feat/x")),
        Some(oid('d')),
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(779)))),
        now,
    );

    assert!(
        mux.pr_watch.pull_request_context.is_none(),
        "Some→None head drift refused"
    );
    assert!(
        !mux.pr_watch
            .pull_request_context_cache
            .contains_key("feat/x")
    );
}

#[test]
fn apply_git_context_simultaneous_branch_and_head_change_invalidates_cache() {
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
            name: branch("feat/b"),
            head: Some(oid('2')),
        },
        now,
    );

    assert!(changed, "branch+head flip must dirty the visible context");
    assert_eq!(
        mux.pr_watch.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "simultaneous branch+head flip must bump request_id once"
    );
    assert_eq!(
        mux.pr_watch.pull_request_context_branch.as_deref(),
        Some("feat/b")
    );
    assert_eq!(
        mux.pr_watch.pull_request_context_head.as_deref(),
        Some("2222222222222222222222222222222222222222")
    );
    assert!(
        mux.pr_watch.pull_request_context.is_none(),
        "old PR cache entry under feat/a must not survive the branch flip"
    );
}

#[test]
fn read_branch_from_git_head_reads_normal_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();

    assert_eq!(
        read_branch_from_git_head(temp.path()).as_deref(),
        Some("feat/context")
    );
}

#[test]
fn read_context_from_git_metadata_reads_loose_head_oid() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(git_dir.join("refs/heads/feat")).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();
    std::fs::write(
        git_dir.join("refs/heads/feat/context"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();

    let context = read_context_from_git_metadata(temp.path()).unwrap();

    assert_eq!(
        context.branch_name().map(BranchName::as_str),
        Some("feat/context")
    );
    assert_eq!(
        context.head().map(Oid::as_str),
        Some("1111111111111111111111111111111111111111")
    );
}

#[test]
fn read_context_from_git_metadata_reads_packed_head_oid() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();
    std::fs::write(
        git_dir.join("packed-refs"),
        "\
# pack-refs with: peeled fully-peeled sorted
2222222222222222222222222222222222222222 refs/tags/v0.1.0
1111111111111111111111111111111111111111 refs/heads/feat/context
^3333333333333333333333333333333333333333
",
    )
    .unwrap();

    let context = read_context_from_git_metadata(temp.path()).unwrap();

    assert_eq!(
        context.branch_name().map(BranchName::as_str),
        Some("feat/context")
    );
    assert_eq!(
        context.head().map(Oid::as_str),
        Some("1111111111111111111111111111111111111111")
    );
}

#[test]
fn read_packed_git_ref_oid_refreshes_after_metadata_change() {
    let temp = tempfile::tempdir().unwrap();
    let packed_refs = temp.path().join("packed-refs");
    std::fs::write(
        &packed_refs,
        "1111111111111111111111111111111111111111 refs/heads/feat/context\n",
    )
    .unwrap();

    assert_eq!(
        read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/context").as_deref(),
        Some("1111111111111111111111111111111111111111")
    );

    std::fs::write(
        &packed_refs,
        "\
# changed
2222222222222222222222222222222222222222 refs/heads/feat/context
",
    )
    .unwrap();

    assert_eq!(
        read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/context").as_deref(),
        Some("2222222222222222222222222222222222222222")
    );
}

#[test]
fn workdir_context_recognizes_direct_git_metadata_without_default_branch() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();

    let context = WorkdirContext::resolve(temp.path());

    assert!(context.is_git_repo);
}

#[test]
fn read_branch_from_git_head_reads_worktree_gitdir_file() {
    let temp = tempfile::tempdir().unwrap();
    let (workdir, common_git) = make_worktree_layout(temp.path(), "workdir");
    let wt_git = common_git.join("worktrees/workdir");
    std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feat/worktree\n").unwrap();

    assert_eq!(
        read_branch_from_git_head(&workdir).as_deref(),
        Some("feat/worktree")
    );
}

#[test]
fn oid_parse_accepts_sha1_and_sha256_lengths_only() {
    assert!(Oid::parse(&"a".repeat(40)).is_some());
    assert!(Oid::parse(&"F".repeat(40)).is_some());
    assert!(Oid::parse(&"0".repeat(64)).is_some());
    assert!(Oid::parse(&"f".repeat(64)).is_some());
    assert!(Oid::parse(&"a".repeat(39)).is_none());
    assert!(Oid::parse(&"a".repeat(41)).is_none());
    assert!(Oid::parse(&"a".repeat(63)).is_none());
    assert!(Oid::parse(&"a".repeat(65)).is_none());
    // Non-hex character at SHA-1 length.
    let mut s = "a".repeat(39);
    s.push('g');
    assert!(Oid::parse(&s).is_none());
}

#[test]
fn read_context_from_git_metadata_reads_detached_head_oid() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(
        git_dir.join("HEAD"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();

    let context = read_context_from_git_metadata(temp.path()).unwrap();

    assert_eq!(context.branch_name(), None);
    assert_eq!(
        context.head().map(Oid::as_str),
        Some("1111111111111111111111111111111111111111")
    );
}

#[test]
fn read_context_from_git_metadata_handles_malformed_head_content() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    // Neither `ref: ` prefix nor full hex OID — corruption / mid-write.
    std::fs::write(git_dir.join("HEAD"), "abc123\n").unwrap();

    let context = read_context_from_git_metadata(temp.path()).unwrap();

    assert_eq!(context.branch_name(), None);
    assert_eq!(
        context.head(),
        None,
        "malformed HEAD content must not be treated as an OID"
    );
}

#[test]
fn read_context_from_git_metadata_handles_malformed_gitfile_content() {
    let temp = tempfile::tempdir().unwrap();
    let workdir = temp.path();
    // `.git` is a file but does not start with `gitdir:` — corruption.
    std::fs::write(workdir.join(".git"), "not a gitdir pointer\n").unwrap();

    assert!(read_context_from_git_metadata(workdir).is_none());
}

#[test]
fn apply_git_context_flips_is_git_repo_on_detached_head() {
    let mut mux = test_mux(24, 100);
    mux.launch_env.workdir_context.is_git_repo = false;
    let now = Instant::now();

    mux.apply_git_context(GitContext::Detached { head: oid('1') }, now);

    assert!(
        mux.launch_env.workdir_context.is_git_repo,
        "detached HEAD must promote is_git_repo (branch is None but head is Some)"
    );
}

#[test]
fn read_context_from_git_metadata_resolves_worktree_head_via_commondir() {
    let temp = tempfile::tempdir().unwrap();
    let (workdir, common_git) = make_worktree_layout(temp.path(), "wt");
    let wt_git = common_git.join("worktrees/wt");
    std::fs::create_dir_all(common_git.join("refs/heads/feat")).unwrap();
    // Loose ref lives in the COMMON dir, not the per-worktree gitdir.
    std::fs::write(
        common_git.join("refs/heads/feat/wt"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();
    std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feat/wt\n").unwrap();
    std::fs::write(wt_git.join("commondir"), "../..\n").unwrap();

    let context = read_context_from_git_metadata(&workdir).unwrap();

    assert_eq!(context.branch_name(), Some(&branch("feat/wt")));
    assert_eq!(context.head(), Some(&oid('1')));
}
