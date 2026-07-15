// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Control reply + status capture helpers extracted from the daemon
//! coordinator: `write_status_capture`, `control_reply_for_request`, and the
//! related reply builders.

use super::{
    ClientFrame, ClientMsg, ClipboardImageInsertMode, Instant, Multiplexer, PathBuf, Result,
    ServerMsg, Session, TokenTotals, explicit_redraw_reason, log_clipboard_image_rejection,
    prefix_mode_for_mux_mode,
};
use jackin_core::container_paths;

/// Write a capture fixture for `session`: its live visible grid (`visible.txt`)
/// and the current evidence report (`evidence.json`), under
/// `/jackin/state/agent-status/captures/<id>-<seq>/`. Turns a live
/// mis-detection into a regression fixture in one command.
pub fn write_status_capture(session_id: u64, session: &Session) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CAPTURE_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = CAPTURE_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(container_paths::AGENT_STATUS_CAPTURES_DIR)
        .join(format!("{session_id}-{seq}"));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("visible.txt"),
        session.visible_screen_rows().join("\n"),
    )?;
    let report = session.status.report(session.agent.clone());
    std::fs::write(
        dir.join("evidence.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    crate::clog!("status.capture: wrote {}", dir.display());
    Ok(())
}

pub fn control_reply_for_request(mux: &mut Multiplexer, msg: ClientMsg) -> ServerMsg {
    match msg {
        ClientMsg::Status => ServerMsg::SessionList {
            sessions: mux.session_infos(),
        },
        ClientMsg::Snapshot => ServerMsg::Snapshot {
            tabs: mux.tab_snapshots(),
            active_tab: u32::try_from(mux.session_supervisor.active_tab).unwrap_or(0),
        },
        ClientMsg::Agents => ServerMsg::AgentRegistry {
            records: mux.agent_registry_snapshot(),
        },
        // Forwarded in-container reporter event: apply it to the addressed
        // session's authority and Ack immediately (never block the agent hook).
        ClientMsg::ReportRuntimeEvent {
            session_id,
            source_id,
            runtime,
            event,
            payload,
        } => {
            let session_known = mux.session_supervisor.sessions.contains_key(session_id);
            if let Some(session) = mux.session_supervisor.sessions.get_mut(session_id) {
                session.apply_runtime_event(
                    &source_id,
                    &runtime,
                    &event,
                    payload.as_deref(),
                    Instant::now(),
                );
            } else {
                crate::cdebug!("agent-status: runtime event for unknown session {session_id}");
            }
            // INV-D12: agent hooks never block — ACK even when session is unknown
            // (port documents the always-ack policy at this call site).
            use super::ports::{ControlPort, PORTS};
            debug_assert!(
                PORTS.should_ack_unknown_session_runtime_event(session_id, session_known),
                "control port must ACK runtime events (session_known={session_known})"
            );
            ServerMsg::Ack
        }
        // Contributor diagnostic: snapshot the live grid + evidence to a fixture.
        ClientMsg::StatusCapture { session_id } => {
            if let Some(session) = mux.session_supervisor.sessions.get(session_id) {
                if let Err(e) = write_status_capture(session_id, session) {
                    crate::clog!("status.capture: session {session_id} failed: {e:#}");
                }
            } else {
                crate::clog!("status.capture: no live session {session_id} to capture");
            }
            ServerMsg::Ack
        }
        ClientMsg::UsageFocused => ServerMsg::UsageFocused {
            usage: Box::new(mux.focused_usage_snapshot()),
        },
        ClientMsg::UsageRefreshFocused => {
            mux.request_usage_refresh_for_provider(None);
            ServerMsg::UsageFocused {
                usage: Box::new(mux.focused_usage_snapshot()),
            }
        }
        ClientMsg::UsageAccountList => ServerMsg::UsageAccounts {
            accounts: mux.usage.usage_cache.account_snapshot_views(),
        },
        ClientMsg::ExecCommand { .. } => {
            // Defensive only: `ExecCommand` is intercepted by the control loop
            // (it opens `Dialog::ExecPicker` and replies after the operator
            // confirms/cancels), so it never reaches this synchronous path.
            // Fail closed if it ever does.
            ServerMsg::ExecDenied {
                reason: "jackin-exec must be dispatched through the credential picker".to_owned(),
            }
        }
        ClientMsg::TokenUsage { session_id } => ServerMsg::TokenUsage {
            summary: mux
                .usage
                .token_monitor
                .totals(session_id)
                .map(TokenTotals::to_summary),
        },
        ClientMsg::Unknown => {
            crate::clog!("control: ignoring unknown ClientMsg variant from peer");
            ServerMsg::Unknown
        }
    }
}

pub fn handle_client_frame(mux: &mut Multiplexer, frame: ClientFrame) {
    match frame {
        ClientFrame::Hello { .. } => {
            // The initial Hello is consumed by the accept handler; any
            // further Hello on the same connection is ignored.
        }
        ClientFrame::Resize { rows, cols } => {
            crate::cdebug!("resize-event: source=client-frame rows={rows} cols={cols}");
            // resize() records the Resize invalidation (and its wipe); the
            // render loop composes the resized frame on the next pass.
            mux.resize(rows, cols);
        }
        ClientFrame::Input(bytes) => {
            // Debug-only input-path telemetry: every chunk from the
            // client and every parser event lands in the log when
            // `JACKIN_DEBUG=1`. Production runs stay quiet — the macro
            // skips the format + write entirely. The pair is the
            // canonical trace for "key X did nothing" triage: chunk
            // line proves the byte reached the daemon, event line
            // proves the parser classified it.
            crate::ctrace_payload!(
                "rx ClientFrame::Input len={} bytes={:02x?}",
                bytes.len(),
                bytes
            );
            let events = mux.control.input_parser.parse(&bytes);
            for event in events {
                let mode = mux.mux_mode();
                crate::ctrace_payload!("  -> InputEvent::{:?} mode={mode:?}", event,);
                mux.handle_input(event);
            }
            let prefix_mode = prefix_mode_for_mux_mode(mux.mux_mode());
            if mux.status.status_bar.prefix_mode != prefix_mode {
                mux.status.status_bar.set_prefix_mode(prefix_mode);
                mux.invalidate(explicit_redraw_reason());
            }
        }
        ClientFrame::Command(_payload) => {
            // Reserved for future structured commands from the host CLI.
        }
        ClientFrame::ClipboardImage(image) => mux.stage_clipboard_image_response(image),
        ClientFrame::ClipboardImageStart(start) => {
            let size = start.size;
            if let Err(err) = mux.clipboard.clipboard_image_transfers.start(start) {
                log_clipboard_image_rejection("transfer-start", &err);
                mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
                mux.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
            } else if mux.clipboard.clipboard_image_insert_mode
                == ClipboardImageInsertMode::StageOnly
            {
                mux.set_clipboard_image_notice(format!("Image staging: receiving {size} bytes"));
            } else {
                mux.set_clipboard_image_notice(format!("Image paste: receiving {size} bytes"));
            }
        }
        ClientFrame::ClipboardImageChunk(chunk) => {
            if let Err(err) = mux.clipboard.clipboard_image_transfers.chunk(chunk) {
                log_clipboard_image_rejection("transfer-chunk", &err);
                mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
                mux.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
            }
        }
        ClientFrame::ClipboardImageEnd(end) => {
            match mux.clipboard.clipboard_image_transfers.end(end) {
                Ok(image) => mux.stage_clipboard_image_response(image),
                Err(err) => {
                    log_clipboard_image_rejection("transfer-end", &err);
                    mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
                    mux.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
                }
            }
        }
        ClientFrame::ClipboardImageError(error) => {
            let reason = error.reason_code();
            crate::clog!("clipboard-image: host request failed reason={reason}");
            crate::cdebug!("clipboard-image: host request failed detail={error}");
            mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
            mux.set_clipboard_image_notice(format!("Image paste rejected: {error}"));
        }
        ClientFrame::HostNotice(message) => {
            crate::clog!("host-affordance: {message}");
            mux.set_clipboard_image_notice(message);
        }
        ClientFrame::Detach => {
            mux.client_registry.detach_requested = true;
        }
        ClientFrame::FocusIn => {
            // Forward only when no dialog is intercepting input AND
            // the focused session actually asked for focus reports
            // (`?1004h`). Without the gate, normal-screen shells
            // surface `[I` as literal text at the prompt.
            if !mux.dialog_captures_input()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.session_supervisor.sessions.get(focused)
                && s.focus_events_enabled()
            {
                s.send_input(b"\x1b[I");
            }
        }
        ClientFrame::FocusOut => {
            if !mux.dialog_captures_input()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.session_supervisor.sessions.get(focused)
                && s.focus_events_enabled()
            {
                s.send_input(b"\x1b[O");
            }
        }
    }
}

/// Coalesce a run of consecutive `Resize` frames into the latest size and
/// return the ordered frames the daemon must process, plus how many resizes
/// were coalesced away.
///
/// A non-`Resize` frame pulled from the channel while draining is preserved and
/// returned after the coalesced resize (previously it was silently dropped
/// because `try_recv()` removes a frame before the `while let` pattern rejects
/// it). Order is preserved because the stray frame may depend on the new
/// geometry.
pub(crate) fn coalesce_client_frames(
    first: ClientFrame,
    mut next: impl FnMut() -> Option<ClientFrame>,
) -> (Vec<ClientFrame>, u32) {
    if !matches!(first, ClientFrame::Resize { .. }) {
        return (vec![first], 0);
    }
    let mut latest = first;
    let mut coalesced: u32 = 0;
    loop {
        match next() {
            Some(ClientFrame::Resize { rows, cols }) => {
                latest = ClientFrame::Resize { rows, cols };
                coalesced = coalesced.saturating_add(1);
            }
            Some(other) => return (vec![latest, other], coalesced),
            None => return (vec![latest], coalesced),
        }
    }
}

#[cfg(test)]
mod tests;
