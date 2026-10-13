// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const UI_CAUSALITY_CHILD: &str = "JACKIN_UI_CAUSALITY_WIRE_CHILD";

pub(super) struct TestFrameContext<'a> {
    pub(super) terminal: &'a mut ratatui::Terminal<ratatui::backend::TestBackend>,
    pub(super) config: &'a AppConfig,
    pub(super) cwd: &'a std::path::Path,
    pub(super) screens: &'a mut jackin_telemetry::ui::ScreenVisitTracker,
    pub(super) widgets: &'a mut jackin_telemetry::ui::WidgetFocusTracker,
    pub(super) mouse: &'a mut ConsoleMouseState,
    pub(super) jank: &'a mut jackin_telemetry::ui::JankMonitor,
}

impl TestFrameContext<'_> {
    pub(super) fn render_action(&mut self, state: &mut ConsoleState) -> anyhow::Result<()> {
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

pub(super) fn canonical_usage_publication() -> jackin_console::tui::state::UsageScreenState {
    use jackin_protocol::usage_broker::{
        UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1, UsageProjectionRefreshStateV1,
        UsageProjectionSchemaV1, UsageProjectionV1,
    };
    jackin_console::tui::state::UsageScreenState::from_projection(&UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "independent-console-publication".to_owned(),
        generated_at_epoch: 1_800_000_123,
        discovery_revision: "independent-discovery".to_owned(),
        broker_instance_id: "independent-broker".to_owned(),
        broker_generation: 7,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: vec![UsageIssueV1 {
            code: "independent_failure".to_owned(),
            scope: UsageIssueScopeV1::Projection,
            recoverability: UsageIssueRecoverabilityV1::Retryable,
            message: "independent broker diagnostic".to_owned(),
            retry_at_epoch: Some(1_800_000_456),
        }],
    })
}
