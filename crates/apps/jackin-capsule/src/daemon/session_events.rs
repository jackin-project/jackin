// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use crate::session::SessionEvent;

use crate::tui::update::{
    FullRedrawReason, session_exit_redraw_reason, status_change_redraw_reason,
};

use super::{Multiplexer, handle_last_session_exit, session_observation};
use jackin_protocol::control::SessionEventKind;

pub(crate) async fn handle_session_event(mux: &mut Multiplexer, event: SessionEvent) -> bool {
    match event {
        SessionEvent::Output { session_id, data } => {
            let focused_id = mux.active_focused_id();
            let is_focused = Some(session_id) == focused_id;
            let mut to_emit: Vec<Vec<u8>> = Vec::new();
            let mut reassert_outer_terminal_title = false;
            if let Some(session) = mux.session_supervisor.sessions.get_mut(session_id) {
                session.feed_pty(&data);
                let drained = session.drain_passthrough();
                if is_focused {
                    reassert_outer_terminal_title = !drained.is_empty();
                    to_emit.extend(drained);
                }
            }
            for bytes in to_emit {
                mux.send_out_of_band(bytes);
            }
            if reassert_outer_terminal_title {
                mux.client_registry.last_outer_terminal_title = None;
            }
            mux.invalidate(FullRedrawReason::PtyOutput);
        }
        SessionEvent::Exited {
            session_id,
            mut reason,
        } => {
            if let Some(base) = reason.take() {
                let tail = mux
                    .session_supervisor
                    .sessions
                    .get(session_id)
                    .and_then(|session| session.diagnostic_tail(12));
                reason = Some(match tail {
                    Some(tail) => format!("{base}\nlast pane output:\n{tail}"),
                    None => base,
                });
            }
            if !mux.control.event_subscribers.is_empty()
                && let Some(session) = mux.session_supervisor.sessions.get(session_id)
            {
                let observation = session_observation(session_id, session);
                mux.control.event_subscribers.publish(
                    Instant::now(),
                    &observation,
                    &SessionEventKind::Exited {
                        reason: reason.clone(),
                    },
                );
            }
            mux.remove_exited_session(session_id);
            mux.invalidate(session_exit_redraw_reason());
            if mux.no_live_sessions() && handle_last_session_exit(&mut *mux, reason).await {
                return true;
            }
        }
        SessionEvent::GitBranchContextRefreshRequested => {
            mux.force_spawn_git_branch_context_lookup(Instant::now());
        }
        SessionEvent::GitBranchContextLoaded {
            request_id,
            context,
        } => {
            if mux.apply_git_branch_context_loaded(request_id, context, Instant::now()) {
                mux.invalidate(status_change_redraw_reason());
            }
        }
        SessionEvent::PullRequestContextLoaded {
            request_id,
            branch,
            head,
            outcome,
        } => {
            if mux.apply_pull_request_context_loaded(
                request_id,
                branch,
                head,
                outcome,
                Instant::now(),
            ) {
                mux.invalidate(status_change_redraw_reason());
            }
        }
    }
    false
}
