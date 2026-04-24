//! Mouse event handling for the workspace manager: list/details seam drag,
//! click-to-select in the list pane, and `FileBrowser` URL-click fallthrough.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::super::super::widgets::file_browser::FileBrowserState;
use super::super::state::{
    DragState, ManagerListRow, ManagerStage, ManagerState, Modal, clamp_split,
};

/// Minimum terminal width (in columns) at which the list/details seam is
/// draggable. Below this, the 20/80 clamp bounds leave the right pane
/// implausibly narrow for meaningful interaction — silently ignore mouse
/// events rather than produce an unusable layout.
const MIN_DRAGGABLE_WIDTH: u16 = 40;
/// Half-width of the seam hit-region. A Down event lands within ±1 column
/// of the computed seam to initiate drag. Narrow enough that operators
/// don't accidentally start a drag while clicking in either pane.
const SEAM_HIT_SLACK: u16 = 1;

/// Height of the header chunk in the list-view chrome. Mirrors
/// `Constraint::Length(3)` in `render::render`. Used by mouse hit-testing
/// to convert a terminal row into a list item index.
const LIST_HEADER_HEIGHT: u16 = 3;
/// Height of the footer chunk in the list-view chrome. Mirrors
/// `Constraint::Length(2)` in `render::render`.
const LIST_FOOTER_HEIGHT: u16 = 2;

/// Dispatch a mouse event into the workspace manager's list view. Drives
/// the mouse-draggable seam between the list pane and the details pane.
///
/// Behaviour:
/// - On `ManagerStage::List` with no list-level modal open: drives the
///   list/details seam drag (anchor + drag + release) and click-to-select.
/// - On `ManagerStage::Editor` / `CreatePrelude` with a `FileBrowser` modal
///   whose git-prompt overlay is active AND has a resolved URL: a
///   `Down(Left)` on the URL row fires `open::that_detached` best-effort.
/// - Ignores everything when the terminal is narrower than
///   [`MIN_DRAGGABLE_WIDTH`] — drag bounds would be absurd.
/// - All other events are ignored.
///
/// The caller (run-loop in `src/launch/mod.rs`) is responsible for
/// passing the current `terminal.size()?` as `term_size` so the handler
/// can compute the seam column as `term_size.width * list_split_pct / 100`.
pub fn handle_mouse(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    if term_size.width < MIN_DRAGGABLE_WIDTH {
        return;
    }

    // Editor / CreatePrelude file-browser URL click: only on Down(Left),
    // only when the modal is a FileBrowser with a resolved git URL.
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_open_file_browser_git_url(state, mouse, term_size)
    {
        return;
    }

    // Stage + modal gate for the list-view seam drag. Only the List view
    // participates in drag; the Editor, CreatePrelude and ConfirmDelete
    // stages only observe the URL-click path above.
    if !matches!(state.stage, ManagerStage::List) {
        return;
    }
    if state.list_modal.is_some() {
        return;
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let seam_x = seam_column(state.list_split_pct, term_size.width);
            // Seam hit always wins — a click on the seam column starts a
            // drag, never a row select. Even if the seam happens to overlap
            // a valid row position, the resize affordance takes precedence.
            if near_seam(mouse.column, seam_x) {
                state.drag_state = Some(DragState {
                    anchor_pct: state.list_split_pct,
                    anchor_x: mouse.column,
                });
                return;
            }
            // Otherwise, treat as click-to-select if the click lands inside
            // the list pane's content area (excluding borders).
            if let Some(row) = list_content_row_index(state, mouse, term_size, seam_x) {
                state.selected = row.to_screen_index(state.workspaces.len());
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(anchor) = state.drag_state {
                let new_pct = pct_from_drag(anchor, mouse.column, term_size.width);
                state.list_split_pct = clamp_split(new_pct);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            state.drag_state = None;
        }
        _ => {}
    }
}

/// If the `Editor` or `CreatePrelude` stage has an open `FileBrowser`
/// whose git-prompt is active with a resolved URL, and the click lands
/// on the URL row, fire `open::that_detached` best-effort. Returns
/// `true` iff the click was consumed (URL opened). Non-matching stages,
/// non-click events, and clicks outside the URL row all return `false`
/// and the caller falls through to the list-view handler.
///
/// Modal geometry comes from `render::modal_outer_rect` — the same
/// helper `render_modal` uses — so mouse hit-testing can never drift
/// out of sync with what was drawn.
fn try_open_file_browser_git_url(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let (modal, fb_state): (&Modal<'_>, &FileBrowserState) = match &state.stage {
        ManagerStage::Editor(editor) => match editor.modal.as_ref() {
            Some(m @ Modal::FileBrowser { state, .. }) => (m, state),
            _ => return false,
        },
        ManagerStage::CreatePrelude(prelude) => match prelude.modal.as_ref() {
            Some(m @ Modal::FileBrowser { state, .. }) => (m, state),
            _ => return false,
        },
        _ => return false,
    };
    let modal_area = super::super::render::modal_outer_rect(modal, term_size);
    fb_state.maybe_open_url_on_click(modal_area, mouse.column, mouse.row)
}

/// Return the logical list row the mouse is over, or `None` if the click
/// falls outside the list pane's content area.
///
/// Mirrors the layout from `render::render` + `render::render_list_body`:
///   - Chrome: `[header (3 rows)][body][footer (2 rows)]`
///   - Body is horizontally split; left column hosts the workspace list.
///   - The list itself sits inside a bordered block — row 0 of list
///     items is at y = header + 1 (the +1 skips the top border).
///
/// Returns `Some(row)` only when:
///   - `mouse.column` is inside `[1, seam_x - 1]` (left pane interior,
///     i.e. excluding both the left border and the seam column itself)
///   - `mouse.row` is inside `[header + 1, body_end - 1]` (body interior,
///     excluding the top and bottom border rows)
///   - The computed index maps to a valid `ManagerListRow`. See
///     `ManagerListRow` docs for row layout.
fn list_content_row_index(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    seam_x: u16,
) -> Option<ManagerListRow> {
    // Column check — strictly inside the left pane (exclude left border
    // and seam column, which is also the left pane's right border).
    if mouse.column == 0 || mouse.column >= seam_x {
        return None;
    }
    // Row check — strictly inside the bordered list block.
    let content_top = LIST_HEADER_HEIGHT + 1; // +1 skips the top border
    let body_end = term_size.height.saturating_sub(LIST_FOOTER_HEIGHT);
    // Content bottom is body_end - 1 (skip bottom border). Guard against
    // a terminal so short that the list has no interior.
    let content_bottom = body_end.saturating_sub(1);
    if mouse.row < content_top || mouse.row >= content_bottom {
        return None;
    }
    // Row index into the list: items start at y = content_top (the first
    // row below the top border).
    let idx = usize::from(mouse.row - content_top);
    state.row_at(idx)
}

/// Compute the seam column (0-based) for a given split percentage and
/// total terminal width. Mirrors ratatui's own `Layout::split` arithmetic
/// closely enough for hit-testing purposes.
const fn seam_column(pct: u16, width: u16) -> u16 {
    // (width * pct) / 100 — saturating so a pathological width of 0 doesn't
    // panic. Under MIN_DRAGGABLE_WIDTH this arithmetic is already gated off
    // by the caller, but keep the helper safe for direct unit-testing.
    width.saturating_mul(pct) / 100
}

/// `true` when `column` is within ±`SEAM_HIT_SLACK` of `seam_x`.
const fn near_seam(column: u16, seam_x: u16) -> bool {
    let lo = seam_x.saturating_sub(SEAM_HIT_SLACK);
    let hi = seam_x.saturating_add(SEAM_HIT_SLACK);
    column >= lo && column <= hi
}

/// Derive the new split percentage from an active drag anchor and the
/// current mouse column. Handles the signed delta safely (mouse can move
/// either way along x) without underflow on u16.
fn pct_from_drag(anchor: DragState, mouse_col: u16, width: u16) -> u16 {
    // Signed delta in columns, scaled to a percentage of terminal width.
    let delta_cols = i32::from(mouse_col) - i32::from(anchor.anchor_x);
    let delta_pct = delta_cols * 100 / i32::from(width.max(1));
    let candidate = i32::from(anchor.anchor_pct) + delta_pct;
    // Clamp into [0, 100] before the narrower [MIN..=MAX] clamp so we can
    // safely cast back to u16.
    let bounded = candidate.clamp(0, 100);
    // `as u16` is safe: bounded is in [0,100].
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let narrowed = bounded as u16;
    narrowed
}

#[cfg(test)]
mod mouse_drag_tests {
    //! Unit tests for `handle_mouse`: the list/details seam is a
    //! mouse-draggable resize affordance driven entirely from `ManagerState`.
    //! These build `MouseEvent` values directly and bypass the ratatui
    //! event loop — enough to pin the seam hit-test + drag math without a
    //! real terminal.
    use super::handle_mouse;
    use crate::launch::manager::state::{
        DEFAULT_SPLIT_PCT, EditorState, MAX_SPLIT_PCT, MIN_SPLIT_PCT, ManagerStage, ManagerState,
        Modal,
    };
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    /// Build a `ManagerState` in the List stage at the default split,
    /// with no workspaces and no modal.
    fn list_state() -> ManagerState<'static> {
        let config = crate::config::AppConfig::default();
        let tmp = tempfile::tempdir().unwrap();
        ManagerState::from_config(&config, tmp.path())
    }

    /// Build a `MouseEvent` at column `col`, row 0.
    const fn mouse(kind: MouseEventKind, col: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// A 100-col-wide terminal area.
    const fn term(width: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height: 30,
        }
    }

    #[test]
    fn mouse_down_on_seam_starts_drag() {
        // Default split on a 100-col terminal => seam at column
        // `DEFAULT_SPLIT_PCT`.
        let mut state = list_state();
        assert_eq!(state.list_split_pct, DEFAULT_SPLIT_PCT);
        let e = mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT);
        handle_mouse(&mut state, e, term(100));
        assert!(
            state.drag_state.is_some(),
            "Down on seam must capture drag anchor; got {:?}",
            state.drag_state,
        );
        let drag = state.drag_state.unwrap();
        assert_eq!(drag.anchor_pct, DEFAULT_SPLIT_PCT);
        assert_eq!(drag.anchor_x, DEFAULT_SPLIT_PCT);
    }

    #[test]
    fn mouse_drag_updates_split_pct() {
        // Anchor at DEFAULT_SPLIT_PCT. Drag +10 columns on a 100-col
        // terminal ⇒ +10%.
        let mut state = list_state();
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        let target = DEFAULT_SPLIT_PCT + 10;
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Drag(MouseButton::Left), target),
            term(100),
        );
        assert_eq!(state.list_split_pct, target);
    }

    #[test]
    fn mouse_drag_clamps_to_min_and_max() {
        // Drag far left ⇒ clamp to MIN_SPLIT_PCT.
        let mut state = list_state();
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Drag(MouseButton::Left), 0),
            term(100),
        );
        assert_eq!(state.list_split_pct, MIN_SPLIT_PCT);

        // Drag far right ⇒ clamp to MAX_SPLIT_PCT.
        let mut state = list_state();
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Drag(MouseButton::Left), 99),
            term(100),
        );
        assert_eq!(state.list_split_pct, MAX_SPLIT_PCT);
    }

    #[test]
    fn mouse_up_ends_drag() {
        let mut state = list_state();
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        assert!(state.drag_state.is_some());
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Up(MouseButton::Left), 60),
            term(100),
        );
        assert!(state.drag_state.is_none(), "Up must clear drag anchor");
    }

    #[test]
    fn mouse_down_far_from_seam_does_not_start_drag() {
        // Clicks in the middle of either pane must be ignored — the
        // operator's intent is "click a row/button", not "start a resize".
        let mut state = list_state();
        // Seam at column `DEFAULT_SPLIT_PCT`; columns near either border
        // are far enough from the seam to be rejected.
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), 2),
            term(100),
        );
        assert!(state.drag_state.is_none(), "left-pane click must not drag");
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), 80),
            term(100),
        );
        assert!(state.drag_state.is_none(), "right-pane click must not drag",);
    }

    #[test]
    fn drag_ignored_when_list_modal_open() {
        // GithubPicker is the only list-level modal today. Any mouse event
        // while it's up must be a silent no-op — the picker owns the
        // keyboard + (implicitly) the mouse focus.
        let mut state = list_state();
        // Use the github_mounts resolver indirectly — easier to
        // just synthesize a GithubPicker state with an arbitrary choice.
        // The picker's exact contents don't matter; only `list_modal.is_some()`.
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![MountConfig {
                src: "/w".into(),
                dst: "/w".into(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        // Ensure the helper signature compiles (guards against future refactors).
        let _ = crate::launch::manager::github_mounts::resolve_for_workspace(&ws);
        state.list_modal = Some(Modal::GithubPicker {
            state: crate::launch::widgets::github_picker::GithubPickerState::new(vec![
                crate::launch::widgets::github_picker::GithubChoice {
                    src: "/w".into(),
                    branch: "main".into(),
                    url: "https://github.com/o/r".into(),
                },
            ]),
        });

        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        assert!(
            state.drag_state.is_none(),
            "Down with list_modal open must not drag",
        );
    }

    #[test]
    fn drag_ignored_on_non_list_stage() {
        // While in the Editor (or any non-List stage), mouse events are
        // ignored outright — no seam to drag.
        let mut state = list_state();
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        state.stage = ManagerStage::Editor(EditorState::new_edit("x".into(), ws));

        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
            term(100),
        );
        assert!(
            state.drag_state.is_none(),
            "Down on Editor stage must not drag",
        );
    }

    #[test]
    fn drag_ignored_when_terminal_too_narrow() {
        // Terminals narrower than MIN_DRAGGABLE_WIDTH skip hit-testing
        // entirely — below that the clamp bounds already leave the right
        // pane implausibly small.
        let mut state = list_state();
        // 30-col terminal is below the 40-col threshold.
        handle_mouse(
            &mut state,
            mouse(MouseEventKind::Down(MouseButton::Left), 13),
            term(30),
        );
        assert!(state.drag_state.is_none());
    }

    // ── File-browser URL-click integration ─────────────────────────────
    //
    // When a FileBrowser modal with a git-prompt + resolved URL is open
    // during the Editor or CreatePrelude stages, Down(Left) on the URL
    // row must be consumed by the open-URL path (best-effort; silent on
    // failure) — observable side-effect: the drag-anchor never latches.

    /// Term of 120x40 ⇒ FileBrowser modal at (18, 9, 84, 22); URL row at
    /// y = 17, column range ≈ 19..=100. Mirrors the reference geometry
    /// used in `file_browser::tests::manufactured_modal_area`.
    fn term_120x40() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        }
    }

    /// Mouse event at `(col, row)`, left-button Down.
    const fn mouse_down_at(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn mouse_down_on_url_row_in_prelude_with_url_does_not_drag() {
        use crate::launch::manager::state::CreatePreludeState;
        use crate::launch::widgets::file_browser::FileBrowserState;
        let mut state = list_state();
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        // Build a FileBrowser at `parent`, select the repo, open git prompt,
        // and inject a URL so the URL row renders.
        let mut fb = FileBrowserState::new_at(tmp.path().to_path_buf(), parent);
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.pending_git_url = Some("file:///tmp/unreachable".to_string());

        let prelude = CreatePreludeState {
            modal: Some(Modal::FileBrowser {
                target: crate::launch::manager::state::FileBrowserTarget::CreateFirstMountSrc,
                state: fb,
            }),
            ..CreatePreludeState::default()
        };
        state.stage = ManagerStage::CreatePrelude(prelude);

        // URL row at y = 17 for this term size; centre column ≈ 60.
        handle_mouse(&mut state, mouse_down_at(60, 17), term_120x40());
        // No drag latched — URL click is consumed before the seam path.
        assert!(
            state.drag_state.is_none(),
            "URL click must not start a seam drag",
        );
    }

    #[test]
    fn mouse_down_outside_url_row_in_prelude_is_silent_noop() {
        use crate::launch::manager::state::CreatePreludeState;
        use crate::launch::widgets::file_browser::FileBrowserState;
        let mut state = list_state();
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut fb = FileBrowserState::new_at(tmp.path().to_path_buf(), parent);
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.pending_git_url = Some("file:///tmp/unreachable".to_string());

        let prelude = CreatePreludeState {
            modal: Some(Modal::FileBrowser {
                target: crate::launch::manager::state::FileBrowserTarget::CreateFirstMountSrc,
                state: fb,
            }),
            ..CreatePreludeState::default()
        };
        state.stage = ManagerStage::CreatePrelude(prelude);

        // Row 0 is well outside the URL row (17) and the modal entirely.
        handle_mouse(&mut state, mouse_down_at(60, 0), term_120x40());
        // CreatePrelude is not the List stage, so the list-drag path is
        // also inert — no drag latched regardless of the URL branch.
        assert!(state.drag_state.is_none());
    }

    // ── Click-to-select tests ──────────────────────────────────────
    //
    // Layout (100x30 terminal, header=3 footer=2 body=25):
    //   y = 0..=2   → header (chunks[0])
    //   y = 3       → body top border (list block)
    //   y = 4       → list item 0 ("Current directory")
    //   y = 5       → list item 1 (first saved workspace)
    //   ...
    //   y = 28      → body bottom border
    //   y = 29      → footer (chunks[2])
    //
    // Left pane (default split = DEFAULT_SPLIT_PCT%): x = 0..=(seam-1)
    // with x=0 = left border and x=seam-1 inclusive = last interior col.
    // The seam column itself is the drag-handle.

    /// Mouse event at `(col, row)`, left-button Down.
    const fn mouse_at(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Build a list state with `n` saved workspaces (row 0 + n + sentinel).
    fn list_state_with_saved(n: usize) -> ManagerState<'static> {
        let mut config = crate::config::AppConfig::default();
        for i in 0..n {
            config.workspaces.insert(
                format!("ws-{i:02}"),
                WorkspaceConfig {
                    workdir: format!("/w/{i}"),
                    mounts: vec![],
                    allowed_agents: vec![],
                    default_agent: None,
                    last_agent: None,
                    env: std::collections::BTreeMap::new(),
                    agents: std::collections::BTreeMap::new(),
                },
            );
        }
        let tmp = tempfile::tempdir().unwrap();
        ManagerState::from_config(&config, tmp.path())
    }

    #[test]
    fn click_on_first_row_sets_selected_to_zero() {
        // y=4 = first list item (index 0, "Current directory").
        let mut state = list_state_with_saved(3);
        state.selected = 2;
        handle_mouse(&mut state, mouse_at(10, 4), term(100));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn click_on_fifth_row_sets_selected_to_four() {
        // y=8 = fifth list row (index 4). Needs enough saved workspaces
        // to make index 4 a valid selection target.
        let mut state = list_state_with_saved(5);
        state.selected = 0;
        handle_mouse(&mut state, mouse_at(10, 8), term(100));
        assert_eq!(state.selected, 4);
    }

    #[test]
    fn click_on_sentinel_row_sets_selected_to_sentinel_idx() {
        // 3 saved workspaces ⇒ rows are:
        //   y=4  → index 0 ("Current directory")
        //   y=5,6,7 → indices 1, 2, 3 (saved)
        //   y=8  → index 4 (sentinel "+ New workspace")
        let mut state = list_state_with_saved(3);
        state.selected = 0;
        handle_mouse(&mut state, mouse_at(10, 8), term(100));
        assert_eq!(state.selected, 4, "sentinel_idx = saved_count + 1 = 4");
    }

    #[test]
    fn click_outside_list_rows_does_not_change_selected() {
        // Several "outside" positions must all leave selected untouched:
        //   - Click above the list (y < 4, e.g. in the header)
        //   - Click on the left border (x=0)
        //   - Click at x >= seam (right pane territory)
        //   - Click below the list content (footer)
        let mut state = list_state_with_saved(3);
        state.selected = 2;
        let initial = state.selected;

        // In the header.
        handle_mouse(&mut state, mouse_at(10, 1), term(100));
        assert_eq!(state.selected, initial, "click in header must not select");

        // On the top border of the list block.
        handle_mouse(&mut state, mouse_at(10, 3), term(100));
        assert_eq!(state.selected, initial, "click on top border");

        // On the left border column.
        handle_mouse(&mut state, mouse_at(0, 4), term(100));
        assert_eq!(state.selected, initial, "click on left border");

        // Past the sentinel row (y=9+ when we have 3 saved workspaces).
        handle_mouse(&mut state, mouse_at(10, 10), term(100));
        assert_eq!(state.selected, initial, "click below sentinel");

        // In the right pane (x=60, well clear of the default seam).
        handle_mouse(&mut state, mouse_at(60, 5), term(100));
        assert_eq!(state.selected, initial, "click in details pane");

        // In the footer.
        handle_mouse(&mut state, mouse_at(10, 29), term(100));
        assert_eq!(state.selected, initial, "click on footer row");
    }

    #[test]
    fn click_on_seam_still_starts_drag_not_selection() {
        // Regression guard for batch 14: a click on the seam column must
        // kick off a drag and NOT retarget selection, even when the y
        // coordinate happens to overlap a valid list row.
        let mut state = list_state_with_saved(3);
        state.selected = 0;
        // Default split on a 100-col terminal ⇒ seam at column
        // `DEFAULT_SPLIT_PCT`. y=5 maps to list index 1 in our layout —
        // if seam didn't win, selection would flip to 1.
        handle_mouse(&mut state, mouse_at(DEFAULT_SPLIT_PCT, 5), term(100));
        assert!(state.drag_state.is_some(), "click on seam must start drag");
        assert_eq!(
            state.selected, 0,
            "seam-click must not change selection even when y lands on a list row"
        );
    }
}
