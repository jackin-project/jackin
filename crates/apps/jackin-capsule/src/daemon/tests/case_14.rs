// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn apply_action_open_container_info_pushes_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.status.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status.status_bar.role = "test-role".to_owned();

    mux.apply_action(Action::OpenContainerInfo);

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            container_name,
            copied_row: None,
            ..
        }) if container_name == "jk-test-container"
    ));
}

#[test]
fn apply_action_open_rename_tab_pushes_dialog() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::OpenRenameTab(0))
        .expect("open rename dialog should redraw");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::RenameTab { tab_idx: 0, .. })
    ));
    assert!(mux.render.last_tab_click.is_none());
    assert!(
        !frame_contains_screen_erase(&frame),
        "open rename dialog must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_switch_tab_moves_active_tab() {
    let mut mux = single_pane_tab_mux();
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("Shell", 2, "test"));
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));

    mux.apply_action(Action::SwitchTab(1));

    assert_eq!(mux.session_supervisor.active_tab, 1);
}

#[test]
fn tab_bar_focus_key_maps_arrows_and_exit() {
    use super::input_dispatch::{TabBarFocusKey, tab_bar_focus_key};
    assert_eq!(tab_bar_focus_key(b"\x1b[C"), Some(TabBarFocusKey::Next)); // Right
    assert_eq!(tab_bar_focus_key(b"\x1b[D"), Some(TabBarFocusKey::Prev)); // Left
    assert_eq!(tab_bar_focus_key(b"\x1b[B"), Some(TabBarFocusKey::Exit)); // Down
    assert_eq!(tab_bar_focus_key(b"\x1b"), Some(TabBarFocusKey::Exit)); // Esc
    assert_eq!(tab_bar_focus_key(b"x"), None);
}

#[test]
fn tab_bar_focus_mode_arrows_switch_tabs_then_esc_returns_to_agent() {
    // P5: while the tab bar is focused, Left/Right switch agent tabs and Esc
    // returns focus to the agent content.
    let mut mux = single_pane_tab_mux();
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("Shell", 2, "test"));
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));

    mux.set_tab_bar_focused(true);
    assert!(mux.render.tab_bar_focused);

    mux.handle_input(InputEvent::Data(b"\x1b[C".to_vec())); // Right → next tab
    assert_eq!(mux.session_supervisor.active_tab, 1);
    mux.handle_input(InputEvent::Data(b"\x1b[D".to_vec())); // Left → previous tab
    assert_eq!(mux.session_supervisor.active_tab, 0);
    assert!(mux.render.tab_bar_focused, "arrows keep the bar focused");

    mux.handle_input(InputEvent::Data(b"\x1b".to_vec())); // Esc → back to agent
    assert!(!mux.render.tab_bar_focused);
}

#[test]
fn capsule_widget_focus_lifecycle_is_exported_without_runtime_identity() {
    let _telemetry_guard = crate::support::telemetry_test_guard();
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(true);
    tracing::subscriber::with_default(subscriber, || {
        let mut mux = test_mux(40, 80);
        assert_eq!(mux.widget_focus.current_widget(), Some("capsule.pane"));
        mux.synthesise_focus_swap(Some(41), Some(42));

        mux.set_tab_bar_focused(true);
        assert_eq!(mux.widget_focus.current_widget(), Some("capsule.tab"));

        mux.open_command_palette();
        assert_eq!(
            mux.widget_focus.current_widget(),
            Some("capsule.command_palette")
        );

        mux.dialog_clear();
        assert_eq!(mux.widget_focus.current_widget(), Some("capsule.tab"));
        mux.set_tab_bar_focused(false);
        assert_eq!(mux.widget_focus.current_widget(), Some("capsule.pane"));
        mux.widget_focus.unfocus().unwrap();
    });
    export.force_flush();

    assert_eq!(export.event_count("ui.widget.focused"), 6);
    assert_eq!(export.event_count("ui.widget.unfocused"), 6);
    let spans = export.finished_spans();
    assert!(
        spans
            .iter()
            .all(|span| span.name != jackin_telemetry::schema::spans::UI_ACTION),
        "focus lifecycle must not create pane-action spans: {spans:?}"
    );
    for widget in ["capsule.pane", "capsule.tab", "capsule.command_palette"] {
        assert!(export.contains_log_text(widget));
    }
    assert!(!export.contains_log_text("test-role"));
    assert!(!export.contains_log_text("/workspace"));
}

#[test]
fn conformance_wire_capsule_mouse_dispatch_counts_once_without_coordinates() -> Result<()> {
    const CHILD: &str = "JACKIN_CAPSULE_MOUSE_WIRE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "daemon::tests::case_14::conformance_wire_capsule_mouse_dispatch_counts_once_without_coordinates",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()?;
        anyhow::ensure!(status.success(), "isolated Capsule mouse test failed");
        return Ok(());
    }

    let _telemetry_guard = crate::support::telemetry_test_guard();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )?;
    let mut mux = test_mux(40, 80);
    mux.handle_input(InputEvent::MousePress {
        col: 73,
        row: 37,
        button: 0,
    });
    mux.handle_input(InputEvent::MouseRelease {
        col: 73,
        row: 37,
        button: 0,
    });
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    record_agent_status_tick(
        &session,
        crate::session::StatusTick {
            transition: Some(crate::session::StatusTransition {
                previous: crate::protocol::AgentState::Idle,
                effective: crate::protocol::AgentState::Working,
                winner: crate::agent_status::evidence::EvidenceWinner::Unknown,
            }),
            stuck: false,
            flap: true,
        },
    );
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|reason| anyhow::anyhow!("validation event rejected: {reason:?}"))?;
    jackin_diagnostics::flush_wire_test_export()?;
    drop(runtime_guard);
    anyhow::ensure!(
        runtime.block_on(testbed.wait_for_all_signals(Duration::from_secs(2))),
        "Capsule mouse metric did not reach all three-signal receiver"
    );

    let mouse_count = testbed
        .metrics()
        .into_iter()
        .flat_map(|request| request.resource_metrics)
        .flat_map(|resource| resource.scope_metrics)
        .flat_map(|scope| scope.metrics)
        .filter(|metric| metric.name == "terminal.input.mouse")
        .filter_map(|metric| metric.data)
        .filter_map(|data| match data {
            opentelemetry_proto::tonic::metrics::v1::metric::Data::Sum(sum) => Some(sum),
            _ => None,
        })
        .flat_map(|sum| sum.data_points)
        .filter_map(|point| point.value)
        .map(|value| match value {
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsInt(value) => {
                value.to_string().parse::<f64>().unwrap_or_default()
            }
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsDouble(value) => {
                value
            }
        })
        .sum::<f64>();
    anyhow::ensure!(
        (mouse_count - 2.0).abs() < f64::EPSILON,
        "expected two mouse events, got {mouse_count}"
    );
    let flap_count = testbed
        .metrics()
        .into_iter()
        .flat_map(|request| request.resource_metrics)
        .flat_map(|resource| resource.scope_metrics)
        .flat_map(|scope| scope.metrics)
        .filter(|metric| metric.name == "agent.state.flaps")
        .filter_map(|metric| metric.data)
        .filter_map(|data| match data {
            opentelemetry_proto::tonic::metrics::v1::metric::Data::Sum(sum) => Some(sum),
            _ => None,
        })
        .flat_map(|sum| sum.data_points)
        .filter_map(|point| point.value)
        .map(|value| match value {
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsInt(value) => {
                value.to_string().parse::<f64>().unwrap_or_default()
            }
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsDouble(value) => {
                value
            }
        })
        .sum::<f64>();
    anyhow::ensure!(
        (flap_count - 1.0).abs() < f64::EPSILON,
        "expected one flap episode, got {flap_count}"
    );
    for key in testbed.metric_dimension_keys() {
        anyhow::ensure!(
            !matches!(
                key.as_str(),
                "row" | "column" | "mouse.row" | "mouse.column" | "ui.pointer.x" | "ui.pointer.y"
            ),
            "raw pointer coordinate key escaped to OTLP: {key}"
        );
    }
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn apply_action_status_bar_click_switches_tab() {
    let mut mux = single_pane_tab_mux();
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("Shell", 2, "test"));
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let col = (1..mux.render.term_cols)
        .find(|col| mux.status.status_bar.tab_at_col(*col) == Some(1))
        .expect("second tab should have a clickable column")
        - 1;

    mux.apply_action(Action::StatusBarClick { col });

    assert_eq!(mux.session_supervisor.active_tab, 1);
}

#[test]
fn apply_action_status_bar_double_click_opens_rename() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let col = (1..mux.render.term_cols)
        .find(|col| mux.status.status_bar.tab_at_col(*col) == Some(0))
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
    mux.status.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status.status_bar.instance_id_label = "test".to_owned();
    mux.status.status_bar.role = "the-architect".to_owned();
    mux.pr_watch.pull_request_context_branch = Some(branch("feature/context"));
    let hit = branch_context_bar_layout(
        mux.render.term_rows,
        mux.render.term_cols,
        mux.pr_watch.pull_request_context_branch.as_deref(),
        None,
        mux.pr_watch.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        None,
        mux.status.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.container)
    .expect("container should fit");

    mux.apply_action(Action::BranchContextBarClick {
        row: mux.render.term_rows - 1,
        col: hit.start - 1,
    });

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            container_name,
            copied_row: None,
            ..
        }) if container_name == "jk-test-container"
    ));
}

#[test]
fn apply_action_palette_new_tab_pushes_agent_picker() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::Palette(PaletteCommand::NewTab))
        .expect("palette new tab should redraw agent picker");

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
    assert!(
        !frame_contains_screen_erase(&frame),
        "palette new tab agent picker must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_open_agent_picker_pushes_picker() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::OpenAgentPicker(PickerIntent::NewTab))
        .expect("open agent picker should redraw");

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
    assert!(
        !frame_contains_screen_erase(&frame),
        "open agent picker must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_detach_sets_detach_request() {
    let mut mux = single_pane_tab_mux();

    mux.apply_action(Action::Detach);

    assert!(mux.client_registry.detach_requested);
}

#[test]
fn prefix_new_tab_routes_through_action_picker() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::NewTab)
        .expect("prefix new-tab should redraw agent picker");

    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));
    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix new-tab must not clear the full terminal screen"
    );
}
