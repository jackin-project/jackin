// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Session-event application: PTY output, exits, and context results.

use std::time::Instant;

use crate::session::SessionEvent;

use crate::tui::update::{
    FullRedrawReason, session_exit_redraw_reason, status_change_redraw_reason,
};

use super::{Multiplexer, handle_last_session_exit, session_observation};
use jackin_protocol::control::SessionEventKind;

/// Apply one session event (PTY output, exit, or context lookup result).
/// Returns `true` when the daemon should exit (last live session gone).
pub(crate) async fn handle_session_event(mux: &mut Multiplexer, event: SessionEvent) -> bool {
    match event {
        SessionEvent::Output { session_id, data } => {
            let focused_id = mux.active_focused_id();
            let is_focused = Some(session_id) == focused_id;
            // Collect any focused-pane output into local
            // vecs so the `&mut Session` borrow ends before
            // `mux.send_output` (which takes `&mut Multiplexer`).
            let mut to_emit: Vec<Vec<u8>> = Vec::new();
            let mut reassert_outer_terminal_title = false;
            if let Some(session) = mux.session_supervisor.sessions.get_mut(session_id) {
                session.feed_pty(&data);
                // Always drain the OSC + unhandled-CSI
                // passthrough buffer so a backgrounded
                // agent emitting OSC 7 / OSC 9 / OSC 8 on
                // every prompt does not grow `pending`
                // unboundedly until it becomes focused.
                // Forward the drained bytes ONLY when this
                // session is the focused pane —
                // backgrounded panes' notifications,
                // clipboard writes, and titles must not
                // reach the operator's outer terminal.
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
            // Bump the generation; the render loop coalesces
            // bursts of PTY output into one frame per pass.
            // Dialog-open still invalidates — the next frame
            // paints the dialog overlay against the latest pane
            // state, so dismiss doesn't jump.
            mux.invalidate(FullRedrawReason::PtyOutput);
        }
        SessionEvent::Exited {
            session_id,
            mut reason,
        } => {
            // Only a non-clean exit carries a `reason`; skip the
            // pane snapshot entirely on clean teardown so the grid
            // render never runs on the common exit path. When the
            // pane has no tail to attach (PTY never rendered, or the
            // session was already removed), keep the base reason —
            // dropping it would misroute a real failure into the
            // clean-shutdown branch and swallow it.
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
            // Last record for this session on the event stream.
            // Emitted before removal so the observation still
            // carries the session's real agent, state and
            // activity rather than a placeholder.
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
            // Remove the pane / tab immediately rather than
            // leaving a stale `○ Done` placeholder behind.
            // Matches the operator's mental model: "agent
            // exited → its tab is gone."
            mux.remove_exited_session(session_id);
            mux.invalidate(session_exit_redraw_reason());
            // When the last live session exits — whether
            // the operator typed `/exit` in the agent or
            // the agent crashed — there is nothing left to
            // attach to. Tear down the container so the
            // host cleanup path fires.
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
