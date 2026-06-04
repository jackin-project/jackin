//! Unit tests for `jackin-capsule` daemon: input dispatch, session management,
//! tab lifecycle, git context, status-bar rendering, and PTY session behavior.
use super::*;
use std::sync::{Arc, Mutex};

use crate::pr_context::{command_output_or_lookup_error, command_stdout_trimmed};
use crate::tui::components::dialog::PullRequestStatus;
use portable_pty::{ChildKiller, MasterPty, PtySize};

#[derive(Debug)]
struct NullChildKiller;

impl ChildKiller for NullChildKiller {
    fn kill(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self)
    }
}

struct NullMasterPty;

impl MasterPty for NullMasterPty {
    fn resize(&self, _size: PtySize) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_size(&self) -> anyhow::Result<PtySize> {
        Ok(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
    }

    fn try_clone_reader(&self) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
        Ok(Box::new(std::io::empty()))
    }

    fn take_writer(&self) -> anyhow::Result<Box<dyn std::io::Write + Send>> {
        Ok(Box::new(std::io::sink()))
    }

    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<nix::libc::pid_t> {
        None
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
        None
    }

    #[cfg(unix)]
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        None
    }
}

#[test]
fn spawn_failure_banner_wraps_in_save_restore_and_carries_reason() {
    let bytes = spawn_failure_banner("boom: agent slug rejected");
    assert!(bytes.starts_with(b"\x1b7\x1b[1;1H"));
    assert!(bytes.ends_with(b"\x1b8"));
    assert!(
        bytes
            .windows(b"boom: agent slug rejected".len())
            .any(|w| w == b"boom: agent slug rejected"),
        "reason missing from banner: {:?}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(
        bytes.windows(2).any(|w| w == b"\x1b["),
        "missing SGR opener"
    );
}

fn test_mux(rows: u16, cols: u16) -> Multiplexer {
    Multiplexer::new(
        rows,
        cols,
        CapsuleConfig {
            role: "test-role".to_string(),
            workdir: "/workspace".to_string(),
            agents: Vec::new(),
            models: std::collections::BTreeMap::new(),
            initial_provider: None,
        },
    )
}

fn single_pane_tab_mux() -> Multiplexer {
    single_pane_tab_mux_with_size(24, 80)
}

fn single_pane_tab_mux_with_size(rows: u16, cols: u16) -> Multiplexer {
    let mut mux = test_mux(24, 80);
    mux.resize(rows, cols);
    mux.tabs.push(Tab::new_single("Shell", 1));
    mux
}

fn pull_request_fixture(number: u64) -> PullRequestInfo {
    PullRequestInfo {
        number,
        title: "Surface PR context in Capsule".to_string(),
        url: format!("https://github.com/jackin-project/jackin/pull/{number}"),
        is_draft: false,
        checks: None,
    }
}

/// Build a 40-char SHA-1-shaped OID from a single hex nibble
/// repeated 40 times. Tests want distinguishable OIDs ("H1", "H2",
/// "H3") without the eye-strain of typing 40 hex digits inline.
fn oid(nibble: char) -> Oid {
    assert!(nibble.is_ascii_hexdigit(), "nibble must be 0-9/a-f");
    Oid::parse(&nibble.to_string().repeat(40)).expect("40 hex chars is a valid Oid")
}

fn branch(name: &str) -> BranchName {
    BranchName::parse(name).expect("test branch names must parse")
}

/// Lay out a fake worktree under `temp` and return the
/// (`workdir`, `common_git_dir`) paths the test can then write
/// into. The `workdir/.git` pointer file is written so
/// `read_context_from_git_metadata` discovers the per-worktree
/// gitdir; the caller is responsible for writing HEAD + any
/// `commondir` / `refs/heads/*` ref files specific to the
/// scenario under test.
fn make_worktree_layout(temp: &Path, worktree_name: &str) -> (PathBuf, PathBuf) {
    let workdir = temp.join("workdir");
    let common_git = temp.join("repo/.git");
    let wt_git = common_git.join(format!("worktrees/{worktree_name}"));
    std::fs::create_dir_all(&workdir).unwrap();
    std::fs::create_dir_all(&wt_git).unwrap();
    std::fs::write(
        workdir.join(".git"),
        format!("gitdir: {}\n", wt_git.display()),
    )
    .unwrap();
    (workdir, common_git)
}

/// Construct the state production would land in after
/// `maybe_spawn_pull_request_context_lookup` actually spawned a
/// worker for `branch` (without shelling out to `gh`):
/// `request_id` is the id the worker carries, `in_flight = true`
/// gates the next spawn, `pull_request_context_branch` is the
/// branch the worker was started for, and a `GitHubContext`
/// dialog is open so apply-path redraw decisions exercise the
/// dialog-open code path.
fn arm_pending_pr_lookup(mux: &mut Multiplexer, branch_name: &str, request_id: u64) {
    mux.pull_request_lookup.request_id = request_id;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch(branch_name));
    mux.open_github_context_dialog(Instant::now());
}

#[test]
fn outer_terminal_title_uses_workspace_and_pr_title() {
    let title = compose_outer_terminal_title(
        Path::new("/Users/operator/Projects/jackin"),
        Some("feat/capsule-pr-context-bar"),
        Some(&pull_request_fixture(436)),
    );

    assert_eq!(title, "jackin · PR #436 · Surface PR context in Capsule");
}

#[test]
fn outer_terminal_title_falls_back_to_branch_without_pr() {
    let title = compose_outer_terminal_title(
        Path::new("/Users/operator/Projects/jackin"),
        Some("feat/capsule-pr-context-bar"),
        None,
    );

    assert_eq!(title, "jackin · feat/capsule-pr-context-bar");
}

#[test]
fn outer_terminal_title_sanitizes_control_bytes() {
    let pull_request = PullRequestInfo {
        number: 436,
        title: "bad\x1b]2;owned\x07title".to_string(),
        url: "https://github.com/jackin-project/jackin/pull/436".to_string(),
        is_draft: false,
        checks: None,
    };
    let title =
        compose_outer_terminal_title(Path::new("/workspace/jackin"), None, Some(&pull_request));

    assert_eq!(title, "jackin · PR #436 · bad ]2;owned title");
}

#[test]
fn display_title_falls_back_when_shell_sets_empty_title() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]2;\x07");

    assert_eq!(session_display_title(&session), "Test");
}

#[test]
fn display_title_uses_shell_title_without_repeating_shell_label() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]2;prompt title\x07");

    assert_eq!(session_display_title(&session), "prompt title");
}

#[test]
fn display_title_uses_shell_cwd_without_repeating_shell_label() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]7;file:///workspace/project\x07");

    assert_eq!(session_display_title(&session), "/workspace/project");
}

#[test]
fn full_frame_emits_outer_terminal_title_once_until_context_changes() {
    let mut mux = single_pane_tab_mux();
    mux.workdir = PathBuf::from("/workspace/jackin");
    mux.pull_request_context_branch = Some(branch("feat/capsule-pr-context-bar"));

    let first = String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
        .to_string();
    assert!(
        first.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
        "first frame should set branch title: {first:?}"
    );

    let second = String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
        .to_string();
    assert!(
        !second.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
        "unchanged full frame should not spam title: {second:?}"
    );

    mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    let updated =
        String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        updated.contains("\x1b]2;jackin · PR #436 · Surface PR context in Capsule\x1b\\"),
        "PR context change should refresh title: {updated:?}"
    );
}

#[test]
fn full_frame_updates_outer_terminal_title_on_branch_switch() {
    let mut mux = single_pane_tab_mux();
    mux.workdir = PathBuf::from("/workspace/jackin");
    mux.workdir_context.default_branch = Some("main".to_string());
    mux.pull_request_context_branch = Some(branch("feat/a"));

    let first = String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
        .to_string();
    assert!(
        first.contains("\x1b]2;jackin · feat/a\x1b\\"),
        "first non-default branch should set title: {first:?}"
    );

    mux.pull_request_context_branch = Some(branch("feat/b"));
    let switched =
        String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        switched.contains("\x1b]2;jackin · feat/b\x1b\\"),
        "branch switch should refresh title: {switched:?}"
    );

    mux.pull_request_context_branch = Some(branch("main"));
    let default_branch =
        String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        default_branch.contains("\x1b]2;jackin\x1b\\"),
        "default branch should fall back to workspace-only title: {default_branch:?}"
    );
    assert!(
        !default_branch.contains("jackin · main"),
        "default branch name should not be propagated into title: {default_branch:?}"
    );
}

fn test_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, Some("codex".to_string()))
}

fn test_shell_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, None)
}

fn pane_kind_cases() -> [(Option<&'static str>, &'static str); 2] {
    [(Some("codex"), "agent"), (None, "shell")]
}

fn test_pane_session(
    rows: u16,
    cols: u16,
    agent: Option<&str>,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, agent.map(ToString::to_string))
}

fn assert_focused_scroll_chrome(frame: &[u8], context: &str) {
    let rendered = String::from_utf8_lossy(frame);
    let focused_scroll_fg = format!(
        "{}{}",
        jackin_tui::ansi::RESET,
        jackin_tui::ansi::rgb_fg(jackin_tui::PHOSPHOR_GREEN)
    );
    assert!(
        rendered.contains(&focused_scroll_fg),
        "focused {context} should use green chrome"
    );
    assert!(
        rendered.contains('█'),
        "focused {context} should draw a scrollbar thumb"
    );
}

fn assert_no_scroll_thumb(frame: &[u8], context: &str) {
    assert!(
        !String::from_utf8_lossy(frame).contains('█'),
        "{context} should not draw fake scrollback chrome"
    );
}

fn assert_wheel_cursor_fallback_sent(
    input_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    expected_bytes: &[u8],
) {
    assert_eq!(
        input_rx
            .try_recv()
            .expect("wheel fallback should reach PTY"),
        expected_bytes,
    );
    assert!(
        input_rx.try_recv().is_err(),
        "wheel should not produce extra PTY input"
    );
}

fn feed_top_anchored_inline_history(session: &mut Session, region_bottom: u16, lines: usize) {
    session.feed_pty(format!("\x1b[1;{region_bottom}r\x1b[{region_bottom};1H").as_bytes());
    for i in 0..lines {
        session.feed_pty(format!("\r\n\x1b[2Khistory {i}").as_bytes());
    }
    session.feed_pty(b"\x1b[r");
}

fn test_session_with_agent(
    rows: u16,
    cols: u16,
    agent: Option<String>,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    (
        Session::new_for_test(
            "Test".to_string(),
            agent,
            None,
            (rows, cols),
            100,
            input_tx,
            Arc::new(Mutex::new(Box::new(NullMasterPty))),
            Arc::new(Mutex::new(Box::new(NullChildKiller))),
        ),
        input_rx,
    )
}

fn test_provider_session(
    provider: jackin_protocol::Provider,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (mut session, input_rx) = test_session_with_agent(24, 80, Some("claude".to_string()));
    session.provider = Some(crate::session::SessionProvider {
        label: provider.label().to_string(),
        env_overrides: provider.env_overrides(Some("zai-test-token")),
    });
    (session, input_rx)
}

#[test]
fn refresh_tab_labels_preserves_provider_suffix() {
    let mut mux = test_mux(24, 80);
    let (session, _rx) = test_provider_session(jackin_protocol::Provider::Zai);
    mux.sessions.insert(1, session);
    mux.tabs.push(Tab::new_single("Claude", 1));

    mux.refresh_tab_labels();

    assert_eq!(mux.tabs[0].label(), "Claude (Z.AI)");
}

#[test]
fn split_metadata_inherits_focused_provider() {
    let mut mux = test_mux(24, 80);
    let (session, _rx) = test_provider_session(jackin_protocol::Provider::Zai);
    let expected_env = session
        .provider
        .as_ref()
        .map(|p| p.env_overrides.clone())
        .unwrap_or_default();
    mux.sessions.insert(1, session);
    mux.tabs.push(Tab::new_single("Claude (Z.AI)", 1));

    let (agent, env, provider) = mux.focused_spawn_metadata();

    assert_eq!(agent.as_deref(), Some("claude"));
    assert_eq!(provider.as_deref(), Some("Z.AI"));
    assert_eq!(env, expected_env);
}

fn split_tab_mux() -> Multiplexer {
    let mut mux = test_mux(24, 80);
    let mut tab = Tab::new_single("Shell", 1);
    assert!(tab.tree.split_h(1, 2, SplitPosition::After));
    mux.tabs.push(tab);
    mux
}

#[test]
fn resize_zero_zero_normalizes_to_default_dimensions() {
    // A client sending Resize { rows: 0, cols: 0 } is asking for
    // "use the defaults"; the daemon must floor through
    // `normalize_size` and never store 0 in `term_rows`/`term_cols`,
    // because zero-row PTYs collapse grid rendering.
    let mut mux = test_mux(48, 160);
    mux.resize(0, 0);
    assert_eq!((mux.term_rows, mux.term_cols), (DEFAULT_ROWS, DEFAULT_COLS));
}

#[test]
fn resize_then_full_frame_repaints_with_new_geometry() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    assert!(
        !mux.compose_full_frame(FullRedrawReason::FirstAttach)
            .is_empty()
    );

    mux.resize(30, 100);
    let frame = mux.compose_full_frame(FullRedrawReason::Resize);

    assert_eq!((mux.term_rows, mux.term_cols), (30, 100));
    assert!(
        !frame.is_empty(),
        "resize must produce a repaint for the attach client"
    );
}

#[test]
fn resize_shrink_then_grow_does_not_panic() {
    // Defect 614/634 regression: rapid resize including shrink-to-floor and grow
    // must not panic. Also verifies cache invalidation on each resize.
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    // Shrink to a small size (above normalize_size floor which is ~5 rows, 3 cols).
    mux.resize(6, 4);
    assert_eq!((mux.term_rows, mux.term_cols), (6, 4));
    assert!(mux.pane_body_caches.is_empty(), "caches cleared on resize");
    // Shrink to zero (normalized to defaults).
    mux.resize(0, 0);
    assert_eq!((mux.term_rows, mux.term_cols), (DEFAULT_ROWS, DEFAULT_COLS));
    // Grow back.
    mux.resize(50, 200);
    assert_eq!((mux.term_rows, mux.term_cols), (50, 200));
    // Full repaint after growth must not be empty.
    let frame = mux.compose_full_frame(FullRedrawReason::Resize);
    assert!(!frame.is_empty(), "grow must produce repaint");
}

#[test]
fn initial_spawn_request_is_data_only_agent_or_shell() {
    assert_eq!(
        initial_spawn_request("codex", None),
        SpawnRequest::Agent("codex".to_string())
    );
    assert_eq!(initial_spawn_request("", None), SpawnRequest::Shell);
}

#[test]
fn initial_spawn_request_carries_provider_when_selected() {
    let provider = jackin_protocol::InitialProvider {
        label: jackin_protocol::Provider::Zai.label().to_string(),
    };
    assert_eq!(
        initial_spawn_request("claude", Some(&provider)),
        SpawnRequest::AgentWithProvider {
            slug: "claude".to_string(),
            provider_label: "Z.AI".to_string(),
        }
    );
    // An empty agent still degrades to a shell even with a provider.
    assert_eq!(
        initial_spawn_request("", Some(&provider)),
        SpawnRequest::Shell
    );
}

#[test]
fn spawn_request_rejects_agent_outside_allowlist_before_pty_spawn() {
    let mut mux = test_mux(24, 80);
    mux.available_agents = vec!["codex".to_string()];

    let err = mux
        .spawn_request(SpawnRequest::Agent("claude".to_string()), &[])
        .unwrap_err();

    assert!(err.to_string().contains("rejected agent \"claude\""));
    assert!(mux.sessions.is_empty());
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
fn dialog_opaque_backdrop_hides_multiplexer_chrome() {
    fn mux_with_two_sessions() -> Multiplexer {
        let mut mux = split_tab_mux();
        let (session_one, _) = test_session(24, 80);
        let (session_two, _) = test_shell_session(24, 80);
        mux.sessions.insert(1, session_one);
        mux.sessions.insert(2, session_two);
        mux
    }

    fn assert_backdrop_opaque(mut mux: Multiplexer, context: &str) {
        let frame =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::DialogChange))
                .to_string();

        assert!(
            frame.contains(jackin_tui::ansi::reset_rgb_bg(jackin_tui::DIALOG_BACKDROP)),
            "{context} should paint an opaque black backdrop: {frame:?}"
        );
        assert!(
            !frame.contains("jackin'"),
            "{context} should hide the top status brand pill behind the dialog: {frame:?}"
        );
        assert!(
            !frame.contains(&format!(
                "{}┌",
                jackin_tui::ansi::rgb_fg(jackin_tui::BORDER_GRAY)
            )),
            "{context} should hide inactive pane borders behind the dialog: {frame:?}"
        );
        // The dialog itself renders with a PHOSPHOR_GREEN border, so we cannot
        // assert the absence of PHOSPHOR_GREEN + "┌" in the frame — the dialog's
        // own box corner will always be there. The opaque backdrop assertion above
        // already guarantees pane chrome is painted over.
    }

    let mut menu_mux = mux_with_two_sessions();
    menu_mux.open_command_palette();
    assert_backdrop_opaque(menu_mux, "menu dialog");

    let mut container_mux = mux_with_two_sessions();
    container_mux.open_container_info_dialog();
    assert_backdrop_opaque(container_mux, "container info dialog");

    let mut github_mux = mux_with_two_sessions();
    github_mux.pull_request_context_branch = Some(branch("feat/capsule-pr-context-bar"));
    github_mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    github_mux.workdir_context.gh_available = false;
    github_mux.open_github_context_dialog(Instant::now());
    assert_backdrop_opaque(github_mux, "GitHub context dialog");
}

#[test]
fn palette_close_single_pane_opens_confirm_directly() {
    let mut mux = single_pane_tab_mux();
    mux.handle_palette_command(PaletteCommand::Close);

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ConfirmAction {
            kind: ConfirmKind::CloseTab,
            selected_yes: false
        })
    ));
}

#[test]
fn palette_close_split_tab_opens_target_picker() {
    let mut mux = split_tab_mux();
    mux.handle_palette_command(PaletteCommand::Close);

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CloseTargetPicker {
            selected: 0,
            filter
        }) if filter.is_empty()
    ));
}

#[test]
fn branch_context_visibility_keeps_content_area_reserved() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    // 24 rows − STATUS_BAR_ROWS(2) − BRANCH_CONTEXT_BAR_ROWS(1) − CAPSULE_HINT_BAR_ROWS(1) − CAPSULE_HINT_SEPARATOR_ROWS(1) = 19
    assert_eq!(mux.content_rows, 19);

    mux.pull_request_context_cache.insert(
        branch("asa/pr-context"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(434))),
        },
    );
    assert!(mux.apply_git_branch_context(Some("asa/pr-context"), now));
    assert_eq!(mux.content_rows, 19);
    assert_eq!(
        mux.pull_request_context.as_deref().map(|pr| pr.number),
        Some(434)
    );

    mux.pull_request_context_cache.insert(
        branch("feature/no-pr"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );
    assert!(mux.apply_git_branch_context(Some("feature/no-pr"), now));
    assert_eq!(mux.content_rows, 19);
    assert!(mux.pull_request_context.is_none());

    assert!(mux.apply_git_branch_context(Some("main"), now));
    assert_eq!(mux.content_rows, 19);
    assert!(mux.pull_request_context.is_none());
}

#[test]
fn git_branch_context_updates_status_before_github_lookup() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("old/pr"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(434)));
    mux.reconcile_content_rows();
    // 24 rows − STATUS_BAR_ROWS(2) − BRANCH_CONTEXT_BAR_ROWS(1) − CAPSULE_HINT_BAR_ROWS(1) − CAPSULE_HINT_SEPARATOR_ROWS(1) = 19
    assert_eq!(mux.content_rows, 19);

    mux.pull_request_context_cache.insert(
        branch("new/local-branch"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );
    assert!(mux.apply_git_branch_context(Some("new/local-branch"), now));

    assert_eq!(
        mux.pull_request_context_branch.as_deref(),
        Some("new/local-branch")
    );
    assert!(mux.pull_request_context.is_none());
    assert_eq!(mux.content_rows, 19);
}

#[test]
fn git_branch_context_recognizes_repo_after_startup() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.workdir_context.is_git_repo = false;
    mux.workdir_context.gh_available = false;

    assert!(mux.apply_git_branch_context(Some("feat/capsule-pr-context-bar"), now));

    assert!(mux.workdir_context.is_git_repo);
    assert_eq!(
        mux.context_bar_branch(),
        Some("feat/capsule-pr-context-bar")
    );
    assert!(mux.pull_request_context.is_none());
}

#[test]
fn apply_pull_request_context_loaded_drops_stale_request() {
    let mut mux = test_mux(24, 100);
    mux.pull_request_lookup.request_id = 5;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
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
        mux.pull_request_lookup.in_flight,
        "stale request must leave in_flight untouched"
    );
    assert!(
        mux.pull_request_context.is_none(),
        "stale request must not write PR"
    );
}

#[test]
fn apply_pull_request_context_loaded_transient_failure_preserves_prior_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_lookup.request_id = 7;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(123)));
    mux.pull_request_context_cache.insert(
        branch("feat/x"),
        PullRequestContextCacheEntry {
            checked_at: now - Duration::from_secs(5),
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
        !mux.pull_request_lookup.in_flight,
        "transient failure must clear in_flight so next tick retries"
    );
    assert_eq!(
        mux.pull_request_context_cache
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
        Some(Dialog::GitHubContext { copied: false })
    ));
    assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/x"));
    assert_eq!(
        mux.pull_request_context.as_ref().map(|pr| pr.number),
        Some(436)
    );
    assert!(!mux.pull_request_context_loading());
}

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
        Some(Dialog::GitHubContext { copied: false })
    ));
    assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/x"));
    assert!(mux.pull_request_context.is_none());
    assert!(!mux.pull_request_context_loading());
    assert!(
        !mux.pull_request_context_cache.contains_key("feat/x"),
        "transient failure must not cache a no-PR result"
    );
}

#[test]
fn apply_git_branch_context_loaded_drops_stale_request() {
    let mut mux = test_mux(24, 100);
    mux.git_branch_lookup.request_id = 4;
    mux.git_branch_lookup.in_flight = true;
    let changed = mux.apply_git_branch_context_loaded(
        2,
        GitContext::Branch {
            name: branch("feat/x"),
            head: None,
        },
        Instant::now(),
    );
    assert!(!changed);
    assert!(mux.git_branch_lookup.in_flight, "stale id leaves in_flight");
    assert!(mux.pull_request_context_branch.is_none());
}

#[test]
fn apply_git_branch_context_bumps_pr_request_id_on_branch_change() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("feat/a"));
    mux.workdir_context.gh_available = false;
    let id_before = mux.pull_request_lookup.request_id;
    let _ = mux.apply_git_branch_context(Some("feat/b"), now);
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "branch change must bump request_id so stale gh worker responses are rejected"
    );
}

#[test]
fn apply_git_context_bumps_pr_request_id_on_same_branch_head_change() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("feat/a"));
    mux.pull_request_context_head = Some(oid('1'));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(455)));
    mux.pull_request_context_cache.insert(
        branch("feat/a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: Some(oid('1')),
            pull_request: Some(Arc::new(pull_request_fixture(455))),
        },
    );
    mux.workdir_context.gh_available = false;
    let id_before = mux.pull_request_lookup.request_id;

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
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "HEAD change must bump request_id so stale gh worker responses are rejected"
    );
    assert!(
        mux.pull_request_context.is_none(),
        "old PR cache must not stay visible for the new HEAD"
    );
}

#[test]
fn purge_expired_pull_request_cache_entries_drops_old_entries() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    let ttl = PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL * 2;
    mux.pull_request_context_cache.insert(
        branch("feat/fresh"),
        PullRequestContextCacheEntry {
            checked_at: now - Duration::from_secs(10),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(1))),
        },
    );
    mux.pull_request_context_cache.insert(
        branch("feat/old"),
        PullRequestContextCacheEntry {
            checked_at: now - ttl - Duration::from_secs(1),
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(2))),
        },
    );
    mux.purge_expired_pull_request_cache_entries(now);
    assert!(mux.pull_request_context_cache.contains_key("feat/fresh"));
    assert!(!mux.pull_request_context_cache.contains_key("feat/old"));
}

#[test]
fn pull_request_cache_fresh_at_strict_boundary() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    // Just-fresh: at the boundary minus 1 ms.
    mux.pull_request_context_cache.insert(
        branch("branch-a"),
        PullRequestContextCacheEntry {
            checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL + Duration::from_millis(1),
            head: None,
            pull_request: None,
        },
    );
    // Just-stale: at the boundary plus 1 ms.
    mux.pull_request_context_cache.insert(
        branch("branch-b"),
        PullRequestContextCacheEntry {
            checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL - Duration::from_millis(1),
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
    mux.pull_request_context_head = Some(oid('2'));
    mux.pull_request_context_cache.insert(
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
    mux.pull_request_context_cache.insert(
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
    mux.pull_request_context_branch = Some(branch("feature/current"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_cache.insert(
        branch("feature/current"),
        PullRequestContextCacheEntry {
            checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL,
            head: None,
            pull_request: Some(Arc::new(pull_request_fixture(436))),
        },
    );

    assert!(!mux.apply_git_branch_context(Some("feature/current"), now));
    assert_eq!(
        mux.pull_request_context.as_deref().map(|pr| pr.number),
        Some(436)
    );
}

#[test]
fn cached_pull_request_stays_visible_during_forced_dialog_refresh() {
    let mut mux = test_mux(24, 100);
    mux.pull_request_context_branch = Some(branch("feature/current"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    mux.pull_request_lookup.in_flight = true;
    // Exercise the real dialog-open path so a future refactor that
    // skips force_spawn (or routes through a different dispatcher)
    // is caught here instead of by silent UX regression.
    mux.workdir_context.gh_available = false;
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
    mux.workdir_context.gh_available = true;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_string());
    mux.pull_request_context_branch = Some(branch("feat/x"));
    let id_before = mux.pull_request_lookup.request_id;

    mux.open_github_context_dialog(Instant::now());

    assert!(
        mux.pull_request_lookup.in_flight,
        "dialog-open must fire a real worker spawn when gh_available is true"
    );
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "force-spawn must bump request_id"
    );
}

#[test]
fn open_github_context_dialog_force_spawns_when_startup_missed_gh() {
    let mut mux = test_mux(24, 100);
    mux.workdir_context.gh_available = false;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_string());
    mux.pull_request_context_branch = Some(branch("feat/x"));
    let id_before = mux.pull_request_lookup.request_id;

    mux.open_github_context_dialog(Instant::now());

    assert!(
        mux.pull_request_lookup.in_flight,
        "manual refresh must schedule a background lookup even when startup marked gh unavailable"
    );
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "manual refresh should not need a synchronous gh availability probe"
    );
    assert!(
        !mux.workdir_context.gh_available,
        "gh availability flips only after the background lookup succeeds"
    );
}

#[test]
fn background_pull_request_success_marks_gh_available_after_startup_miss() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.workdir_context.gh_available = false;
    mux.workdir_context.is_git_repo = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_lookup.request_id = 7;
    mux.pull_request_lookup.in_flight = true;

    let changed = mux.apply_pull_request_context_loaded(
        7,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(436)))),
        now,
    );

    assert!(changed);
    assert!(
        mux.workdir_context.gh_available,
        "successful background gh lookup should unblock later conservative refreshes"
    );
}

#[test]
fn open_github_context_dialog_bypasses_fresh_no_pr_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.workdir_context.gh_available = true;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_string());
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context_cache.insert(
        branch("feat/x"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: None,
            pull_request: None,
        },
    );

    mux.open_github_context_dialog(now);

    assert!(
        mux.pull_request_lookup.in_flight,
        "manual dialog open must refresh even when a recent background lookup saw no PR"
    );
    assert!(
        mux.pull_request_context_loading(),
        "dialog should show resolving while the forced refresh is in flight"
    );
}

#[test]
fn apply_git_context_head_change_schedules_fresh_pr_lookup() {
    // gh_available=true so the spawn path runs end-to-end; we assert
    // in_flight=true after the head flip to prove the maybe_spawn at
    // the tail of `apply_git_context` fires (not just request_id bump).
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.workdir_context.gh_available = true;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_string());
    mux.pull_request_context_branch = Some(branch("feat/a"));
    mux.pull_request_context_head = Some(oid('1'));

    mux.apply_git_context(
        GitContext::Branch {
            name: branch("feat/a"),
            head: Some(oid('2')),
        },
        now,
    );

    assert!(
        mux.pull_request_lookup.in_flight,
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
    mux.pull_request_lookup.request_id = 9;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context_head = Some(oid('a'));

    let changed = mux.apply_pull_request_context_loaded(
        9,
        Some(branch("feat/x")),
        Some(oid('b')),
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(777)))),
        now,
    );

    assert!(
        mux.pull_request_context.is_none(),
        "head-drift result must not be assigned to visible context"
    );
    assert!(
        !mux.pull_request_context_cache.contains_key("feat/x"),
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
    mux.pull_request_lookup.request_id = 11;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context_head = Some(oid('c'));

    let _ = mux.apply_pull_request_context_loaded(
        11,
        Some(branch("feat/x")),
        None,
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(778)))),
        now,
    );

    assert!(
        mux.pull_request_context.is_none(),
        "None→Some head drift refused"
    );
    assert!(!mux.pull_request_context_cache.contains_key("feat/x"));
}

#[test]
fn apply_pull_request_context_loaded_refuses_head_drift_some_to_none() {
    // Inverse: spawn captured a head, apply-time mux.head was
    // cleared (e.g. HEAD became unreadable between spawn and apply).
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_lookup.request_id = 13;
    mux.pull_request_lookup.in_flight = true;
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_context_head = None;

    let _ = mux.apply_pull_request_context_loaded(
        13,
        Some(branch("feat/x")),
        Some(oid('d')),
        PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(779)))),
        now,
    );

    assert!(
        mux.pull_request_context.is_none(),
        "Some→None head drift refused"
    );
    assert!(!mux.pull_request_context_cache.contains_key("feat/x"));
}

#[test]
fn apply_git_context_simultaneous_branch_and_head_change_invalidates_cache() {
    let mut mux = test_mux(24, 100);
    let now = Instant::now();
    mux.pull_request_context_branch = Some(branch("feat/a"));
    mux.pull_request_context_head = Some(oid('1'));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(455)));
    mux.pull_request_context_cache.insert(
        branch("feat/a"),
        PullRequestContextCacheEntry {
            checked_at: now,
            head: Some(oid('1')),
            pull_request: Some(Arc::new(pull_request_fixture(455))),
        },
    );
    mux.workdir_context.gh_available = false;
    let id_before = mux.pull_request_lookup.request_id;

    let changed = mux.apply_git_context(
        GitContext::Branch {
            name: branch("feat/b"),
            head: Some(oid('2')),
        },
        now,
    );

    assert!(changed, "branch+head flip must dirty the visible context");
    assert_eq!(
        mux.pull_request_lookup.request_id,
        id_before.wrapping_add(1),
        "simultaneous branch+head flip must bump request_id once"
    );
    assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/b"));
    assert_eq!(
        mux.pull_request_context_head.as_deref(),
        Some("2222222222222222222222222222222222222222")
    );
    assert!(
        mux.pull_request_context.is_none(),
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
    mux.workdir_context.is_git_repo = false;
    let now = Instant::now();

    mux.apply_git_context(GitContext::Detached { head: oid('1') }, now);

    assert!(
        mux.workdir_context.is_git_repo,
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

#[test]
fn read_packed_git_ref_oid_does_not_cache_truncated_read() {
    // packed-refs cap forces a synthetic-truncation scenario: write
    // exactly PACKED_REFS_MAX_BYTES of content so read_text_bounded's
    // length equals the cap, then mutate underlying bytes and confirm
    // the second read sees the new value (would not, if the truncated
    // first read had cached).
    let temp = tempfile::tempdir().unwrap();
    let packed_refs = temp.path().join("packed-refs-truncated");
    // Pad with comment lines + a real ref entry until total length
    // matches the cap exactly.
    let real_line = "1111111111111111111111111111111111111111 refs/heads/feat/x\n";
    let padding_per_line = "# padding to fill packed-refs to the cap byte limit aaaaaaaaaa\n";
    // Target one byte OVER the cap so metadata.len() > cap triggers
    // the real truncation path (not the exactly-cap edge case).
    let target_size = PACKED_REFS_MAX_BYTES as usize + 1;
    let mut buf = String::with_capacity(target_size);
    while buf.len() + real_line.len() + padding_per_line.len() <= target_size {
        buf.push_str(padding_per_line);
    }
    buf.push_str(real_line);
    let remaining = target_size.saturating_sub(buf.len());
    buf.extend(std::iter::repeat_n('#', remaining));
    buf.truncate(target_size);
    std::fs::write(&packed_refs, &buf).unwrap();

    let _ = read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/x");

    // Mutate same-length bytes (overwrite oid in place); mtime advances.
    let buf2 = buf.replacen(
        "1111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222",
        1,
    );
    std::fs::write(&packed_refs, &buf2).unwrap();

    assert_eq!(
        read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/x").as_deref(),
        Some("2222222222222222222222222222222222222222"),
        "truncated first read must not have cached; second read sees fresh content"
    );
}

#[test]
fn packed_refs_cache_eviction_bounds_entries_at_cap() {
    // Create CAP+1 distinct packed-refs paths and read each once.
    // After the (CAP+1)th insert, exactly CAP of the inserted
    // paths must remain — proves both the upper bound AND that
    // eviction removed only one entry (catches over-evict bugs
    // where the cache would degrade to a single entry).
    let temp = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for i in 0..=PACKED_REFS_CACHE_MAX_ENTRIES {
        let path = temp.path().join(format!("packed-refs-evict-{i}"));
        std::fs::write(
            &path,
            format!("1111111111111111111111111111111111111111 refs/heads/branch-{i}\n"),
        )
        .unwrap();
        let _ = read_packed_git_ref_oid(&path, &format!("refs/heads/branch-{i}"));
        paths.push(path);
    }

    let count = with_packed_refs_cache(|cache| {
        paths
            .iter()
            .filter(|p| cache.contains_key(p.as_path()))
            .count()
    });
    // The just-inserted (CAP+1)th entry MUST be present; eviction
    // targets pre-existing entries, never the new insert.
    assert!(
        with_packed_refs_cache(|cache| cache.contains_key(paths.last().unwrap().as_path())),
        "newly-inserted entry must survive eviction"
    );
    // Exactly one of the previously-inserted CAP entries must have
    // been evicted: count of our tracked paths in the cache should
    // equal CAP, not less (over-evict) or more (no-op evict).
    assert_eq!(
        count, PACKED_REFS_CACHE_MAX_ENTRIES,
        "eviction must drop exactly one entry; saw {count} surviving of CAP={}",
        PACKED_REFS_CACHE_MAX_ENTRIES
    );
}

#[test]
fn read_git_ref_oid_loose_wins_over_packed() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().to_path_buf();
    std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
    std::fs::write(
        git_dir.join("refs/heads/feat-x"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
    )
    .unwrap();
    std::fs::write(
        git_dir.join("packed-refs"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feat-x\n",
    )
    .unwrap();

    assert_eq!(
        read_git_ref_oid(&git_dir, None, "refs/heads/feat-x").as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "loose ref must win over packed-refs entry"
    );
}

#[test]
fn force_spawn_pull_request_context_lookup_skipped_when_in_flight() {
    let mut mux = test_mux(24, 100);
    mux.workdir_context.gh_available = true;
    mux.workdir_context.is_git_repo = true;
    mux.workdir_context.default_branch = Some("main".to_string());
    mux.pull_request_context_branch = Some(branch("feat/x"));
    mux.pull_request_lookup.in_flight = true;
    let id_before = mux.pull_request_lookup.request_id;

    let spawned = mux.force_spawn_pull_request_context_lookup(Instant::now());

    assert!(
        !spawned,
        "force-spawn must no-op when a worker is in flight"
    );
    assert_eq!(
        mux.pull_request_lookup.request_id, id_before,
        "force-spawn skip must not bump request_id"
    );
}

#[test]
fn palette_exit_opens_exit_confirm() {
    let mut mux = single_pane_tab_mux();
    mux.handle_palette_command(PaletteCommand::Exit);

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ConfirmAction {
            kind: ConfirmKind::Exit,
            selected_yes: false
        })
    ));
}

#[test]
fn kitty_escape_in_agent_picker_returns_to_menu() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    let frame = mux
        .handle_input(InputEvent::Data(b"\r".to_vec()))
        .expect("New tab command should redraw");
    assert!(String::from_utf8_lossy(&frame).contains("New tab"));
    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));

    let events = mux.input_parser.parse(b"\x1b[27;1u");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b".to_vec())]);
    for event in events {
        mux.handle_input(event);
    }

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CommandPalette { .. })
    ));
}

#[test]
fn mouse_sgr_encoding_preserves_press_and_release() {
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, true, jackin_term::MouseProtocolEncoding::Sgr).unwrap(),
        b"\x1b[<0;12;3M"
    );
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, false, jackin_term::MouseProtocolEncoding::Sgr)
            .unwrap(),
        b"\x1b[<0;12;3m"
    );
}

#[test]
fn mouse_default_encoding_uses_xterm_fields() {
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, true, jackin_term::MouseProtocolEncoding::Default)
            .unwrap(),
        b"\x1b[M ,#"
    );
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, false, jackin_term::MouseProtocolEncoding::Default)
            .unwrap(),
        b"\x1b[M#,#"
    );
}

#[test]
fn mouse_mode_filter_respects_tracking_granularity() {
    use jackin_term::MouseProtocolMode;

    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::None,
        0,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::Press,
        0,
        true
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::Press,
        0,
        false
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::PressRelease,
        32,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::ButtonMotion,
        32,
        true
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::ButtonMotion,
        SGR_NO_BUTTON_MOTION,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::AnyMotion,
        SGR_NO_BUTTON_MOTION,
        true
    ));
}

#[test]
fn wheel_forwards_to_mouse_enabled_tui() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[?1049h\x1b[?1003h\x1b[?1006h");
    mux.sessions.insert(1, session);

    let redraw = mux.handle_input(InputEvent::MousePress {
        row: STATUS_BAR_ROWS + 1,
        col: 1,
        button: 64,
    });

    assert!(
        redraw.is_none(),
        "pane-owned wheel should not redraw jackin'"
    );
    assert_eq!(
        input_rx.try_recv().expect("wheel should reach PTY"),
        b"\x1b[<64;1;1M"
    );
    assert!(
        input_rx.try_recv().is_err(),
        "wheel should not produce extra PTY input"
    );
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
}

#[test]
fn wheel_scrolls_jackin_scrollback_when_mouse_is_disabled() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_pane_session(20, 78, agent);
        for i in 0..40 {
            session.feed_pty(format!("line {i}\r\n").as_bytes());
        }
        assert_eq!(session.scrollback_offset, 0);
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        assert!(
            redraw.is_some(),
            "{pane_kind} pane scrollback should redraw jackin'"
        );
        assert!(
            input_rx.try_recv().is_err(),
            "mouse-disabled {pane_kind} panes must not receive raw wheel bytes"
        );
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
    }
}

#[test]
fn wheel_noops_for_focused_normal_screen_pane_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(55, 200);
        let (mut session, mut input_rx) = test_pane_session(51, 198, agent);
        session.feed_pty(b"\x1b[49;3Hcodex prompt");
        assert_eq!(session.scrollback_filled(), 0);
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 10,
            col: 10,
            button: 64,
        });

        assert!(
            redraw.is_none(),
            "{pane_kind} normal-screen pane without scrollback should not redraw jackin'"
        );
        assert!(
            input_rx.try_recv().is_err(),
            "normal-screen {pane_kind} pane without scrollback must not receive cursor-key wheel fallback"
        );
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
    }
}

#[test]
fn wheel_scrolls_top_anchored_inline_history_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        feed_top_anchored_inline_history(&mut session, 5, 12);
        session.feed_pty(b"\x1b[8;1Hlive prompt");
        assert!(
            session.scrollback_filled() >= 3,
            "{pane_kind} pane should retain top-anchored inline history"
        );
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        let frame = redraw.expect("inline history wheel should redraw");
        assert!(
            input_rx.try_recv().is_err(),
            "{pane_kind} pane must not receive cursor-key wheel fallback"
        );
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
        assert_focused_scroll_chrome(
            &frame,
            &format!("normal-screen {pane_kind} pane with inline history"),
        );
        assert!(
            String::from_utf8_lossy(&frame).contains("history 4"),
            "normal-screen {pane_kind} wheel should render retained inline history"
        );
    }
}

#[test]
fn scrolled_inline_history_preserves_color_and_selection_highlight() {
    let mut mux = single_pane_tab_mux_with_size(12, 40);
    let (mut session, mut input_rx) = test_pane_session(8, 38, Some("codex"));
    session.feed_pty(b"\x1b[1;5r\x1b[5;1H");
    for i in 0..12 {
        session.feed_pty(format!("\r\n\x1b[2K\x1b[31mred history {i}\x1b[0m").as_bytes());
    }
    session.feed_pty(b"\x1b[r\x1b[8;1Hlive prompt");
    mux.sessions.insert(1, session);

    let frame = mux
        .handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        })
        .expect("inline history wheel should redraw");

    assert!(
        input_rx.try_recv().is_err(),
        "Codex-style inline history scroll must not forward wheel bytes"
    );
    let rendered = String::from_utf8_lossy(&frame);
    assert!(
        rendered.contains("\x1b[0;31mred history"),
        "scrolled Codex inline history should preserve red SGR styling: {rendered:?}"
    );

    let inner = mux.visible_panes()[0].inner;
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 10,
    });
    let selected_frame = mux.compose_full_frame(FullRedrawReason::SelectionRepaint);
    let selected = String::from_utf8_lossy(&selected_frame);
    assert!(
        selected.contains("\x1b[0;7;31mred history"),
        "selection overlay should repaint scrolled inline history with inverse red styling: {selected:?}"
    );
}

#[test]
fn wheel_scrolls_normal_screen_history_preserved_before_clear_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        for i in 0..5 {
            session.feed_pty(format!("release note {i}\r\n").as_bytes());
        }
        assert_eq!(
            session.scrollback_filled(),
            0,
            "{pane_kind} setup output fits without native scrollback before clear"
        );

        session.feed_pty(b"\x1b[1;1H\x1b[Jlive prompt");
        assert!(
            session.scrollback_filled() >= 5,
            "{pane_kind} pane should preserve normal-screen rows erased by clear/redraw"
        );
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        let frame = redraw.expect("clear-preserved history wheel should redraw");
        assert!(
            input_rx.try_recv().is_err(),
            "{pane_kind} pane must not receive cursor-key wheel fallback"
        );
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
        assert_focused_scroll_chrome(
            &frame,
            &format!("normal-screen {pane_kind} pane with clear-preserved history"),
        );
        assert!(
            String::from_utf8_lossy(&frame).contains("release note"),
            "normal-screen {pane_kind} wheel should render rows preserved before clear"
        );
    }
}

#[test]
fn wheel_scrolls_csi_scroll_up_inline_history_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        session.feed_pty(b"\x1b[1;5r\x1b[1;1Htop row\x1b[2;1Hsecond row\x1b[3;1Hthird row");
        session.feed_pty(b"\x1b[2S\x1b[r\x1b[8;1Hlive prompt");
        assert!(
            session.scrollback_filled() >= 2,
            "{pane_kind} pane should retain rows removed by top-anchored CSI S"
        );
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        let frame = redraw.expect("CSI S inline history wheel should redraw");
        assert!(
            input_rx.try_recv().is_err(),
            "{pane_kind} pane must not receive cursor-key wheel fallback"
        );
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 2);
        assert_focused_scroll_chrome(
            &frame,
            &format!("normal-screen {pane_kind} pane with CSI S inline history"),
        );
        assert!(
            String::from_utf8_lossy(&frame).contains("top row"),
            "normal-screen {pane_kind} wheel should render CSI S retained history"
        );
    }
}

#[test]
fn wheel_sends_cursor_fallback_to_mouse_disabled_alt_screen_tui() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?1049h");
    mux.sessions.insert(1, session);

    let redraw = mux.handle_input(InputEvent::MousePress {
        row: STATUS_BAR_ROWS + 1,
        col: 1,
        button: 64,
    });

    assert!(
        redraw.is_none(),
        "pane-owned fallback should not redraw jackin'"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
}

#[test]
fn wheel_sends_cursor_fallback_to_alt_screen_tui_with_retained_primary_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert!(
        session.scrollback_filled() > 0,
        "setup should leave retained primary-screen scrollback"
    );
    session.feed_pty(b"\x1b[?1049h");
    assert!(
        session.alternate_screen(),
        "setup should leave pane in the alternate screen"
    );
    mux.sessions.insert(1, session);

    let redraw = mux.handle_input(InputEvent::MousePress {
        row: STATUS_BAR_ROWS + 1,
        col: 1,
        button: 64,
    });

    assert!(
        redraw.is_none(),
        "alternate-screen fallback should not redraw jackin'"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
}

#[test]
fn wheel_cursor_fallback_respects_application_cursor_mode() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[?1049h\x1b[?1h");
    mux.sessions.insert(1, session);

    let redraw = mux.handle_input(InputEvent::MousePress {
        row: STATUS_BAR_ROWS + 1,
        col: 1,
        button: 65,
    });

    assert!(
        redraw.is_none(),
        "pane-owned fallback should not redraw jackin'"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1bOB\x1bOB\x1bOB");
}

#[test]
fn alt_screen_overflow_does_not_draw_scrollbar_without_retained_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_session(8, 20);
    session.feed_pty(b"\x1b[?1049h");
    for i in 0..20 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert_eq!(session.scrollback_filled(), 0);
    mux.sessions.insert(1, session);

    let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
    assert_no_scroll_thumb(&frame, "alt-screen pane without retained scrollback");
}

#[test]
fn normal_screen_panes_do_not_draw_scrollbar_when_grid_is_full_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux();
        let (mut session, _input_rx) = test_pane_session(8, 20, agent);
        for row in 0..8 {
            session.feed_pty(format!("\x1b[{};1Hrow {row}", row + 1).as_bytes());
        }
        mux.sessions.insert(1, session);

        let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
        assert_no_scroll_thumb(
            &frame,
            &format!("normal-screen {pane_kind} pane with full grid but no scrollback"),
        );
    }
}

#[test]
fn normal_screen_panes_do_not_draw_scrollbar_when_content_spans_viewport_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux();
        let (mut session, _input_rx) = test_pane_session(8, 20, agent);
        session.feed_pty(b"\x1b[1;1Htop transcript\x1b[8;1Hbottom status");
        assert_eq!(session.scrollback_filled(), 0);
        mux.sessions.insert(1, session);

        let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
        assert_no_scroll_thumb(
            &frame,
            &format!(
                "normal-screen {pane_kind} pane with viewport-spanning content but no scrollback"
            ),
        );
    }
}

#[test]
fn normal_screen_panes_do_not_keep_scrollbar_when_cursor_moves_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(55, 200);
        let (mut session, _input_rx) = test_pane_session(51, 198, agent);
        session.feed_pty(b"\x1b[1;1Hrelease notes\x1b[51;1Hstatus line\x1b[48;3Hx");
        assert_eq!(session.scrollback_filled(), 0);
        mux.sessions.insert(1, session);

        let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
        assert_no_scroll_thumb(
            &frame,
            &format!("normal-screen {pane_kind} transcript pane after cursor moved up"),
        );
    }
}

#[test]
fn alt_screen_exit_resets_keyboard_modes_for_shell_prompt() {
    let (mut session, _input_rx) = test_session(8, 20);
    session.feed_pty(b"\x1b[?1049h\x1b[>1u\x1b[>4;2m");
    let _ = session.drain_passthrough();

    session.feed_pty(b"\x1b[?1049l");
    let drained = session.drain_passthrough();

    assert!(
        drained.iter().any(|bytes| bytes == b"\x1b[<u"),
        "kitty keyboard reset missing from {drained:?}"
    );
    assert!(
        drained.iter().any(|bytes| bytes == b"\x1b[>4;0m"),
        "modifyOtherKeys reset missing from {drained:?}"
    );
}

#[test]
fn pointer_shape_updates_only_when_shape_changes() {
    let mut mux = test_mux(24, 80);
    mux.pointer_shapes_supported = true;
    mux.status_bar.identity_label = "jk-test-container".to_string();
    mux.status_bar.instance_id_label = "test".to_string();
    mux.pull_request_context_branch = Some(branch("feature/context"));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.attached_out = Some(tx);
    let hit = branch_context_bar_layout(
        mux.term_rows,
        mux.term_cols,
        mux.pull_request_context_branch.as_deref(),
        mux.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        mux.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.left_region)
    .expect("branch context should fit");

    mux.update_pointer_shape_for_mouse(23, hit.start - 1, SGR_NO_BUTTON_MOTION);
    let first = rx.try_recv().expect("first pointer-shape update");
    assert!(first.ends_with(b"\x1b]22;pointer\x1b\\"));

    mux.update_pointer_shape_for_mouse(23, hit.start, SGR_NO_BUTTON_MOTION);
    assert!(rx.try_recv().is_err(), "unchanged shape should not re-emit");
}

#[test]
fn pointer_shape_updates_for_clickable_top_chrome() {
    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    let _ = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.attached_out = Some(tx);
    let tab_col = mux
        .status_bar
        .tab_regions
        .first()
        .map(|(start, _)| start.saturating_sub(1))
        .expect("tab region should render");

    mux.update_pointer_shape_for_mouse(0, tab_col, SGR_NO_BUTTON_MOTION);
    let tab_shape = rx.try_recv().expect("tab pointer-shape update");
    assert!(tab_shape.ends_with(b"\x1b]22;pointer\x1b\\"));

    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    let _ = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.attached_out = Some(tx);
    let menu_col = mux
        .status_bar
        .hint_region
        .map(|(start, _)| start.saturating_sub(1))
        .expect("menu region should render");

    mux.update_pointer_shape_for_mouse(0, menu_col, SGR_NO_BUTTON_MOTION);
    let menu_shape = rx.try_recv().expect("menu pointer-shape update");
    assert!(menu_shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn pointer_shape_updates_for_clickable_dialog_copy_target() {
    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    mux.status_bar.identity_label = "jk-test-container".to_string();
    mux.open_container_info_dialog();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.attached_out = Some(tx);
    let dialog = mux.dialog_top().expect("container info dialog should open");
    let (row, col, _, _) = dialog.box_rect(mux.term_rows, mux.term_cols);

    mux.update_pointer_shape_for_mouse(
        row.saturating_add(1),
        col.saturating_add(1),
        SGR_NO_BUTTON_MOTION,
    );
    let shape = rx.try_recv().expect("dialog pointer-shape update");
    assert!(shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn bottom_container_click_opens_container_info_without_copying() {
    let mut mux = test_mux(24, 80);
    mux.pointer_shapes_supported = false;
    mux.status_bar.identity_label = "jk-test-container".to_string();
    mux.status_bar.instance_id_label = "test".to_string();
    mux.status_bar.role = "the-architect".to_string();
    mux.pull_request_context_branch = Some(branch("feature/context"));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.attached_out = Some(tx);
    let hit = branch_context_bar_layout(
        mux.term_rows,
        mux.term_cols,
        mux.pull_request_context_branch.as_deref(),
        mux.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        mux.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.container_region)
    .expect("container should fit");

    let frame = mux
        .handle_input(InputEvent::MousePress {
            row: mux.term_rows - 1,
            col: hit.start - 1,
            button: 0,
        })
        .expect("container click should redraw");

    while let Ok(output) = rx.try_recv() {
        assert!(
            !output
                .windows(b"\x1b]52;c;".len())
                .any(|w| w == b"\x1b]52;c;"),
            "opening container info must not send OSC 52"
        );
    }
    assert!(!String::from_utf8_lossy(&frame).contains("Copied!"));
    let Some(Dialog::ContainerInfo {
        copied: false,
        workdir,
        ..
    }) = mux.dialog_top()
    else {
        panic!("identity click should open container info")
    };
    assert_eq!(workdir, "/workspace");
}

#[test]
fn bottom_context_click_opens_github_context_dialog() {
    let mut mux = test_mux(24, 100);
    mux.status_bar.identity_label = "jk-test-container".to_string();
    mux.status_bar.instance_id_label = "test".to_string();
    mux.pull_request_context_branch = Some(branch("feature/context"));
    mux.pull_request_context = Some(Arc::new(pull_request_fixture(434)));
    mux.workdir_context.gh_available = false;
    let hit = branch_context_bar_layout(
        mux.term_rows,
        mux.term_cols,
        mux.pull_request_context_branch.as_deref(),
        mux.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        mux.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.left_region)
    .expect("GitHub context should fit");

    let frame = mux
        .handle_input(InputEvent::MousePress {
            row: mux.term_rows - 1,
            col: hit.start - 1,
            button: 0,
        })
        .expect("context click should redraw");

    let rendered = String::from_utf8_lossy(&frame);
    assert!(rendered.contains("GitHub context"));
    assert!(
        rendered.contains("copy GitHub URL"),
        "dialog hint must render above the bottom branch/context bar: {rendered:?}"
    );
    assert!(
        rendered.rfind("copy GitHub URL") > rendered.rfind("test"),
        "dialog footer should be painted after the bottom branch/context bar so it clears its own rows: {rendered:?}"
    );
    let hint_row = mux.term_rows - 2;
    let bottom_row = mux.term_rows;
    assert!(
        rendered.contains(&format!("\x1b[{hint_row};")),
        "dialog hint should render one row above the spacer: {rendered:?}"
    );
    assert!(
        rendered.contains(&format!("\x1b[{bottom_row};")),
        "bottom branch/context bar should stay on the final row: {rendered:?}"
    );
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::GitHubContext { copied: false })
    ));
    assert_eq!(
        mux.pull_request_context_branch.as_deref(),
        Some("feature/context")
    );
    assert_eq!(
        mux.pull_request_context.as_ref().map(|pr| pr.number),
        Some(434)
    );
}

#[test]
fn container_info_copy_feedback_expires() {
    let mut mux = test_mux(24, 80);
    mux.dialog_push(Dialog::ContainerInfo {
        container_name: "jk-test-container".to_string(),
        role: "the-architect".to_string(),
        focused_agent: Some("claude".to_string()),
        workdir: "/workspace".to_string(),
        diagnostics: crate::tui::components::dialog::ContainerInfoDiagnostics::default(),
        copied: true,
    });
    let now = Instant::now();
    mux.dialog_copy_feedback_deadline = Some(now);

    assert!(mux.expire_dialog_copy_feedback(now));
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo { copied: false, .. })
    ));
}

#[test]
fn container_info_id_click_copies_and_renders_feedback() {
    let mut mux = test_mux(40, 120);
    mux.pointer_shapes_supported = false;
    mux.dialog_push(Dialog::ContainerInfo {
        container_name: "jk-test-container".to_string(),
        role: "the-architect".to_string(),
        focused_agent: Some("claude".to_string()),
        workdir: "/workspace".to_string(),
        diagnostics: crate::tui::components::dialog::ContainerInfoDiagnostics::default(),
        copied: false,
    });
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.attached_out = Some(tx);
    let (box_row, box_col, _, _) = mux
        .dialog_top()
        .expect("container info dialog should be open")
        .box_rect(mux.term_rows, mux.term_cols);

    let frame = mux
        .handle_input(InputEvent::MousePress {
            row: box_row + 1,
            col: box_col + 1,
            button: 0,
        })
        .expect("container id click should redraw copy feedback");

    let mut saw_osc52 = false;
    while let Ok(output) = rx.try_recv() {
        saw_osc52 |= output
            .windows(b"\x1b]52;c;".len())
            .any(|w| w == b"\x1b]52;c;");
    }
    assert!(saw_osc52, "copy should emit OSC 52");
    assert!(String::from_utf8_lossy(&frame).contains("Copied!"));
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo { copied: true, .. })
    ));
}

#[test]
fn prefix_ctrl_l_has_named_pane_clear_reason() {
    assert_eq!(
        prefix_full_redraw_reason(&PrefixCommand::ClearPane),
        FullRedrawReason::PaneClear
    );
}

#[test]
fn command_stdout_trimmed_returns_trimmed_stdout() {
    let mut command = Command::new("printf");
    command.arg("  branch-name\n");

    assert_eq!(
        command_stdout_trimmed(&mut command),
        Some("branch-name".to_string())
    );
}

#[test]
fn command_stdout_trimmed_rejects_known_failure_status() {
    // `sleep 0.05` keeps the child alive long enough for the
    // try_wait poll loop to observe `Ok(None)` first and then the
    // failing `Ok(Some(1))` exit on the next tick. Without the
    // sleep the child can vanish between spawn and the first
    // try_wait, which collapses the Err(ECHILD) "status lost"
    // arm and the Ok(Some(false)) "failed" arm into one path.
    let mut command = Command::new("sh");
    command.args(["-c", "printf branch-name; sleep 0.05; exit 1"]);

    assert_eq!(command_stdout_trimmed(&mut command), None);
}

#[test]
fn gh_lookup_output_rejects_statusless_stderr_only_failure() {
    let err = command_output_or_lookup_error("gh", None, b"", b"HTTP 401: Bad credentials\n")
        .expect_err("stderr-only statusless gh output is a transient failure");

    assert!(
        err.to_string().contains("HTTP 401"),
        "stderr detail should survive for logs: {err}"
    );
}

// Action-boundary dispatch tests: drive apply_action directly without
// going through handle_input so the dispatch layer is testable without
// a live PTY or input parser in the loop.

#[test]
fn apply_action_dismiss_closes_top_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open(), "palette should be open");

    mux.apply_action(Action::Dialog(DialogAction::Dismiss));

    assert!(!mux.dialog_open(), "dismiss should close the dialog");
    assert_eq!(mux.mux_mode(), MuxMode::Normal);
}

#[test]
fn apply_action_open_palette_pushes_palette_dialog() {
    let mut mux = single_pane_tab_mux();
    assert!(!mux.dialog_open());

    mux.apply_action(Action::OpenPalette);

    assert!(
        matches!(mux.dialog_top(), Some(Dialog::CommandPalette { .. })),
        "OpenPalette should push CommandPalette dialog"
    );
    assert_eq!(mux.mux_mode(), MuxMode::Dialog);
}

#[test]
fn apply_action_open_palette_closes_existing_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());

    mux.apply_action(Action::OpenPalette);

    assert!(
        !mux.dialog_open(),
        "palette toggle should close open dialog"
    );
    assert_eq!(mux.mux_mode(), MuxMode::Normal);
}

#[test]
fn apply_action_open_container_info_pushes_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.status_bar.identity_label = "jk-test-container".to_string();
    mux.status_bar.role = "test-role".to_string();

    mux.apply_action(Action::OpenContainerInfo);

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            container_name,
            copied: false,
            ..
        }) if container_name == "jk-test-container"
    ));
}

#[test]
fn apply_action_open_rename_tab_pushes_dialog() {
    let mut mux = single_pane_tab_mux();

    mux.apply_action(Action::OpenRenameTab(0));

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::RenameTab { tab_idx: 0, .. })
    ));
    assert!(mux.last_tab_click.is_none());
}

#[test]
fn apply_action_switch_tab_moves_active_tab() {
    let mut mux = single_pane_tab_mux();
    mux.tabs.push(Tab::new_single("Shell", 2));
    let _ = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);

    mux.apply_action(Action::SwitchTab(1));

    assert_eq!(mux.active_tab, 1);
}

#[test]
fn apply_action_status_bar_click_switches_tab() {
    let mut mux = single_pane_tab_mux();
    mux.tabs.push(Tab::new_single("Shell", 2));
    let _ = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
    let col = (1..mux.term_cols)
        .find(|col| mux.status_bar.tab_at_col(*col) == Some(1))
        .expect("second tab should have a clickable column")
        - 1;

    mux.apply_action(Action::StatusBarClick { col });

    assert_eq!(mux.active_tab, 1);
}

#[test]
fn apply_action_status_bar_double_click_opens_rename() {
    let mut mux = single_pane_tab_mux();
    let _ = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
    let col = (1..mux.term_cols)
        .find(|col| mux.status_bar.tab_at_col(*col) == Some(0))
        .expect("first tab should have a clickable column")
        - 1;

    mux.apply_action(Action::StatusBarClick { col });
    mux.apply_action(Action::StatusBarClick { col });

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::RenameTab { tab_idx: 0, .. })
    ));
}

#[test]
fn apply_action_branch_context_bar_click_opens_container_info() {
    let mut mux = test_mux(24, 80);
    mux.status_bar.identity_label = "jk-test-container".to_string();
    mux.status_bar.instance_id_label = "test".to_string();
    mux.status_bar.role = "the-architect".to_string();
    mux.pull_request_context_branch = Some(branch("feature/context"));
    let hit = branch_context_bar_layout(
        mux.term_rows,
        mux.term_cols,
        mux.pull_request_context_branch.as_deref(),
        mux.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        mux.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.container_region)
    .expect("container should fit");

    mux.apply_action(Action::BranchContextBarClick {
        row: mux.term_rows - 1,
        col: hit.start - 1,
    });

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            container_name,
            copied: false,
            ..
        }) if container_name == "jk-test-container"
    ));
}

#[test]
fn apply_action_palette_new_tab_pushes_agent_picker() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();

    mux.apply_action(Action::Palette(PaletteCommand::NewTab));

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
}

#[test]
fn apply_action_open_agent_picker_pushes_picker() {
    let mut mux = single_pane_tab_mux();

    mux.apply_action(Action::OpenAgentPicker(PickerIntent::NewTab));

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
}

#[test]
fn apply_action_detach_sets_detach_request() {
    let mut mux = single_pane_tab_mux();

    mux.apply_action(Action::Detach);

    assert!(mux.detach_requested);
}

#[test]
fn prefix_new_tab_routes_through_action_picker() {
    let mut mux = single_pane_tab_mux();

    mux.handle_prefix_command(PrefixCommand::NewTab);

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
}

#[test]
fn apply_action_focus_pane_at_changes_focus() {
    let mut mux = split_tab_mux();
    let target = mux
        .visible_panes()
        .into_iter()
        .find(|pane| pane.id == 2)
        .expect("second pane should be visible")
        .inner;

    let frame = mux
        .apply_action(Action::FocusPaneAt {
            row: target.row,
            col: target.col,
        })
        .expect("focus change should redraw");

    assert_eq!(mux.tabs[mux.active_tab].focused_id, 2);
    assert!(!frame.is_empty(), "focus redraw frame should be emitted");
}

#[test]
fn apply_action_forward_mouse_sends_to_focused_pane() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?1003h\x1b[?1006h");
    mux.sessions.insert(1, session);

    let frame = mux.apply_action(Action::ForwardMouse {
        row: STATUS_BAR_ROWS + 1,
        col: 1,
        button: 0,
        press: true,
    });

    assert!(frame.is_none(), "PTY mouse forward should not redraw");
    assert_eq!(
        input_rx.try_recv().expect("mouse press should reach PTY"),
        b"\x1b[<0;1;1M"
    );
}

#[test]
fn apply_action_dialog_consume_keeps_dialog_open() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());

    // Consume should leave the dialog open (key was absorbed, no state change).
    mux.apply_action(Action::Dialog(DialogAction::Consume));

    assert!(mux.dialog_open(), "Consume must not close the dialog");
}

#[test]
fn apply_action_dialog_click_routes_to_dialog_handler() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());

    mux.apply_action(Action::DialogClick { row: 0, col: 0 });

    assert!(!mux.dialog_open(), "outside click should dismiss dialog");
}

#[test]
fn apply_action_focus_report_does_not_open_dialog() {
    let mut mux = single_pane_tab_mux();
    assert!(!mux.dialog_open());

    mux.apply_action(Action::FocusReport(true));
    assert!(!mux.dialog_open());

    mux.apply_action(Action::FocusReport(false));
    assert!(!mux.dialog_open());
}

#[test]
fn apply_action_mouse_chrome_update_sets_pointer_shape() {
    let mut mux = single_pane_tab_mux();
    mux.pointer_shapes_supported = true;
    let _ = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.attached_out = Some(tx);
    let tab_col = mux
        .status_bar
        .tab_regions
        .first()
        .map(|(start, _)| start.saturating_sub(1))
        .expect("tab region should render");

    mux.apply_action(Action::MouseChromeUpdate {
        row: 0,
        col: tab_col,
        button: SGR_NO_BUTTON_MOTION,
    });

    let mut outputs = Vec::new();
    while let Ok(output) = rx.try_recv() {
        outputs.push(output);
    }
    assert!(
        outputs
            .iter()
            .any(|output| output.ends_with(b"\x1b]22;pointer\x1b\\")),
        "mouse chrome action should emit pointer shape update"
    );
}

#[test]
fn apply_action_wheel_scrolls_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.sessions.insert(1, session);

    let frame = mux
        .apply_action(Action::Wheel {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        })
        .expect("wheel over retained scrollback should redraw");

    assert!(
        input_rx.try_recv().is_err(),
        "mouse-disabled pane must not receive raw wheel bytes"
    );
    assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
    assert!(
        !frame.is_empty(),
        "scrollback redraw frame should be emitted"
    );
}

#[test]
fn apply_action_end_drag_resize_clears_drag_state() {
    let mut mux = single_pane_tab_mux();
    mux.drag = Some(DragState {
        tab_idx: 0,
        path: Vec::new(),
        orient: SplitOrient::Horizontal,
        rect: Rect::new(STATUS_BAR_ROWS, 0, mux.content_rows, mux.term_cols),
    });

    let frame = mux
        .apply_action(Action::EndDragResize)
        .expect("ending drag should redraw layout");

    assert!(mux.drag.is_none(), "drag state should be cleared");
    assert!(!frame.is_empty(), "layout redraw frame should be emitted");
}

#[test]
fn apply_action_mouse_release_ends_drag_resize() {
    let mut mux = single_pane_tab_mux();
    mux.drag = Some(DragState {
        tab_idx: 0,
        path: Vec::new(),
        orient: SplitOrient::Horizontal,
        rect: Rect::new(STATUS_BAR_ROWS, 0, mux.content_rows, mux.term_cols),
    });

    let frame = mux
        .apply_action(Action::MouseRelease {
            row: STATUS_BAR_ROWS,
            col: 1,
            button: 0,
        })
        .expect("left-button release should redraw layout after drag");

    assert!(mux.drag.is_none(), "drag state should be cleared");
    assert!(!frame.is_empty(), "layout redraw frame should be emitted");
}

#[test]
fn apply_action_start_drag_resize_sets_drag_state() {
    let mut mux = split_tab_mux();
    let (row, col) = (0..mux.term_rows)
        .flat_map(|row| (0..mux.term_cols).map(move |col| (row, col)))
        .find(|(row, col)| mux.detect_drag_start(*row, *col).is_some())
        .expect("split tab should expose a draggable border");

    let frame = mux.apply_action(Action::StartDragResize { row, col });

    assert!(frame.is_none(), "drag start should not redraw yet");
    assert!(mux.drag.is_some(), "drag state should be active");
}

#[test]
fn apply_action_pane_primary_press_starts_drag_on_border() {
    let mut mux = split_tab_mux();
    let (row, col) = (0..mux.term_rows)
        .flat_map(|row| (0..mux.term_cols).map(move |col| (row, col)))
        .find(|(row, col)| mux.detect_drag_start(*row, *col).is_some())
        .expect("split tab should expose a draggable border");

    let frame = mux.apply_action(Action::PanePrimaryPress { row, col });

    assert!(frame.is_none(), "drag start should not redraw yet");
    assert!(mux.drag.is_some(), "drag state should be active");
}

#[test]
fn apply_action_pane_primary_press_starts_selection_for_shell() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);

    let frame = mux
        .apply_action(Action::PanePrimaryPress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
        })
        .expect("selection start should repaint");

    assert!(
        input_rx.try_recv().is_err(),
        "mouse-disabled pane should start selection instead of receiving raw mouse"
    );
    assert!(mux.selection.is_some(), "selection should be active");
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
}

#[test]
fn apply_action_start_selection_sets_selection_state() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.sessions.insert(1, session);

    let frame = mux
        .apply_action(Action::StartSelection {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
        })
        .expect("selection start should repaint");

    let selection = mux.selection.expect("selection should be active");
    assert_eq!((selection.anchor_row, selection.anchor_col), (0, 0));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
}

#[test]
fn apply_action_selection_motion_updates_selection() {
    let mut mux = single_pane_tab_mux();
    let inner = Rect::new(STATUS_BAR_ROWS + 1, 1, 10, 20);
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 0,
    });

    let frame = mux
        .apply_action(Action::SelectionMotion {
            row: inner.row + 2,
            col: inner.col + 3,
        })
        .expect("selection motion should redraw");

    let selection = mux.selection.expect("selection should remain active");
    assert_eq!((selection.end_row, selection.end_col), (2, 3));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
}

#[test]
fn apply_action_pane_button_motion_updates_selection() {
    let mut mux = single_pane_tab_mux();
    let inner = Rect::new(STATUS_BAR_ROWS + 1, 1, 10, 20);
    mux.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 0,
    });

    let frame = mux
        .apply_action(Action::PaneButtonMotion {
            row: inner.row + 2,
            col: inner.col + 3,
        })
        .expect("button motion should repaint active selection");

    let selection = mux.selection.expect("selection should remain active");
    assert_eq!((selection.end_row, selection.end_col), (2, 3));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
}

#[test]
fn split_close_frame_contains_screen_erase() {
    // Regression for Defect 29: pane/tab close must produce a full repaint
    // with \x1b[2J so stale cells from the removed pane are cleared.
    // compose_full_frame(SplitClose) is the path triggered after close_focused_pane/tab.
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    let _ = mux.compose_full_frame(FullRedrawReason::FirstAttach);

    let frame = mux.compose_full_frame(FullRedrawReason::SplitClose);
    assert!(!frame.is_empty(), "SplitClose must produce a repaint frame");
    // \x1b[2J is the screen-erase sequence required by the layout-change path.
    assert!(
        frame.windows(4).any(|w| w == b"\x1b[2J"),
        "SplitClose frame must include \\x1b[2J screen erase to flush stale cells"
    );
}
