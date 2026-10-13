// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn conformance_wire_console_reducer_preserves_ui_causality() -> anyhow::Result<()> {
    if std::env::var_os(UI_CAUSALITY_CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "console::adapter::run::tests::case_01::conformance_wire_console_reducer_preserves_ui_causality",
                "--nocapture",
            ])
            .env(UI_CAUSALITY_CHILD, "1")
            .env("JACKIN_TELEMETRY_LEVEL", "debug")
            .status()?;
        anyhow::ensure!(status.success(), "isolated UI causality test failed");
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = {
        let _entered = runtime.enter();
        jackin_otlp_testbed::Testbed::start()?
    };
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )?;

    let temp = tempfile::tempdir()?;
    let config = AppConfig::default();
    let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())?;
    let mut screens = jackin_telemetry::ui::ScreenVisitTracker::new();
    let mut widgets = jackin_telemetry::ui::WidgetFocusTracker::default();
    let mut mouse = ConsoleMouseState::new();
    let mut jank = jackin_telemetry::ui::JankMonitor::default();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30))?;
    sync_active_screen(&state, &mut screens, None);
    let mut frame = TestFrameContext {
        terminal: &mut terminal,
        config: &config,
        cwd: temp.path(),
        screens: &mut screens,
        widgets: &mut widgets,
        mouse: &mut mouse,
        jank: &mut jank,
    };

    let ConsoleStage::Manager(manager) = &mut state.stage;
    update_manager(
        manager,
        ManagerMessage::EnterEditor(EditorState::new_edit(
            "private-workspace-label".into(),
            jackin_config::WorkspaceConfig::default(),
        )),
    );
    frame.render_action(&mut state)?;

    let ConsoleStage::Manager(manager) = &mut state.stage;
    update_manager(manager, ManagerMessage::SelectEditorTab(EditorTab::Mounts));
    frame.render_action(&mut state)?;
    frame
        .widgets
        .unfocus()
        .map_err(|error| anyhow::anyhow!("widget exit rejected: {error:?}"))?;
    frame
        .screens
        .exit(jackin_telemetry::schema::enums::TransitionReason::Shutdown)
        .map_err(|error| anyhow::anyhow!("screen exit rejected: {error:?}"))?;

    jackin_diagnostics::flush_wire_test_export()?;
    anyhow::ensure!(
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))),
        "console UI flow did not export all signals"
    );
    let spans = testbed.spans();
    let actions = spans
        .iter()
        .filter(|span| span.name == "ui.action")
        .collect::<Vec<_>>();
    let transitions = spans
        .iter()
        .filter(|span| span.name == "ui.screen.transition")
        .collect::<Vec<_>>();
    let renders = spans
        .iter()
        .filter(|span| span.name == "ui.render")
        .collect::<Vec<_>>();
    anyhow::ensure!(actions.len() == 2, "expected open and tab-switch actions");
    anyhow::ensure!(transitions.len() == 1, "expected one screen transition");
    anyhow::ensure!(renders.len() == 2, "each action must own one render");
    anyhow::ensure!(
        actions
            .iter()
            .any(|span| format!("{:?}", span.attributes).contains("workspace.open")),
        "workspace.open action missing"
    );
    anyhow::ensure!(
        actions
            .iter()
            .any(|span| format!("{:?}", span.attributes).contains("tab.switch")),
        "tab.switch action missing"
    );
    anyhow::ensure!(
        transitions[0].parent_span_id
            == actions
                .iter()
                .find(|span| format!("{:?}", span.attributes).contains("workspace.open"))
                .expect("workspace action")
                .span_id,
        "screen transition must be a child of workspace.open"
    );
    for render in renders {
        anyhow::ensure!(
            actions
                .iter()
                .any(|action| action.span_id == render.parent_span_id),
            "render must be a child of its semantic action"
        );
    }
    let lifecycle_wire = format!("{:?}", testbed.log_records());
    for event in [
        "ui.screen.entered",
        "ui.screen.exited",
        "ui.widget.focused",
        "ui.widget.unfocused",
    ] {
        anyhow::ensure!(
            lifecycle_wire.contains(event),
            "production UI lifecycle omitted {event}: {lifecycle_wire}"
        );
    }
    let metric_wire = format!("{:?}", testbed.metrics());
    for metric in ["ui.screen.dwell", "ui.focus.duration"] {
        anyhow::ensure!(
            metric_wire.contains(metric),
            "production UI lifecycle omitted {metric}: {metric_wire}"
        );
    }
    anyhow::ensure!(
        testbed
            .prohibited_value_violations(&["private-workspace-label"])
            .is_empty(),
        "display label leaked into UI telemetry"
    );
    anyhow::ensure!(testbed.legacy_namespace_violations().is_empty());
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn startup_usage_retains_complete_publication_before_open() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let config = AppConfig::default();
    let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())?;
    let mut rx = Some(jackin_console::tui::runtime::ready_blocking_subscription(
        Ok(canonical_usage_publication()),
    ));
    assert!(poll_startup_usage(&mut state, &mut rx));
    let ConsoleStage::Manager(manager) = &mut state.stage;
    assert_eq!(
        manager.usage_snapshot.generated_at_epoch,
        Some(1_800_000_123)
    );
    assert_eq!(
        manager.usage_snapshot.projection_issues[0].message,
        "independent broker diagnostic"
    );
    let opened = jackin_console::tui::state::UsageScreenState::open_with_snapshot(
        manager.usage_snapshot.clone(),
    );
    assert_eq!(
        opened
            .canonical_projection
            .as_ref()
            .unwrap()
            .broker_generation,
        7
    );
    assert_eq!(
        opened.projection_issues[0].retry_at_epoch,
        Some(1_800_000_456)
    );
    Ok(())
}

#[test]
fn refresh_effect_retains_complete_publication_in_screen_and_cache() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let config = AppConfig::default();
    let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())?;
    let ConsoleStage::Manager(manager) = &mut state.stage;
    let mut screen = jackin_console::tui::state::UsageScreenState {
        refresh_generation: 9,
        ..jackin_console::tui::state::UsageScreenState::default()
    };
    screen.begin_refresh(jackin_console::tui::runtime::ready_blocking_subscription((
        9,
        Ok(canonical_usage_publication()),
    )));
    manager.usage.screen = Some(screen);
    assert!(execute_usage_refresh_effect(
        manager,
        &JackinPaths::for_tests(temp.path())
    ));
    for snapshot in [
        &manager.usage_snapshot,
        manager.usage.screen.as_ref().unwrap(),
    ] {
        assert_eq!(snapshot.generated_at_epoch, Some(1_800_000_123));
        assert_eq!(
            snapshot.projection_issues[0].message,
            "independent broker diagnostic"
        );
        assert_eq!(
            snapshot
                .canonical_projection
                .as_ref()
                .unwrap()
                .projection_id,
            "independent-console-publication"
        );
    }
    Ok(())
}

#[test]
fn late_startup_completion_cannot_replace_newer_route_publication_or_failure() -> anyhow::Result<()>
{
    let temp = tempfile::tempdir()?;
    let config = AppConfig::default();
    for startup in [
        Ok(canonical_usage_publication()),
        Err("older startup failure".to_owned()),
    ] {
        let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())?;
        let ConsoleStage::Manager(manager) = &mut state.stage;
        let mut projection = canonical_usage_publication().canonical_projection.unwrap();
        projection.generated_at_epoch = 1_800_000_999;
        projection.issues[0].message = "newer route diagnostic".to_owned();
        let latest = jackin_console::tui::state::UsageScreenState::from_projection(&projection);
        manager.usage_snapshot = latest.clone();
        manager.usage.screen = Some(latest);
        let mut rx = Some(jackin_console::tui::runtime::ready_blocking_subscription(
            startup,
        ));
        assert!(poll_startup_usage(&mut state, &mut rx));
        let ConsoleStage::Manager(manager) = &state.stage;
        assert_eq!(
            manager.usage_snapshot.generated_at_epoch,
            Some(1_800_000_999)
        );
        assert_eq!(
            manager.usage_snapshot.projection_issues[0].message,
            "newer route diagnostic"
        );
        assert!(manager.usage_snapshot.notice.is_none());
    }
    let mut state = jackin_console::tui::console::new_console_state(&config, temp.path())?;
    let ConsoleStage::Manager(manager) = &mut state.stage;
    let mut screen = jackin_console::tui::state::UsageScreenState::default();
    screen.apply_refresh_error("newer route failure".to_owned(), std::time::Instant::now());
    manager.usage_snapshot.notice = Some("newer route failure".to_owned());
    manager.usage.screen = Some(screen);
    let mut rx = Some(jackin_console::tui::runtime::ready_blocking_subscription(
        Ok(canonical_usage_publication()),
    ));
    assert!(poll_startup_usage(&mut state, &mut rx));
    let ConsoleStage::Manager(manager) = &state.stage;
    assert_eq!(
        manager.usage_snapshot.notice.as_deref(),
        Some("newer route failure")
    );
    assert!(manager.usage_snapshot.canonical_projection.is_none());
    Ok(())
}
