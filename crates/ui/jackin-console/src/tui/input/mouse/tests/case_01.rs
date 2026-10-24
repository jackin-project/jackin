// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn painted_editor_label_position(
    editor: &EditorState<'_>,
    config: &jackin_config::AppConfig,
    terminal_area: Rect,
    op_available: bool,
    label: &str,
) -> ratatui::layout::Position {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("editor TestBackend should initialize");
    terminal
        .draw(|frame| {
            crate::tui::screens::editor::view::render_editor_with_footer(
                frame,
                frame.area(),
                editor,
                config,
                op_available,
            );
        })
        .expect("editor screen should render to TestBackend");

    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .find_map(|row| {
            let text = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(label)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                u16::try_from(column).expect("painted editor label column fits u16"),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("actual editor TestBackend buffer lacks {label:?}"))
}

fn painted_github_picker_row_position(
    state: &ManagerState<'_>,
    terminal_area: Rect,
    marker: &str,
) -> ratatui::layout::Position {
    use ratatui::{Terminal, backend::TestBackend};

    let modal = state
        .list_modal
        .as_ref()
        .expect("GitHub picker modal should be open before paint");
    let Modal::GithubPicker { state: picker } = modal else {
        panic!("GitHub picker modal expected before paint");
    };
    let modal_area = modal.rect(terminal_area);
    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("GitHub picker TestBackend should initialize");
    terminal
        .draw(|frame| {
            crate::tui::components::github_picker::render(frame, modal_area, picker);
        })
        .expect("GitHub picker should render to TestBackend");

    let buffer = terminal.backend().buffer();
    (modal_area.y..modal_area.bottom())
        .find_map(|row| {
            let text = (modal_area.x..modal_area.right())
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(marker)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                modal_area.x.saturating_add(
                    u16::try_from(column).expect("painted GitHub row column fits u16"),
                ),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("actual GitHub picker TestBackend buffer lacks {marker:?}"))
}

fn github_choices(count: usize) -> Vec<crate::github_mounts::GithubChoice> {
    (0..count)
        .map(|index| crate::github_mounts::GithubChoice {
            src: format!("/workspace/repository-{index:02}"),
            branch: format!("branch-{index:02}-painted"),
            url: format!("https://github.invalid/owner/repository-{index:02}"),
        })
        .collect()
}

#[test]
fn conformance_wire_mouse_coordinates_become_only_semantic_action() -> anyhow::Result<()> {
    const CHILD: &str = "JACKIN_MOUSE_PRIVACY_WIRE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "tui::input::mouse::tests::case_01::conformance_wire_mouse_coordinates_become_only_semantic_action",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()?;
        anyhow::ensure!(status.success(), "isolated mouse privacy test failed");
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )?;
    let config = jackin_config::AppConfig::default();
    let mut state = list_state();
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "wire-private-workspace".into(),
        WorkspaceConfig::default(),
    ));
    let terminal_area = term(100);
    let position = {
        let ManagerStage::Editor(editor) = &state.stage else {
            anyhow::bail!("expected editor stage before paint");
        };
        painted_editor_label_position(editor, &config, terminal_area, state.op_available, "Mounts")
    };
    let coordinate = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: position.x,
        row: position.y,
        modifiers: KeyModifiers::NONE,
    };
    handle_mouse_with_config(&mut state, coordinate, terminal_area, Some(&config));
    let ManagerStage::Editor(editor) = &state.stage else {
        anyhow::bail!("mouse route left editor stage");
    };
    anyhow::ensure!(
        editor.active_tab == EditorTab::Mounts,
        "coordinates did not reach the real tab hit-test"
    );
    drop(jackin_telemetry::ui::take_action_parent());
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|reason| anyhow::anyhow!("validation event rejected: {reason:?}"))?;
    jackin_diagnostics::flush_wire_test_export()?;
    drop(runtime_guard);
    anyhow::ensure!(
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))),
        "mouse route did not export all three signals"
    );
    let action_spans = testbed
        .spans()
        .into_iter()
        .filter(|span| span.name == "ui.action")
        .collect::<Vec<_>>();
    anyhow::ensure!(action_spans.len() == 1, "unexpected ui.action spans");
    let action = &action_spans[0];
    anyhow::ensure!(
        format!("{:?}", action.attributes).contains("tab.switch"),
        "mouse action was not exported semantically: {action:?}"
    );
    let ui_action_count = testbed
        .metrics()
        .into_iter()
        .flat_map(|request| request.resource_metrics)
        .flat_map(|resource| resource.scope_metrics)
        .flat_map(|scope| scope.metrics)
        .filter(|metric| metric.name == "ui.actions")
        .filter_map(|metric| metric.data)
        .filter_map(|data| match data {
            opentelemetry_proto::tonic::metrics::v1::metric::Data::Sum(sum) => Some(sum),
            _ => None,
        })
        .flat_map(|sum| sum.data_points)
        .filter_map(|point| point.value)
        .map(|value| match value {
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsInt(value) => {
                value as f64
            }
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsDouble(value) => {
                value
            }
        })
        .sum::<f64>();
    anyhow::ensure!(
        (ui_action_count - 1.0).abs() < f64::EPSILON,
        "one semantic mouse action must increment ui.actions exactly once, got {ui_action_count}"
    );
    for key in action
        .attributes
        .iter()
        .map(|attribute| attribute.key.as_str())
        .chain(testbed.metric_dimension_keys().iter().map(String::as_str))
    {
        anyhow::ensure!(
            !matches!(
                key,
                "row" | "column" | "mouse.row" | "mouse.column" | "ui.pointer.x" | "ui.pointer.y"
            ),
            "raw pointer coordinate key escaped to OTLP: {key}"
        );
    }
    anyhow::ensure!(
        testbed
            .prohibited_value_violations(&["wire-private-workspace"])
            .is_empty(),
        "private editor identity escaped to OTLP"
    );
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn content_areas_exclude_the_cached_footer() {
    let term = Rect::new(0, 0, 80, 24);

    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.cached_footer_h = 3;
    let s = settings.content_area(term);
    assert_eq!(s.y, SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT);
    assert_eq!(
        s.y + s.height,
        term.height - 3,
        "settings content must stop where the footer begins"
    );

    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.cached_footer_h = 4;
    let e = editor.content_area(term);
    assert_eq!(
        e.y + e.height,
        term.height - 4,
        "editor content must stop where the footer begins"
    );
}

#[test]
fn mouse_down_on_seam_starts_drag() {
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
    let mut state = list_state();
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
    assert!(state.drag_state.is_none(), "right-pane click must not drag");
}

#[test]
fn drag_ignored_when_list_modal_open() {
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![MountConfig {
            src: "/w".into(),
            dst: "/w".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    drop(crate::github_mounts::resolve_for_workspace(&ws));
    state.list_modal = Some(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(vec![
            crate::github_mounts::GithubChoice {
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
fn list_github_picker_wheel_scrolls_modal_selection() {
    let mut state = list_state();
    state.list_modal = Some(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(github_choices(2)),
    });
    let term = term_120x40();
    let position = painted_github_picker_row_position(&state, term, "branch-01-painted");

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, position.x, position.y),
        term,
    );

    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("github picker modal expected");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(1));
}

#[test]
fn list_github_picker_wheel_misses_preserve_selection_and_background() {
    let mut state = list_state_with_saved(3);
    state.selected = 1;
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 4);
    state
        .list_focus_owner
        .focus_content(MountScrollFocus::Workspace);
    state.list_modal = Some(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(github_choices(3)),
    });
    let term = term_120x40();
    let modal_area = state
        .list_modal
        .as_ref()
        .expect("GitHub picker modal should be open")
        .rect(term);
    let original_selected = state.selected;
    let original_focus = state.list_focus_owner.clone();
    let original_scroll = state.list_names_scroll.offset_y();

    for (column, row) in [(modal_area.x.saturating_add(2), modal_area.y), (0, 0)] {
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, column, row),
            term,
        );
        let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
            panic!("GitHub picker modal expected after an unpainted wheel event");
        };
        assert_eq!(
            picker.list_state.selected().copied(),
            Some(0),
            "an unpainted modal or outside cell must not scroll the picker"
        );
        assert_eq!(state.selected, original_selected);
        assert_eq!(state.list_focus_owner, original_focus);
        assert_eq!(state.list_names_scroll.offset_y(), original_scroll);
    }

    let painted_row = painted_github_picker_row_position(&state, term, "branch-00-painted");
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, painted_row.x, painted_row.y),
        term,
    );
    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("hidden GitHub picker modal should remain stored");
    };
    assert_eq!(
        picker.list_state.selected().copied(),
        Some(0),
        "a blocked owner must keep the hidden GitHub picker unchanged"
    );
    assert_eq!(state.selected, original_selected);
    assert_eq!(state.list_focus_owner, original_focus);
    assert_eq!(state.list_names_scroll.offset_y(), original_scroll);
}

#[test]
fn list_github_picker_wheel_edges_do_not_wrap_or_scroll_background() {
    let mut state = list_state_with_saved(3);
    state.selected = 1;
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 4);
    state
        .list_focus_owner
        .focus_content(MountScrollFocus::Workspace);
    state.list_modal = Some(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(github_choices(2)),
    });
    let term = term_120x40();
    let original_selected = state.selected;
    let original_focus = state.list_focus_owner.clone();
    let original_scroll = state.list_names_scroll.offset_y();
    let first_row = painted_github_picker_row_position(&state, term, "branch-00-painted");

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollUp, first_row.x, first_row.y),
        term,
    );
    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("GitHub picker modal expected after first-row wheel event");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(0));

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, first_row.x, first_row.y),
        term,
    );
    let second_row = painted_github_picker_row_position(&state, term, "branch-01-painted");
    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("GitHub picker modal expected after non-edge wheel event");
    };
    assert_eq!(
        picker.list_state.selected().copied(),
        Some(1),
        "a valid non-edge wheel event should move to the next choice"
    );

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, second_row.x, second_row.y),
        term,
    );
    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("GitHub picker modal expected after last-row wheel event");
    };
    assert_eq!(
        picker.list_state.selected().copied(),
        Some(1),
        "a valid last-row wheel event must be consumed without wrapping"
    );
    assert_eq!(state.selected, original_selected);
    assert_eq!(state.list_focus_owner, original_focus);
    assert_eq!(state.list_names_scroll.offset_y(), original_scroll);
}

#[test]
fn editor_workdir_picker_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let mounts = vec![MountConfig {
        src: "/workspace/project".into(),
        dst: "/workspace/project".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    }];
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::WorkdirPick {
        state: crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(&mounts),
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 60, 20),
        term_120x40(),
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        editor.tab_scroll.offset_y(),
        0,
        "background editor must not scroll"
    );
    let Some(Modal::WorkdirPick { state: picker }) = &editor.modal else {
        panic!("workdir picker modal expected");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(1));
}

#[test]
fn settings_role_picker_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    crate::tui::scroll_block::scroll_area_set_y(&mut settings.mounts.scroll, 4);
    settings.mounts.modals.open(SettingsModal::MountRolePicker {
        state: crate::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-brown").unwrap(),
            jackin_core::RoleSelector::parse("scentbird/agent-jones").unwrap(),
        ]),
    });
    state.stage = ManagerStage::Settings(settings);

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 60, 20),
        term_120x40(),
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("settings stage expected");
    };
    assert_eq!(
        settings.mounts.scroll.offset_y(),
        4,
        "background settings must not scroll"
    );
    let Some(SettingsModal::MountRolePicker { state: picker }) = settings.mounts.modals.current()
    else {
        panic!("settings role picker modal expected");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(1));
}
