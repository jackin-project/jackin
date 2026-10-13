// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn wipe_policy_erases_only_on_first_attach_and_resize() {
    // I4: no screen erase outside FirstAttach/Resize. Every other
    // invalidation relies on Ratatui's previous buffer instead of blanking
    // the screen.
    let erase = b"\x1b[2J";
    let contains = |frame: &[u8]| frame.windows(erase.len()).any(|w| w == erase);

    for reason in [FullRedrawReason::FirstAttach, FullRedrawReason::Resize] {
        let mut mux = single_pane_tab_mux_with_size(24, 80);
        let frame = compose_after(&mut mux, reason);
        assert!(contains(&frame), "{reason:?} frame must erase the screen");
    }
    for reason in [
        FullRedrawReason::ExplicitRedraw,
        FullRedrawReason::FocusChange,
        FullRedrawReason::TabSwitch,
        FullRedrawReason::SplitClose,
        FullRedrawReason::LayoutChange,
        FullRedrawReason::StatusChange,
        FullRedrawReason::ScrollbackMovement,
        FullRedrawReason::DialogChange,
        FullRedrawReason::PtyOutput,
    ] {
        let mut mux = single_pane_tab_mux_with_size(24, 80);
        drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
        let frame = compose_after(&mut mux, reason);
        assert!(
            !contains(&frame),
            "{reason:?} frame must repaint in place, not erase"
        );
    }
}

#[test]
fn pending_status_change_uses_no_clear_diff_frame() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.invalidate(status_change_redraw_reason());
    assert!(mux.has_pending_render());
    let frame = mux.compose_pending_frame();

    assert!(
        !frame_contains_screen_erase(&frame),
        "status-only refresh must stay out of the clear tier"
    );
    assert!(
        !mux.has_pending_render(),
        "pending diff redraw should be drained after composition"
    );
}

#[test]
fn resize_shrink_then_grow_does_not_panic() {
    // Defect 614/634 regression: rapid resize including shrink-to-floor and grow
    // must not panic.
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    // Shrink to a small size (above normalize_size floor which is ~5 rows, 3 cols).
    mux.resize(6, 4);
    assert_eq!((mux.render.term_rows, mux.render.term_cols), (6, 4));
    // Shrink to zero (normalized to defaults).
    mux.resize(0, 0);
    assert_eq!(
        (mux.render.term_rows, mux.render.term_cols),
        (DEFAULT_ROWS, DEFAULT_COLS)
    );
    // Grow back.
    mux.resize(50, 200);
    assert_eq!((mux.render.term_rows, mux.render.term_cols), (50, 200));
    // Full repaint after growth must not be empty.
    let frame = mux.compose_pending_frame();
    assert!(!frame.is_empty(), "grow must produce repaint");
}

#[test]
fn initial_spawn_request_is_data_only_agent_or_shell() {
    assert_eq!(
        initial_spawn_request("codex"),
        SpawnRequest::Instance("codex".to_owned())
    );
    assert_eq!(initial_spawn_request(""), SpawnRequest::Shell);
}

#[test]
fn spawn_request_rejects_agent_outside_allowlist_before_pty_spawn() {
    let mut mux = test_mux(24, 80);
    mux.launch_env.available_instances = vec!["codex".to_owned()];

    let err = mux
        .spawn_request(SpawnRequest::Instance("claude".to_owned()), &[])
        .unwrap_err();

    assert!(err.to_string().contains("rejected spawn target \"claude\""));
    assert!(mux.session_supervisor.sessions.is_empty());
}

#[test]
fn command_palette_labels_single_pane_close_as_close_tab() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CommandPalette {
            close_label: PaletteCloseLabel::CloseTab,
            ..
        })
    ));
}

#[test]
fn dialog_backdrop_preserves_product_status_brand() {
    fn mux_with_two_sessions() -> Multiplexer {
        let mut mux = split_tab_mux();
        let (session_one, _) = test_session(24, 80);
        let (session_two, _) = test_shell_session(24, 80);
        mux.session_supervisor.sessions.insert(1, session_one);
        mux.session_supervisor.sessions.insert(2, session_two);
        mux
    }

    fn assert_brand_preserved(mut mux: Multiplexer, context: &str) {
        let frame =
            String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::DialogChange))
                .to_string();

        // The brand pill renders as a green block with a black word and a white
        // chevron, so the cursor-diff stream splits `jackin` and `❯` with escape
        // codes. Assert the word plus the block colour rather than a contiguous
        // `jackin❯` substring.
        assert!(
            frame.contains("jackin") && frame.contains("48;2;0;255;65"),
            "{context} should preserve the top status brand (green block) while a dialog is open: {frame:?}"
        );
    }

    let mut menu_mux = mux_with_two_sessions();
    menu_mux.open_command_palette();
    assert_brand_preserved(menu_mux, "menu dialog");

    let mut container_mux = mux_with_two_sessions();
    container_mux.open_container_info_dialog();
    assert_brand_preserved(container_mux, "container info dialog");

    let mut github_mux = mux_with_two_sessions();
    github_mux.pr_watch.pull_request_context_branch = Some(branch("feat/capsule-pr-context-bar"));
    github_mux.pr_watch.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    github_mux.launch_env.workdir_context.gh_available = false;
    github_mux.open_github_context_dialog(Instant::now());
    assert_brand_preserved(github_mux, "GitHub context dialog");
}

#[test]
fn palette_close_single_pane_opens_confirm_directly() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = palette_command_frame(&mut mux, PaletteCommand::Close)
        .expect("single-pane close should redraw confirm dialog");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ConfirmAction {
            kind: ConfirmKind::CloseTab,
            selected_yes: false
        })
    ));
    assert!(
        !frame_contains_screen_erase(&frame),
        "single-pane close confirm must not clear the full terminal screen"
    );
}

#[test]
fn palette_close_split_tab_opens_target_picker() {
    let mut mux = split_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = palette_command_frame(&mut mux, PaletteCommand::Close)
        .expect("split-tab close should redraw target picker");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CloseTargetPicker {
            selected: 0,
            filter
        }) if filter.is_empty()
    ));
    assert!(
        !frame_contains_screen_erase(&frame),
        "split-tab close target picker must not clear the full terminal screen"
    );
}

#[test]
fn branch_context_visibility_keeps_content_area_reserved() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    // 24 rows - status(2) - top spacer(1) - hint(1) - bottom spacer(1) - branch(1) = 18
    assert_eq!(mux.render.content_rows, 18);

    mux.pr_watch.pull_request_context_cache.insert(
        branch("asa/pr-context"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(434))),
        },
    );
    assert!(mux.apply_git_branch_context(Some("asa/pr-context"), now));
    assert_eq!(mux.render.content_rows, 18);
    assert_eq!(
        mux.pr_watch
            .pull_request_context
            .as_deref()
            .map(|pr| pr.number),
        Some(434)
    );

    mux.pr_watch.pull_request_context_cache.insert(
        branch("feature/no-pr"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );
    assert!(mux.apply_git_branch_context(Some("feature/no-pr"), now));
    assert_eq!(mux.render.content_rows, 18);
    assert!(mux.pr_watch.pull_request_context.is_none());

    assert!(mux.apply_git_branch_context(Some("main"), now));
    assert_eq!(mux.render.content_rows, 18);
    assert!(mux.pr_watch.pull_request_context.is_none());
}

#[test]
fn git_branch_context_updates_status_before_github_lookup() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_context_branch = Some(branch("old/pr"));
    mux.pr_watch.pull_request_context = Some(Arc::new(pull_request_fixture(434)));
    mux.reconcile_content_rows();
    // 24 rows - status(2) - top spacer(1) - hint(1) - bottom spacer(1) - branch(1) = 18
    assert_eq!(mux.render.content_rows, 18);

    mux.pr_watch.pull_request_context_cache.insert(
        branch("new/local-branch"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );
    assert!(mux.apply_git_branch_context(Some("new/local-branch"), now));

    assert_eq!(
        mux.pr_watch.pull_request_context_branch.as_deref(),
        Some("new/local-branch")
    );
    assert!(mux.pr_watch.pull_request_context.is_none());
    assert_eq!(mux.render.content_rows, 18);
}

#[test]
fn git_branch_context_recognizes_repo_after_startup() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.launch_env.workdir_context.is_git_repo = false;
    mux.launch_env.workdir_context.gh_available = false;

    assert!(mux.apply_git_branch_context(Some("feat/capsule-pr-context-bar"), now));

    assert!(mux.launch_env.workdir_context.is_git_repo);
    assert_eq!(
        mux.context_bar_branch(),
        Some("feat/capsule-pr-context-bar")
    );
    assert!(mux.pr_watch.pull_request_context.is_none());
}

#[test]
fn apply_pull_request_context_loaded_drops_stale_request() {
    let mut mux = test_mux(24, 100);
    mux.pr_watch.pull_request_lookup.request_id = 5;
    mux.pr_watch.pull_request_lookup.in_flight = true;
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    let pr = pull_request_fixture(99);
    let changed = mux.apply_pull_request_context_loaded(
        3,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pr))),
        Instant::now(),
    );
    assert!(!changed, "stale request must not mutate state");
    assert!(
        mux.pr_watch.pull_request_lookup.in_flight,
        "stale request must leave in_flight untouched"
    );
    assert!(
        mux.pr_watch.pull_request_context.is_none(),
        "stale request must not write PR"
    );
}

#[test]
fn apply_pull_request_context_loaded_transient_failure_preserves_prior_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pr_watch.pull_request_lookup.request_id = 7;
    mux.pr_watch.pull_request_lookup.in_flight = true;
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    mux.pr_watch.pull_request_context = Some(Arc::new(pull_request_fixture(123)));
    mux.pr_watch.pull_request_context_cache.insert(
        branch("feat/x"),
        PullRequestContextCacheEntry {
            checked_at: now.checked_sub(Duration::from_secs(5)).unwrap(),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(123))),
        },
    );
    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::TransientFailure,
        now,
    );
    assert!(!changed, "transient failure must not mutate visible state");
    assert!(
        !mux.pr_watch.pull_request_lookup.in_flight,
        "transient failure must clear in_flight so next tick retries"
    );
    assert_eq!(
        mux.pr_watch
            .pull_request_context_cache
            .get("feat/x")
            .and_then(|e| e.pull_request.as_ref().map(|p| p.number)),
        Some(123),
        "cache must be untouched by transient failure"
    );
}

#[test]
fn apply_pull_request_context_loaded_refreshes_open_github_dialog() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    arm_pending_pr_lookup(&mut mux, "feat/x", 7);

    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(436)))),
        now,
    );

    assert!(changed, "dialog refresh should request redraw");
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::GitHubContext { copied: false, .. })
    ));
    assert_eq!(
        mux.pr_watch.pull_request_context_branch.as_deref(),
        Some("feat/x")
    );
    assert_eq!(
        mux.pr_watch
            .pull_request_context
            .as_ref()
            .map(|pr| pr.number),
        Some(436)
    );
    assert!(!mux.pull_request_context_loading());
}
