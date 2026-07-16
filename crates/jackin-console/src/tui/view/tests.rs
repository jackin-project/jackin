// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

// Tests for `view`.

use super::*;
use crate::tui::model::{ConsoleManagerStageRoute, ConsoleStageModalFacts};

#[test]
fn console_main_frame_plan_routes_workspace_and_fullscreen_stages() {
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::Editor),
        ConsoleMainFramePlan::Editor
    );
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::Settings),
        ConsoleMainFramePlan::Settings
    );
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::List),
        ConsoleMainFramePlan::Workspace {
            render_list_body: true
        }
    );
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::CreatePrelude),
        ConsoleMainFramePlan::Workspace {
            render_list_body: false
        }
    );
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::ConfirmInstancePurge),
        ConsoleMainFramePlan::Workspace {
            render_list_body: false
        }
    );
}

#[test]
fn console_prepare_frame_plan_routes_only_mutating_pre_render_stages() {
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::Editor),
        ConsolePrepareFramePlan::Editor
    );
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::Settings),
        ConsolePrepareFramePlan::Settings
    );
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::List),
        ConsolePrepareFramePlan::List
    );
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::CreatePrelude),
        ConsolePrepareFramePlan::None
    );
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::ConfirmDelete),
        ConsolePrepareFramePlan::None
    );
}

#[test]
fn console_modal_render_plan_routes_modal_families() {
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::List),
        ConsoleModalRenderPlan::List
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::Editor),
        ConsoleModalRenderPlan::Editor
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::Settings),
        ConsoleModalRenderPlan::Settings
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::CreatePrelude),
        ConsoleModalRenderPlan::CreatePrelude
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::ConfirmDelete),
        ConsoleModalRenderPlan::ConfirmDelete
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::ConfirmInstancePurge),
        ConsoleModalRenderPlan::ConfirmInstancePurge
    );
}

#[test]
fn console_reserved_footer_height_plan_routes_screen_footers() {
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::Editor),
        ConsoleReservedFooterHeightPlan::Editor
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::Settings),
        ConsoleReservedFooterHeightPlan::Settings
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::List),
        ConsoleReservedFooterHeightPlan::Workspace
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::CreatePrelude),
        ConsoleReservedFooterHeightPlan::Workspace
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::ConfirmDelete),
        ConsoleReservedFooterHeightPlan::Workspace
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::ConfirmInstancePurge),
        ConsoleReservedFooterHeightPlan::Workspace
    );
}

#[test]
fn workspace_frame_areas_match_header_body_footer_contract() {
    let areas = workspace_frame_areas(Rect::new(0, 0, 80, 24));

    assert_eq!(areas.header, Rect::new(0, 0, 80, 2));
    assert_eq!(areas.body, Rect::new(0, 2, 80, 20));
    assert_eq!(areas.footer, Rect::new(0, 22, 80, 2));
}

#[test]
fn modal_content_area_reserves_footer_height() {
    let area = Rect::new(3, 4, 80, 24);

    assert_eq!(modal_content_area(area, 3), Rect::new(3, 4, 80, 21));
}

#[test]
fn modal_backdrop_area_reserves_footer_height() {
    let area = Rect::new(3, 4, 80, 24);

    assert_eq!(modal_backdrop_area(area, 3), Rect::new(3, 4, 80, 21));
}

#[test]
fn modal_content_area_saturates_when_footer_exceeds_height() {
    let area = Rect::new(3, 4, 80, 2);

    assert_eq!(modal_content_area(area, 3), Rect::new(3, 4, 80, 0));
}

#[test]
fn modal_content_areas_reserve_screen_specific_footers() {
    let area = Rect::new(3, 4, 80, 24);

    assert_eq!(
        modal_content_areas(area, 2, 4, 6),
        ModalContentAreas {
            workspace: Rect::new(3, 4, 80, 22),
            editor: Rect::new(3, 4, 80, 20),
            settings: Rect::new(3, 4, 80, 18),
        }
    );
}

#[test]
fn stage_modal_area_routes_by_visible_stage() {
    let areas = ModalContentAreas {
        workspace: Rect::new(0, 0, 10, 20),
        editor: Rect::new(1, 0, 10, 18),
        settings: Rect::new(2, 0, 10, 16),
    };

    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::Editor, areas),
        Some(StageModalArea::Editor(areas.editor))
    );
    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::Settings, areas),
        Some(StageModalArea::Settings(areas.settings))
    );
    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::CreatePrelude, areas),
        Some(StageModalArea::Workspace(areas.workspace))
    );
    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::List, areas),
        None
    );
    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::ConfirmDelete, areas),
        None
    );
}

#[test]
fn visible_modal_prepare_areas_routes_list_and_stage_modals() {
    let plan = visible_modal_prepare_areas(
        Rect::new(0, 0, 80, 24),
        2,
        4,
        6,
        ConsoleManagerStageRoute::Settings,
    );

    assert_eq!(plan.list_modal, Rect::new(0, 0, 80, 22));
    assert_eq!(
        plan.stage_modal,
        Some(StageModalArea::Settings(Rect::new(0, 0, 80, 18)))
    );

    let list_plan = visible_modal_prepare_areas(
        Rect::new(0, 0, 80, 24),
        2,
        4,
        6,
        ConsoleManagerStageRoute::List,
    );
    assert_eq!(list_plan.list_modal, Rect::new(0, 0, 80, 22));
    assert_eq!(list_plan.stage_modal, None);
}

#[test]
fn visible_modal_prepare_areas_for_stage_facts_uses_active_stage_footer() {
    let area = Rect::new(0, 0, 80, 24);

    let editor = visible_modal_prepare_areas_for_stage_facts(
        area,
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::Editor,
            workspace_footer_height: 2,
            editor_footer_height: 4,
            settings_footer_height: 6,
        },
    );
    assert_eq!(editor.list_modal, Rect::new(0, 0, 80, 22));
    assert_eq!(
        editor.stage_modal,
        Some(StageModalArea::Editor(Rect::new(0, 0, 80, 20)))
    );

    let settings = visible_modal_prepare_areas_for_stage_facts(
        area,
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::Settings,
            workspace_footer_height: 2,
            editor_footer_height: 4,
            settings_footer_height: 6,
        },
    );
    assert_eq!(
        settings.stage_modal,
        Some(StageModalArea::Settings(Rect::new(0, 0, 80, 18)))
    );

    let list = visible_modal_prepare_areas_for_stage_facts(
        area,
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::List,
            workspace_footer_height: 2,
            editor_footer_height: 4,
            settings_footer_height: 6,
        },
    );
    assert_eq!(list.list_modal, Rect::new(0, 0, 80, 22));
    assert_eq!(list.stage_modal, None);
}

#[test]
fn reserved_footer_height_prefers_screen_specific_heights() {
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: Some(4),
            settings_footer_height: Some(6),
            workspace_footer_height: 2,
        }),
        4
    );
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: None,
            settings_footer_height: Some(6),
            workspace_footer_height: 2,
        }),
        6
    );
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: None,
            settings_footer_height: None,
            workspace_footer_height: 2,
        }),
        2
    );
}

#[test]
fn footer_height_helpers_keep_one_row_minimum() {
    assert_eq!(effective_footer_height(0), 1);
    assert_eq!(effective_footer_height(3), 3);
    assert_eq!(measured_footer_height(&[], 80), footer_height(&[], 80));
    assert!(measured_footer_height(&[], 80) >= 1);
}

#[test]
fn workspace_header_title_is_view_owned() {
    assert_eq!(workspace_header_title(), "workspaces");
}

#[test]
fn modal_areas_stable_preferred_size() {
    // On a wide terminal (300 cols) each dialog holds its preferred width
    // (pct_w% of the 160-col reference), not a fraction of the terminal.
    let wide = Rect::new(0, 0, 300, 40);
    assert_eq!(delete_confirm_area(wide).width, 96); // 60% of 160 = 96
    assert_eq!(delete_confirm_area(wide).height, 7);
    assert_eq!(purge_confirm_area(wide).width, 112); // 70% of 160 = 112
    assert_eq!(purge_confirm_area(wide).height, 9);
    assert_eq!(status_overlay_area(wide).width, 80); // 50% of 160 = 80
    assert_eq!(status_overlay_area(wide).height, 7);

    // On a narrow terminal (50 cols), dialogs shrink to terminal_width - 4 margin.
    let narrow = Rect::new(0, 0, 50, 40);
    assert_eq!(delete_confirm_area(narrow).width, 46); // min(96, 50-4) = 46
    assert_eq!(status_overlay_area(narrow).width, 46); // min(80, 46) = 46
}

#[test]
fn modal_overlay_visible_tracks_any_modal_fact() {
    assert!(!modal_overlay_visible(ModalOverlayState::default()));
    assert!(modal_overlay_visible(ModalOverlayState::Status));
    assert!(modal_overlay_visible(ModalOverlayState::SettingsAuth));
    assert!(modal_overlay_visible(ModalOverlayState::DestructiveConfirm));
}

#[test]
fn modal_overlay_state_maps_stage_facts_and_outer_flags() {
    let overlay = modal_overlay_state_from_stage_facts(
        true,
        true,
        ConsoleStageModalFacts {
            editor_modal_open: true,
            settings_error_popup_open: true,
            settings_auth_modal_open: true,
            destructive_confirm_open: true,
            ..ConsoleStageModalFacts::default()
        },
    );

    // status_overlay=true wins the priority order (Status → List → Editor
    // → Settings* → CreatePrelude → DestructiveConfirm).
    assert_eq!(overlay, ModalOverlayState::Status);
    assert!(modal_overlay_visible(overlay));
}

#[test]
fn modal_overlay_state_counts_list_modal_only_on_list_route() {
    let list = modal_overlay_state_for_route(
        ConsoleManagerStageRoute::List,
        false,
        true,
        ConsoleStageModalFacts::default(),
    );
    let editor = modal_overlay_state_for_route(
        ConsoleManagerStageRoute::Editor,
        false,
        true,
        ConsoleStageModalFacts::default(),
    );

    assert_eq!(list, ModalOverlayState::List);
    assert_eq!(editor, ModalOverlayState::None);
}

#[test]
fn list_vertical_clamp_uses_rendered_sidebar_height() {
    use crate::tui::layout::list::{clamp_list_scroll_for_area, selected_sidebar_scroll_areas};
    use crate::tui::state::ManagerState;
    use jackin_config::{AppConfig, MountConfig, MountIsolation, WorkspaceConfig};
    use ratatui::layout::Rect;
    use termrock::scroll::{
        max_offset_u16 as max_scroll_offset, viewport_height as scroll_viewport_height,
    };

    fn split_mount(idx: usize) -> MountConfig {
        MountConfig {
            src: format!("/host/long/source/path/{idx}"),
            dst: format!("/container/long/destination/path/{idx}"),
            readonly: false,
            isolation: MountIsolation::Shared,
        }
    }

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "demo".into(),
        WorkspaceConfig {
            workdir: "/workspace/demo".into(),
            mounts: (0..10).map(split_mount).collect(),
            ..Default::default()
        },
    );
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.selected = 1;

    let body = Rect::new(0, 0, 100, 10);
    let columns = crate::tui::list_geometry::split_list_columns(body, state.list_split_pct);
    let areas =
        selected_sidebar_scroll_areas(columns.preview, &state, &config, tmp.path()).unwrap();
    let rendered_viewport = scroll_viewport_height(areas.workspace.area);
    let desired_viewport = scroll_viewport_height(Rect::new(0, 0, 0, 12));
    assert!(rendered_viewport < desired_viewport);

    let expected = max_scroll_offset(areas.workspace.content_height, rendered_viewport);
    assert!(expected > max_scroll_offset(areas.workspace.content_height, desired_viewport));

    state.list_mounts_scroll_y = u16::MAX;
    clamp_list_scroll_for_area(body, &mut state, &config, tmp.path());

    assert_eq!(state.list_mounts_scroll_y, expected);
}

#[test]
fn tui_header_uses_canonical_brand_wordmark() {
    use ratatui::layout::Rect;

    let backend = TestBackend::new(40, 1);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_header(f, Rect::new(0, 0, 40, 1), "workspaces"))
        .unwrap();

    let buf = term.backend().buffer();
    let dump: String = buf
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();

    assert!(
        dump.contains("jackin❯"),
        "header must render 'jackin❯' (lowercase + chevron wordmark); got {dump:?}"
    );
    assert!(
        !dump.contains("JACKIN"),
        "header must not render 'JACKIN' (uppercase); got {dump:?}"
    );
}

// Product-level dialog composition tests. Exact shared border, title, and
// palette contracts live in TermRock's widget tests and catalog previews.

use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

/// Render a closure into a fresh `TestBackend` and return the resulting
/// buffer. Size is chosen to comfortably fit every modal under test.
fn draw<F: FnOnce(&mut Frame<'_>)>(width: u16, height: u16, render: F) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render(f)).unwrap();
    term.backend().buffer().clone()
}

// Note: the former `assert_hint_row_present` helper and
// `all_modal_hint_rows_use_canonical_styles` test were removed when hint
// lines moved out of widget interiors into the main footer. Widgets no
// longer render an internal hint row; the footer is the single source of
// truth for available key hints.

/// Build and render the `SaveDiscardCancel` modal into a full-area
/// buffer. Returns (buffer, area).
fn render_save_discard() -> (Buffer, Rect) {
    use crate::tui::components::{SaveDiscardState, render_save_discard_dialog as render};
    let area = Rect::new(0, 0, 70, 7);
    let state = SaveDiscardState::new("Save changes?");
    let buf = draw(area.width, area.height, |f| render(f, area, &state));
    (buf, area)
}

fn render_confirm() -> (Buffer, Rect) {
    use crate::tui::components::{ConfirmState, render_confirm_dialog as render};
    let area = Rect::new(0, 0, 60, 7);
    let state = ConfirmState::new("Delete workspace?");
    let buf = draw(area.width, area.height, |f| render(f, area, &state));
    (buf, area)
}

fn render_mount_dst() -> (Buffer, Rect) {
    use crate::tui::components::mount_dst_choice::{MountDstChoiceState, render};
    let area = Rect::new(0, 0, 80, 8);
    let state = MountDstChoiceState::new("/home/user/app");
    let buf = draw(area.width, area.height, |f| render(f, area, &state));
    (buf, area)
}

fn render_confirm_save() -> (Buffer, Rect) {
    use crate::tui::components::confirm_save::{ConfirmSaveState, render};
    use ratatui::text::Line;
    let area = Rect::new(0, 0, 70, 10);
    let state = ConfirmSaveState::<jackin_config::MountConfig>::new(vec![
        Line::from("Create workspace: demo"),
        Line::from(""),
        Line::from("Working directory: /home/user/demo"),
    ]);
    let buf = draw(area.width, area.height, |f| render(f, area, &state));
    (buf, area)
}

fn row_text(buf: &Buffer, y: u16) -> String {
    (buf.area.x..buf.area.x + buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect()
}

fn button_row_y(buf: &Buffer, labels: &[&str]) -> u16 {
    (buf.area.y..buf.area.y + buf.area.height)
        .find(|y| {
            let row = row_text(buf, *y);
            labels.iter().all(|label| row.contains(label))
        })
        .expect("button row should be visible")
}

/// Every dialog with action buttons renders exactly one empty row above
/// that button row.
#[test]
fn dialog_button_rows_have_one_blank_row_above() {
    for (name, (buf, _area), labels) in [
        (
            "SaveDiscardCancel",
            render_save_discard(),
            &["Save", "Discard", "Cancel"][..],
        ),
        ("Confirm", render_confirm(), &["Yes", "No"][..]),
        (
            "MountDstChoice",
            render_mount_dst(),
            &["Mount at same path", "Edit destination", "Cancel"][..],
        ),
        (
            "ConfirmSave",
            render_confirm_save(),
            &["Save", "Cancel"][..],
        ),
    ] {
        let button_y = button_row_y(&buf, labels);
        assert!(
            button_y > buf.area.y,
            "{name} button row cannot be first row"
        );
        let before = row_text(&buf, button_y - 1);
        let non_space_cells = before.chars().filter(|ch| !ch.is_whitespace()).count();
        assert!(
            non_space_cells <= 2,
            "{name} must have one blank row above buttons; got {before:?}",
        );
    }
}

// `all_modal_hint_rows_use_canonical_styles` test removed — hints moved to footer.

// Snapshot tests for the TUI view layer.
//
// Uses `insta` to pin the text output of key view functions. Any change to
// rendered output fails CI until reviewed and accepted with `cargo insta review`.
// This is the Phase 0 regression net for the TUI architecture refactor.
//
// Initial snapshots are generated by running:
// ```sh
// INSTA_UPDATE=new cargo test -p jackin-console -- view::tests --nocapture
// ```

use crate::tui::{
    state::{
        EditorState, EditorTab, GlobalMountConfirm, ManagerStage, ManagerState, Modal,
        MountScrollFocus, SettingsEnvScope, SettingsEnvTextTarget, SettingsModal, SettingsState,
    },
    view::{prepare_for_render, render},
};
use jackin_config::{AppConfig, WorkspaceConfig};

fn render_manager_state(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    width: u16,
    height: u16,
) -> String {
    let buf = render_manager_buffer(state, config, cwd, width, height);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_manager_buffer(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    width: u16,
    height: u16,
) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    prepare_for_render(state, config, cwd, area);
    terminal
        .draw(|frame| render(frame, area, state, config, cwd))
        .unwrap();
    terminal.backend().buffer().clone()
}

#[expect(
    clippy::excessive_nesting,
    reason = "Focused-region flood fill has nested traversal and membership checks"
)]
fn focused_region_count(buf: &Buffer) -> usize {
    let area = buf.area;
    let mut seen = std::collections::BTreeSet::<(u16, u16)>::new();
    let mut clusters = 0usize;

    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let coord = (x, y);
            if seen.contains(&coord) || !is_focused_region_cell(buf, coord) {
                continue;
            }
            clusters += 1;
            let mut stack = vec![coord];
            seen.insert(coord);
            while let Some((cx, cy)) = stack.pop() {
                for next in neighbors(cx, cy, area) {
                    if seen.insert(next) && is_focused_region_cell(buf, next) {
                        stack.push(next);
                    }
                }
            }
        }
    }

    clusters
}

fn neighbors(x: u16, y: u16, area: Rect) -> impl Iterator<Item = (u16, u16)> {
    let min_x = area.x;
    let min_y = area.y;
    let max_x = area.x + area.width - 1;
    let max_y = area.y + area.height - 1;
    [
        x.checked_sub(1).map(|nx| (nx, y)),
        (x < max_x).then_some((x + 1, y)),
        y.checked_sub(1).map(|ny| (x, ny)),
        (y < max_y).then_some((x, y + 1)),
    ]
    .into_iter()
    .flatten()
    .filter(move |(nx, ny)| *nx >= min_x && *ny >= min_y)
}

fn is_focused_region_cell(buf: &Buffer, coord: (u16, u16)) -> bool {
    let cell = &buf[coord];
    // Product focus chrome via jackin helpers (TermRock owns Role→RGB tables).
    let focused = termrock::Theme::default()
        .style(termrock::style::Role::BorderFocused)
        .fg;
    cell.fg == focused.unwrap_or_default()
}

fn test_cwd() -> std::path::PathBuf {
    std::path::PathBuf::from("/workspace")
}

fn detail_config() -> AppConfig {
    toml::from_str(
        r#"
[roles."chainargos/agent-smith"]
git = "https://example.invalid/agent-smith.git"

[docker.mounts]
cache = { src = "/cache", dst = "/cache", readonly = false }

[docker.mounts."chainargos/agent-smith"]
secrets = { src = "/secrets", dst = "/secrets", readonly = true }

[workspaces.ws]
workdir = "/workspace"
allowed_roles = ["chainargos/agent-smith"]

[[workspaces.ws.mounts]]
src = "/workspace"
dst = "/workspace"
readonly = false
"#,
    )
    .expect("valid detail-pane config")
}

fn list_with_modal<'a>(
    config: &AppConfig,
    cwd: &std::path::Path,
    modal: Modal<'a>,
) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(config, cwd);
    state.list_modal = Some(modal);
    state
}

fn settings_mounts_with_modal<'a>(
    config: &AppConfig,
    cwd: &std::path::Path,
    modal: SettingsModal<'a>,
) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
    settings.active_tab = crate::tui::state::SettingsTab::Mounts;
    settings.set_active_content_focused(true);
    settings.mounts.modal = Some(modal);
    state.stage = ManagerStage::Settings(settings);
    state
}

fn settings_env_with_modal<'a>(
    config: &AppConfig,
    cwd: &std::path::Path,
    modal: SettingsModal<'a>,
) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
    settings.active_tab = crate::tui::state::SettingsTab::Environments;
    settings.set_active_content_focused(true);
    settings.env.modal = Some(modal);
    state.stage = ManagerStage::Settings(settings);
    state
}

fn settings_auth_with_modal(
    config: &AppConfig,
    cwd: &std::path::Path,
    modal: SettingsModal<'static>,
) -> ManagerState<'static> {
    let mut state = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
    settings.active_tab = crate::tui::state::SettingsTab::Auth;
    settings.set_active_content_focused(true);
    settings.auth.modal = Some(modal);
    state.stage = ManagerStage::Settings(settings);
    state
}

fn auth_form_modal() -> Modal<'static> {
    let kind = crate::tui::auth::AuthKind::Claude;
    Modal::AuthForm {
        target: crate::tui::state::AuthFormTarget::Workspace { kind },
        state: Box::new(crate::tui::state::AuthForm::new(kind)),
        focus: crate::tui::state::AuthFormFocus::Mode,
        literal_buffer: String::new(),
    }
}

#[test]
fn snapshot_list_empty_80x24() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let rendered = render_manager_state(&mut state, &config, &cwd, 80, 24);
    insta::assert_snapshot!("list_empty_80x24", rendered);
}

#[test]
fn new_workspace_hints_stay_in_footer() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 1;

    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 24);

    assert!(
        !rendered.contains("Press Enter"),
        "new-workspace body must not render keyboard hints inline:\n{rendered}"
    );
    assert!(
        rendered.contains("setup"),
        "footer must own the Enter/setup hint:\n{rendered}"
    );
}

#[test]
fn snapshot_settings_general_90x20() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.stage = ManagerStage::Settings(SettingsState::from_config(&config));
    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);
    insta::assert_snapshot!("settings_general_90x20", rendered);
}

#[test]
fn snapshot_editor_general_90x20() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "my-workspace".into(),
        WorkspaceConfig::default(),
    ));
    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);
    insta::assert_snapshot!("editor_general_90x20", rendered);
}

#[test]
fn editor_general_content_focus_shows_cursor() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("my-workspace".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.set_tab_content_scroll_focused(true);
    state.stage = ManagerStage::Editor(editor);

    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);

    assert!(
        rendered.contains("▸ Name"),
        "focused General tab must show the same cursor signal as its green border:\n{rendered}"
    );
}

#[test]
fn snapshot_editor_mounts_tab_90x20() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;
    state.stage = ManagerStage::Editor(editor);
    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);
    insta::assert_snapshot!("editor_mounts_tab_90x20", rendered);
}

#[test]
fn host_console_content_states_project_visible_focus() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut cases: Vec<(&str, ManagerState<'_>)> = Vec::new();

    let mut list = ManagerState::from_config(&config, &cwd);
    list.set_list_names_focused(true);
    cases.push(("list", list));

    for (name, tab) in [
        ("editor general", EditorTab::General),
        ("editor mounts", EditorTab::Mounts),
        ("editor roles", EditorTab::Roles),
        ("editor secrets", EditorTab::Secrets),
        ("editor auth", EditorTab::Auth),
    ] {
        let mut state = ManagerState::from_config(&config, &cwd);
        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.active_tab = tab;
        editor.set_tab_bar_focused(false);
        editor.set_tab_content_scroll_focused(true);
        editor.set_workspace_mounts_scroll_focused(tab == EditorTab::Mounts);
        state.stage = ManagerStage::Editor(editor);
        cases.push((name, state));
    }

    for tab in crate::tui::state::SettingsTab::ALL {
        let mut state = ManagerState::from_config(&config, &cwd);
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = tab;
        settings.set_active_content_focused(true);
        state.stage = ManagerStage::Settings(settings);
        cases.push((tab.label(), state));
    }

    for (name, mut state) in cases {
        let buf = render_manager_buffer(&mut state, &config, &cwd, 100, 28);
        assert!(
            focused_region_count(&buf) >= 1,
            "{name} must project focused state into the rendered composition"
        );
    }
}

#[test]
fn host_console_list_detail_transitions_project_visible_focus() {
    let cwd = test_cwd();

    let mut cases: Vec<(&str, AppConfig, ManagerState<'_>)> = Vec::new();

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(true);
    cases.push(("current dir list focus", config, state));

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(false);
    state.set_list_scroll_focus(Some(MountScrollFocus::Workspace));
    cases.push(("current dir mounts focus", config, state));

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(false);
    state.set_list_scroll_focus(Some(MountScrollFocus::Global));
    cases.push(("current dir global mounts focus", config, state));

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(false);
    state.set_list_scroll_focus(Some(MountScrollFocus::Global));
    cases.push(("current dir global mounts focus", config, state));

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 1;
    state.set_list_names_focused(true);
    cases.push(("saved workspace list focus", config, state));

    for (name, focus) in [
        ("saved workspace mounts focus", MountScrollFocus::Workspace),
        (
            "saved workspace global mounts focus",
            MountScrollFocus::Global,
        ),
        (
            "saved workspace role global mounts focus",
            MountScrollFocus::RoleGlobal,
        ),
        ("saved workspace roles focus", MountScrollFocus::Roles),
    ] {
        let config = detail_config();
        let mut state = ManagerState::from_config(&config, &cwd);
        state.selected = 1;
        state.set_list_names_focused(false);
        state.set_list_scroll_focus(Some(focus));
        cases.push((name, config, state));
    }

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 2;
    state.set_list_names_focused(true);
    cases.push(("new workspace detail focus", config, state));

    for (name, config, mut state) in cases {
        let buf = render_manager_buffer(&mut state, &config, &cwd, 110, 30);
        assert!(
            focused_region_count(&buf) >= 1,
            "{name} must project focused state into the rendered composition"
        );
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "Product-state projection table enumerates each modal and pane-tone combination"
)]
fn host_console_modal_states_project_visible_focus() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut cases: Vec<(&str, ManagerState<'_>)> = Vec::new();

    let mut confirm_delete = ManagerState::from_config(&config, &cwd);
    confirm_delete.stage = ManagerStage::ConfirmDelete {
        name: "ws".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete workspace?"),
    };
    cases.push(("list confirm delete", confirm_delete));

    cases.push((
        "list confirm modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::Confirm {
                target: crate::tui::state::ConfirmTarget::DeleteEnvVar {
                    scope: crate::tui::state::SecretsScopeTag::Workspace,
                    key: "TOKEN".into(),
                },
                state: crate::tui::components::ConfirmState::new("Delete TOKEN?"),
            },
        ),
    ));

    cases.push((
        "list save discard modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::SaveDiscardCancel {
                state: crate::tui::components::SaveDiscardState::new("Save changes?"),
            },
        ),
    ));

    cases.push((
        "list status modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::StatusPopup {
                state: crate::tui::components::StatusPopupState::new("Loading", "Resolving role"),
            },
        ),
    ));

    cases.push((
        "list file browser modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::FileBrowser {
                target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
                state: crate::tui::components::file_browser::FileBrowserState::from_listing(
                    crate::services::file_browser::listing_at(cwd.clone(), cwd.clone()),
                ),
            },
        ),
    ));

    cases.push((
        "list mount dst choice modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::MountDstChoice {
                target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
                state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new(
                    "/workspace",
                ),
            },
        ),
    ));

    cases.push((
        "list workdir picker modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::WorkdirPick {
                state: crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(&[
                    jackin_config::MountConfig {
                        src: "/workspace".into(),
                        dst: "/workspace".into(),
                        readonly: false,
                        isolation: jackin_config::MountIsolation::Shared,
                    },
                ]),
            },
        ),
    ));

    cases.push((
        "list github picker modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::GithubPicker {
                state: crate::tui::components::github_picker::GithubPickerState::new(vec![
                    crate::github_mounts::GithubChoice {
                        src: "/workspace".into(),
                        branch: "main".into(),
                        url: "https://github.com/example/repo".into(),
                    },
                ]),
            },
        ),
    ));

    cases.push((
        "list role picker modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::RolePicker {
                state: crate::tui::state::RolePickerState::new(vec![
                    jackin_core::RoleSelector::parse("chainargos/agent-smith")
                        .expect("valid role selector"),
                ]),
            },
        ),
    ));

    cases.push((
        "list source picker modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::SourcePicker {
                state: crate::tui::components::source_picker::SourcePickerState::new(
                    "TOKEN".into(),
                    true,
                ),
                env_key: None,
            },
        ),
    ));

    cases.push((
        "list scope picker modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::ScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    let mut editor_text = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.set_tab_content_scroll_focused(true);
    editor.modal = Some(Modal::TextInput {
        target: crate::tui::state::TextInputTarget::Name,
        state: crate::tui::components::TextInputState::new("Name", "ws"),
    });
    editor_text.stage = ManagerStage::Editor(editor);
    cases.push(("editor text input", editor_text));

    let mut editor_state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::ContainerInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState::new(
            "Container",
            vec![
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Run ID", "abc",
                ),
            ],
        ),
    });
    editor_state.stage = ManagerStage::Editor(editor);
    cases.push(("editor container info", editor_state));

    let mut editor_op_picker = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::OpPicker {
        secrets_target: None,
        state: Box::new(crate::tui::op_picker::OpPickerState::new()),
    });
    editor_op_picker.stage = ManagerStage::Editor(editor);
    cases.push(("editor op picker", editor_op_picker));

    let mut editor_role_override = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::RoleOverridePicker {
        state: crate::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-smith")
                .expect("valid role selector"),
        ]),
    });
    editor_role_override.stage = ManagerStage::Editor(editor);
    cases.push(("editor role override picker", editor_role_override));

    let mut editor_auth_role = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::AuthRolePicker {
        state: crate::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-smith")
                .expect("valid role selector"),
        ]),
    });
    editor_auth_role.stage = ManagerStage::Editor(editor);
    cases.push(("editor auth role picker", editor_auth_role));

    let mut editor_auth_source = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::AuthSourcePicker {
        state: crate::tui::components::source_picker::SourcePickerState::new(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            true,
        ),
    });
    editor_auth_source.stage = ManagerStage::Editor(editor);
    cases.push(("editor auth source picker", editor_auth_source));

    let mut editor_auth_form = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(auth_form_modal());
    editor_auth_form.stage = ManagerStage::Editor(editor);
    cases.push(("editor auth form", editor_auth_form));

    let mut settings_mounts_confirm = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = crate::tui::state::SettingsTab::Mounts;
    settings.set_active_content_focused(true);
    settings.mounts.modal = Some(SettingsModal::MountConfirm {
        action: GlobalMountConfirm::Remove,
        state: crate::tui::components::ConfirmState::new("Remove mount?"),
    });
    settings_mounts_confirm.stage = ManagerStage::Settings(settings);
    cases.push(("settings mounts confirm", settings_mounts_confirm));

    cases.push((
        "settings mounts text",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountText {
                target: crate::tui::state::GlobalMountTextTarget::AddName,
                state: Box::new(crate::tui::components::TextInputState::new(
                    "Mount name",
                    "repo",
                )),
            },
        ),
    ));

    cases.push((
        "settings mounts file browser",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountFileBrowser {
                state: Box::new(
                    crate::tui::components::file_browser::FileBrowserState::from_listing(
                        crate::services::file_browser::listing_at(cwd.clone(), cwd.clone()),
                    ),
                ),
            },
        ),
    ));

    cases.push((
        "settings mounts destination choice",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountDstChoice {
                state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new(
                    "/workspace",
                ),
            },
        ),
    ));

    cases.push((
        "settings mounts scope picker",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    cases.push((
        "settings mounts role picker",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountRolePicker {
                state: crate::tui::state::RolePickerState::new(vec![
                    jackin_core::RoleSelector::parse("chainargos/agent-smith")
                        .expect("valid role selector"),
                ]),
            },
        ),
    ));

    cases.push((
        "settings mounts preview save",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountPreviewSave {
                state: crate::tui::components::confirm_save::ConfirmSaveState::new(vec![
                    ratatui::text::Line::from("Add global mount /workspace"),
                ]),
            },
        ),
    ));

    let mut settings_env_text = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = crate::tui::state::SettingsTab::Environments;
    settings.set_active_content_focused(true);
    settings.env.modal = Some(SettingsModal::EnvText {
        target: SettingsEnvTextTarget::EnvKey {
            scope: SettingsEnvScope::Global,
        },
        pending_value: None,
        state: Box::new(crate::tui::components::TextInputState::new(
            "Environment key",
            "TOKEN",
        )),
    });
    settings_env_text.stage = ManagerStage::Settings(settings);
    cases.push(("settings env text", settings_env_text));

    cases.push((
        "settings env source picker",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvSourcePicker {
                key: (SettingsEnvScope::Global, "TOKEN".to_owned()),
                state: crate::tui::components::source_picker::SourcePickerState::new(
                    "TOKEN".into(),
                    true,
                ),
            },
        ),
    ));

    cases.push((
        "settings env op picker",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvOpPicker {
                target: crate::tui::state::SettingsEnvOpPickerTarget::Existing {
                    scope: SettingsEnvScope::Global,
                    key: "TOKEN".to_owned(),
                },
                state: Box::new(crate::tui::op_picker::OpPickerState::new()),
            },
        ),
    ));

    cases.push((
        "settings env role picker",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvRolePicker {
                state: crate::tui::state::RolePickerState::new(vec![
                    jackin_core::RoleSelector::parse("chainargos/agent-smith")
                        .expect("valid role selector"),
                ]),
            },
        ),
    ));

    cases.push((
        "settings env scope picker",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    cases.push((
        "settings env confirm",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvConfirm {
                action: crate::tui::state::SettingsEnvConfirm::Delete,
                state: crate::tui::components::ConfirmState::new("Delete env var?"),
            },
        ),
    ));

    let mut settings_auth_text = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = crate::tui::state::SettingsTab::Auth;
    settings.set_active_content_focused(true);
    settings.auth.modal = Some(SettingsModal::AuthTextInput {
        state: Box::new(crate::tui::components::TextInputState::new(
            "Credential",
            "token",
        )),
    });
    settings_auth_text.stage = ManagerStage::Settings(settings);
    cases.push(("settings auth text", settings_auth_text));

    cases.push((
        "settings auth source picker",
        settings_auth_with_modal(
            &config,
            &cwd,
            SettingsModal::AuthSourcePicker {
                state: crate::tui::components::source_picker::SourcePickerState::new(
                    "CLAUDE_CODE_OAUTH_TOKEN".into(),
                    true,
                ),
            },
        ),
    ));

    cases.push((
        "settings auth op picker",
        settings_auth_with_modal(
            &config,
            &cwd,
            SettingsModal::AuthOpPicker {
                state: Box::new(crate::tui::op_picker::OpPickerState::new()),
            },
        ),
    ));

    let kind = crate::tui::auth::AuthKind::Claude;
    cases.push((
        "settings auth form",
        settings_auth_with_modal(
            &config,
            &cwd,
            SettingsModal::AuthForm {
                target: crate::tui::state::AuthFormTarget::Workspace { kind },
                state: Box::new(crate::tui::state::AuthForm::new(kind)),
                focus: crate::tui::state::AuthFormFocus::Mode,
                literal_buffer: String::new(),
            },
        ),
    ));

    for (name, mut state) in cases {
        let buf = render_manager_buffer(&mut state, &config, &cwd, 100, 28);
        assert!(
            focused_region_count(&buf) >= 1,
            "{name} must project focused state into the rendered composition"
        );
    }
}

#[test]
fn snapshot_global_mounts_110x30() {
    let config = detail_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(false);
    state.set_list_scroll_focus(Some(MountScrollFocus::Global));
    let rendered = render_manager_state(&mut state, &config, &cwd, 110, 30);
    insta::assert_snapshot!("global_mounts_110x30", rendered);
}

#[test]
fn snapshot_editor_auth_tab_90x20() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Auth;
    editor.set_tab_bar_focused(false);
    editor.set_tab_content_scroll_focused(true);
    state.stage = ManagerStage::Editor(editor);
    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);
    insta::assert_snapshot!("editor_auth_tab_90x20", rendered);
}
