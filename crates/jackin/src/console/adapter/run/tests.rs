// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_console::tui::state::update::{ManagerMessage, update_manager};
use jackin_console::tui::state::{EditorState, EditorTab};

const UI_CAUSALITY_CHILD: &str = "JACKIN_UI_CAUSALITY_WIRE_CHILD";

struct TestFrameContext<'a> {
    terminal: &'a mut ratatui::Terminal<ratatui::backend::TestBackend>,
    config: &'a AppConfig,
    cwd: &'a std::path::Path,
    screens: &'a mut jackin_telemetry::ui::ScreenVisitTracker,
    widgets: &'a mut jackin_telemetry::ui::WidgetFocusTracker,
    mouse: &'a mut ConsoleMouseState,
    jank: &'a mut jackin_telemetry::ui::JankMonitor,
}

impl TestFrameContext<'_> {
    fn render_action(&mut self, state: &mut ConsoleState) -> anyhow::Result<()> {
        let action = jackin_telemetry::ui::take_action_parent()
            .ok_or_else(|| anyhow::anyhow!("production reducer omitted action ownership"))?;
        sync_active_screen(state, self.screens, Some(&action));
        sync_widget_focus(state, self.widgets, Some(&action));
        let mut overlay_active = false;
        draw_console_frame(
            self.terminal,
            state,
            DrawConsoleContext {
                config: self.config,
                cwd: self.cwd,
                mouse_state: self.mouse,
                container_info_overlay_active: &mut overlay_active,
                action_parent: Some(&action),
                jank_monitor: self.jank,
            },
        )?;
        drop(action);
        Ok(())
    }
}

#[test]
fn conformance_wire_console_reducer_preserves_ui_causality() -> anyhow::Result<()> {
    if std::env::var_os(UI_CAUSALITY_CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "console::adapter::run::tests::conformance_wire_console_reducer_preserves_ui_causality",
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
    let _update = update_manager(
        manager,
        ManagerMessage::EnterEditor(EditorState::new_edit(
            "private-workspace-label".into(),
            jackin_config::WorkspaceConfig::default(),
        )),
    );
    frame.render_action(&mut state)?;

    let ConsoleStage::Manager(manager) = &mut state.stage;
    let _update = update_manager(manager, ManagerMessage::SelectEditorTab(EditorTab::Mounts));
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
