// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use crate::agent_status::rules::RulePackRegistry;

use crate::clipboard::CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT;

use crate::tui::update::{
    dialog_change_redraw_reason, selection_change_redraw_reason, status_change_redraw_reason,
};

use jackin_core::Agent;

use super::{
    ClipboardImageInsertMode, Multiplexer, publish_status_events, record_agent_status_tick,
    record_skipped_provider_probe, send_attach_control_response,
};

pub(crate) async fn handle_state_tick(
    mux: &mut Multiplexer,
    rule_registry: Option<&RulePackRegistry>,
) {
    mux.poll_file_export().await;
    mux.record_resource_metrics().await;
    mux.maybe_spawn_pull_request_context_lookup(Instant::now());
    let stale_image_transfer_ids = mux
        .clipboard
        .clipboard_image_transfers
        .abort_idle_ids_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    let stale_image_transfers = stale_image_transfer_ids.len();
    for transfer_id in stale_image_transfer_ids {
        if let Some(pending) = mux.clipboard.attach_control_operations.remove(&transfer_id) {
            send_attach_control_response(
                mux,
                pending.request_id,
                jackin_protocol::attach::AttachControlResult::Rejected,
                pending.operation,
            );
        }
    }
    if stale_image_transfers > 0 {
        mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
        mux.set_clipboard_image_notice(format!(
            "Image paste interrupted: cleaned up {stale_image_transfers} idle transfer{}",
            if stale_image_transfers == 1 { "" } else { "s" }
        ));
        mux.invalidate(status_change_redraw_reason());
    }
    let now = Instant::now();
    let token_sessions: Vec<(u64, Agent)> = mux
        .session_supervisor
        .sessions
        .iter()
        .filter_map(|(id, s)| Some((id, Agent::from_slug(s.agent.as_deref()?)?)))
        .collect();
    mux.usage.token_monitor.reconcile_sessions(&token_sessions);
    if mux.usage.token_monitor.due_session_count() == 0 {
        record_skipped_provider_probe();
    } else {
        let cycle = jackin_telemetry::autonomous_cycle_operation(
            jackin_telemetry::schema::enums::BackgroundCycleName::ProviderProbe,
        )
        .ok();
        let report = mux.usage.token_monitor.poll_due_sessions().await;
        if let Some(cycle) = cycle {
            if report.degraded == 0 {
                cycle.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
            } else {
                cycle.complete(
                    jackin_telemetry::schema::enums::OutcomeValue::Success,
                    Some(jackin_telemetry::schema::enums::ErrorType::RecoveredDegradation),
                );
            }
        }
    }
    let states_before: Vec<_> = mux
        .session_supervisor
        .sessions
        .iter()
        .map(|(id, s)| (id, s.state))
        .collect();
    for (_, session) in mux.session_supervisor.sessions.iter_mut() {
        let tick = session.advance_status(rule_registry, now);
        record_agent_status_tick(session, tick);
    }
    if let Some(focused) = mux.active_focused_id()
        && let Some(session) = mux.session_supervisor.sessions.get_mut(focused)
        && let Some(effective) = session.status.acknowledge()
    {
        session.state = effective;
    }
    let states_after: Vec<_> = mux
        .session_supervisor
        .sessions
        .iter()
        .map(|(id, s)| (id, s.state))
        .collect();
    publish_status_events(mux, &states_before, &states_after, now);
    if mux.expire_dialog_copy_feedback(Instant::now()) {
        mux.invalidate(dialog_change_redraw_reason());
        return;
    }
    if mux.expire_selection_copy_feedback(Instant::now()) {
        mux.invalidate(selection_change_redraw_reason());
        return;
    }
    if mux.expire_clipboard_image_notice(Instant::now()) {
        mux.invalidate(status_change_redraw_reason());
        return;
    }
    if mux.refresh_open_usage_dialog_from_cache() {
        mux.invalidate(dialog_change_redraw_reason());
        return;
    }
    if mux.dialog_open() {
        return;
    }
    if states_before == states_after {
        return;
    }
    mux.refresh_tab_labels();
    mux.invalidate(status_change_redraw_reason());
}
