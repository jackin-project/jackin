// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Periodic state tick: metrics, token probes, and agent-state refresh.

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
    mux.record_resource_metrics().await;
    mux.maybe_spawn_pull_request_context_lookup(Instant::now());
    // Reap idle clipboard-image transfers and surface a notice. Must NOT
    // short-circuit the tick: agent-state advancement below is the 1 Hz floor —
    // every session re-evaluates each tick — and a clipboard reap is an
    // orthogonal concern that must not freeze it. The `invalidate` guarantees
    // the notice repaints even if no agent state changed this tick (otherwise
    // the no-change return below would leave the frame clean and the notice
    // never painted).
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
    // Evidence arbitration is the ONLY path that authors agent state. Each
    // session assembles an EvidenceSnapshot (authority, process, OSC, screen)
    // in `advance_status`, arbitrates to a raw state + confidence, and
    // publishes through SessionStatus (which derives the public `effective`
    // state, incl. done-from-seen).
    let now = Instant::now();
    // Token-spend monitor: keep it synced to the live agent sessions and poll
    // any due providers. `poll_due_sessions` self-throttles to the 30s/60s
    // cadence, so calling it each state tick is cheap.
    let token_sessions: Vec<(u64, Agent)> = mux
        .session_supervisor
        .sessions
        .iter()
        .filter_map(|(id, s)| Some((id, Agent::from_slug(s.agent.as_deref()?)?)))
        .collect();
    mux.usage.token_monitor.reconcile_sessions(&token_sessions);
    // Returned changed-id list is unused for now (no live event stream yet);
    // the poll updates the cached per-session totals that
    // `ClientMsg::TokenUsage` reads.
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
    // Snapshot visible agent state, refresh, snapshot again. The ticker's only
    // time-based effect is Working→Idle transitions; tab labels derive from
    // state and the status bar has no per-second counter, so when state is
    // unchanged the chrome is identical. A full redraw (clear + repaint) every
    // tick reads as a constant flicker, so skip it unless state actually
    // changed.
    let states_before: Vec<_> = mux
        .session_supervisor
        .sessions
        .iter()
        .map(|(id, s)| (id, s.state))
        .collect();
    for (_, session) in mux.session_supervisor.sessions.iter_mut() {
        // Session::advance_status is the sole state-authoring path; the daemon
        // only reacts to the resulting transition.
        let tick = session.advance_status(rule_registry, now);
        record_agent_status_tick(session, tick);
    }
    // Seen/ack: the focused pane is being reviewed, so it must never linger on
    // `done`. Acknowledge it each tick (idempotent — only done→idle changes
    // anything), which records the seen revision.
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
    // A modal owns the whole screen behind an opaque backdrop; repainting the
    // status/branch chrome here would draw it back over the fill, so skip the
    // chrome frame while a dialog is open.
    if mux.dialog_open() {
        return;
    }
    if states_before == states_after {
        return;
    }
    mux.refresh_tab_labels();
    mux.invalidate(status_change_redraw_reason());
}
