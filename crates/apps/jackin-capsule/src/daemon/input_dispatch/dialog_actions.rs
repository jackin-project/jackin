// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Dialog-action dispatch, usage-account helpers, and exec resolution.

use std::sync::Arc;

use crate::tui::update::DIALOG_COPY_FEEDBACK_DURATION;

use crate::tui::view::encode_osc52_clipboard_write;
use jackin_protocol::attach::ServerFrame;

use super::super::{
    Action, ConfirmedActionRoute, Dialog, DialogAction, FullRedrawReason, Instant, Multiplexer,
    PickerIntent, confirmed_action_route, dialog_action_frame_plan,
};

impl Multiplexer {
    pub(crate) fn open_host_url_from_dialog(&mut self, url: String, opening_allowed: bool) {
        if !opening_allowed {
            self.set_clipboard_image_notice(
                "Host link opening disabled by JACKIN_OPEN_LINKS".to_owned(),
            );
            return;
        }
        if !crate::tui::url_text::is_host_open_url(&url) {
            self.set_clipboard_image_notice(
                "Host link rejected: unsupported URL scheme".to_owned(),
            );
            return;
        }
        self.send_protocol_frame(ServerFrame::HostOpenUrl(url));
    }

    /// Single dispatch point for a `DialogAction`. Both the
    /// mouse-click and key-event paths call `Dialog::handle_*`
    /// and route the result here, so adding a new variant means
    /// updating one match arm instead of two.
    pub(crate) fn apply_dialog_action(&mut self, action: DialogAction) {
        let frame_plan = dialog_action_frame_plan(&action);
        match action {
            DialogAction::Dismiss => {
                // Back-navigation: pop one dialog so a sub-dialog
                // reveals its parent rather than closing the whole
                // flow. Operator at the top of stack (Menu) pops to
                // an empty stack — same effective "close" the
                // pre-stack code achieved with `self.dialog = None`.
                self.dialog_pop_one();
            }
            DialogAction::Redraw | DialogAction::Consume => {}
            DialogAction::ExecConfirm {
                command,
                args,
                selected,
            } => {
                // Operator approved. Close the picker, then resolve the chosen
                // credentials through the host socket and run the command off
                // the event loop so the daemon keeps rendering; the spawned task
                // owns the deferred control reply and answers when the command
                // finishes (or fails closed).
                self.dialog_pop_one();
                if let Some(reply_tx) = self.control.pending_exec_reply.take() {
                    reply_tx.spawn(async move { run_exec_selected(command, args, selected).await });
                }
            }
            DialogAction::ExecCancel => {
                self.dialog_pop_one();
                if let Some(reply_tx) = self.control.pending_exec_reply.take() {
                    reply_tx.send(jackin_protocol::control::ServerMsg::ExecDenied {
                        reason: "operator cancelled credential selection".to_owned(),
                    });
                }
            }
            DialogAction::ExitDirty(row) => {
                use crate::tui::components::dialog::ExitDirtyRow;
                match row {
                    // Open the verbatim New-tab agent picker over the exit modal.
                    // Picking an agent spawns a session and clears the dialog
                    // stack (SpawnAgent → dialog_clear), dismissing the modal.
                    ExitDirtyRow::StartNewAgent => {
                        self.apply_action(Action::OpenAgentPicker(PickerIntent::NewTab));
                        return;
                    }
                    // Push the read-only changed-files list stored in the
                    // ExitDirty variant; Arc::clone is O(1). Esc walks back.
                    ExitDirtyRow::Inspect => {
                        let rows = match self.dialog_top() {
                            Some(Dialog::ExitDirty { inspect_rows, .. }) => {
                                Arc::clone(inspect_rows)
                            }
                            _ => return,
                        };
                        self.dialog_push(Dialog::new_exit_inspect(rows));
                        self.invalidate(FullRedrawReason::DialogChange);
                        return;
                    }
                    // Record the operator's choice; the event loop writes the
                    // exit-action file and drains on the next iteration.
                    ExitDirtyRow::Keep => {
                        self.control.exit_request = Some(jackin_protocol::ExitAction::Keep);
                    }
                    ExitDirtyRow::Discard => {
                        self.control.exit_request = Some(jackin_protocol::ExitAction::Discard);
                    }
                }
            }
            DialogAction::Command(cmd) => {
                // `handle_palette_command` decides per-arm whether
                // the command opens a sub-dialog (push) or finishes
                // the flow (clear stack). It records its own
                // invalidation, so return before the generic one.
                self.apply_action(Action::Palette(cmd));
                return;
            }
            DialogAction::SpawnAgent { agent, intent } => {
                self.dialog_clear();
                self.dispatch_spawn_intent(agent, intent);
            }
            DialogAction::RenameTab { tab_idx, label } => {
                self.dialog_clear();
                if let Some(tab) = self.session_supervisor.tabs.get_mut(tab_idx) {
                    tab.set_custom_label(label);
                }
            }
            DialogAction::CopyToClipboard(payload) => {
                // OSC 52 selection write — `\x1b]52;c;<base64>\x07`.
                // `c` is the system clipboard target; modern terminals
                // (Ghostty, iTerm2, Kitty, Alacritty, wezterm, recent
                // gnome-terminal) all honour it. Older / locked-down
                // terminals silently drop the sequence — the copy
                // appears to do nothing but no error fires; the
                // multiplexer can't tell from this side. Emitted to
                // the client via `send_output`; the alt-screen path
                // forwards it byte-for-byte to the operator's outer
                // terminal.
                //
                // Copy-capable dialogs stay on the stack — the
                // operator's "did it actually copy?" question is
                // answered by the copied check affordance the renderer
                // paints now that `copied = true` (flipped by the
                // dialog's handle_key or row-click handler before this
                // action returned).
                // The badge expires from the daemon's tick loop.
                self.send_out_of_band(encode_osc52_clipboard_write(&payload));
                self.clipboard.dialog_copy_feedback_deadline =
                    Some(Instant::now() + DIALOG_COPY_FEEDBACK_DURATION);
            }
            DialogAction::OpenHostUrl(url) => {
                self.open_host_url_from_dialog(
                    url,
                    super::super::mouse_input::host_url_opening_allowed(),
                );
            }
            DialogAction::RevealHostPath(path) => {
                self.send_protocol_frame(ServerFrame::HostRevealPath(path));
            }
            DialogAction::ExportFile {
                path,
                reveal_after_export,
                open_after_export,
            } => {
                self.dialog_clear();
                self.export_file_to_host(path, reveal_after_export, open_after_export);
            }
            DialogAction::RefreshUsage => {
                self.request_usage_refresh_for_provider(None);
            }
            DialogAction::SwitchUsageProvider {
                provider_label,
                account_id,
            } => {
                let view = self.focused_usage_snapshot_for_account_id(&account_id, &provider_label);
                if let Some(dialog) = self.dialog_top_mut() {
                    *dialog = Dialog::new_usage(view);
                }
                self.request_usage_refresh_for_account_id(&account_id, &provider_label);
            }
            DialogAction::SplitDirection(direction) => {
                // Chain to the agent picker carrying the direction —
                // push it on top of the SplitDirectionPicker so Esc
                // walks the operator one step back instead of
                // closing the whole flow.
                let agents = self.launch_env.available_instances.clone();
                self.dialog_push(Dialog::new_agent_picker(
                    agents,
                    PickerIntent::Split(direction),
                ));
            }
            DialogAction::PickedCloseTarget(kind) => {
                // Push the ConfirmAction dialog on top of the
                // CloseTargetPicker. Esc walks back to the picker,
                // then back to the Menu — operator can change their
                // mind without destroying anything.
                self.dialog_push(Dialog::new_confirm_action(kind));
            }
            DialogAction::ConfirmedAction(kind) => {
                // Terminal action — clear every dialog under us and
                // fire the matching destructive call.
                self.dialog_clear();
                match confirmed_action_route(kind) {
                    ConfirmedActionRoute::ClosePane => self.close_focused_pane(),
                    ConfirmedActionRoute::CloseTab => self.close_focused_tab(),
                    ConfirmedActionRoute::ExitAllSessions => self.exit_all_sessions(),
                }
            }
        }
        self.invalidate(frame_plan.reason());
    }

    /// Focused usage snapshot for a tab switch: exact account id when the
    /// action carries one, label resolution only for empty ids (old
    /// payloads). A non-empty but unknown id is an honest unavailable, never
    /// a label-guessed sibling account.
    fn focused_usage_snapshot_for_account_id(
        &mut self,
        account_id: &str,
        provider_label: &str,
    ) -> jackin_protocol::control::FocusedUsageView {
        if account_id.is_empty() {
            return self.focused_usage_snapshot_for_provider(Some(provider_label));
        }
        if let Some(view) = self
            .usage
            .usage_cache
            .focused_snapshot_for_account_id(account_id)
        {
            return view;
        }
        jackin_protocol::control::FocusedUsageView::unavailable(
            "usage unavailable: account not cached",
            chrono::Utc::now().timestamp(),
        )
    }

    /// Queue a refresh for a tab switch: exact account id when the action
    /// carries one, label resolution only for empty ids (old payloads). A
    /// non-empty but unrefreshable id queues nothing rather than refreshing
    /// a label-guessed sibling account.
    fn request_usage_refresh_for_account_id(&mut self, account_id: &str, provider_label: &str) {
        if account_id.is_empty() {
            self.request_usage_refresh_for_provider(Some(provider_label));
            return;
        }
        let Some(target) = self.usage_refresh_target_for_account_id(account_id) else {
            return;
        };
        // Queue through the shared path so the open dialog gets the same
        // refreshing treatment, then pin the exact id-resolved target: the
        // display label must never route a refresh.
        self.request_usage_refresh_for_provider(None);
        self.usage.pending_usage_refresh = Some(target);
    }

    /// Refresh target for the session holding the broker account behind a tab
    /// id. The session's own provider label and capability keep the target
    /// authoritative; the action's display label never routes a refresh.
    fn usage_refresh_target_for_account_id(
        &self,
        account_id: &str,
    ) -> Option<crate::usage::UsageRefreshTarget> {
        let broker_account_id = self
            .usage
            .usage_cache
            .broker_account_id_for_tab_id(account_id)?;
        let session = self.session_supervisor.sessions.values().find(|session| {
            session
                .usage_capability
                .as_ref()
                .is_some_and(|capability| capability.account_id == broker_account_id)
        })?;
        Some(crate::usage::UsageRefreshTarget {
            agent: session.agent.clone()?,
            provider: session
                .provider
                .as_ref()
                .map(|provider| provider.label.clone()),
            capability: session.usage_capability.clone()?,
        })
    }
}

/// Resolve the operator-selected credentials through the host socket and run the
/// command with them injected as env vars, returning the framed control reply.
///
/// Fails closed (`ExecDenied`) on any resolver or spawn error — the command is
/// never run with a partially-resolved credential set. The container reaches the
/// host resolver at `/jackin/run/host.sock` (bind-mounted by the launch path).
async fn run_exec_selected(
    command: String,
    args: Vec<String>,
    selected: Vec<jackin_protocol::ExecBinding>,
) -> jackin_protocol::control::ServerMsg {
    use jackin_protocol::control::ServerMsg;

    let resolved =
        match crate::exec::resolve_credentials(jackin_protocol::HOST_SOCK_CONTAINER_PATH, selected)
            .await
        {
            Ok(map) => map,
            Err(error) => {
                return ServerMsg::ExecDenied {
                    reason: format!("credential resolution failed: {error}"),
                };
            }
        };
    // Redaction set borrows the resolved values — no second copy of secret
    // material; the strings already live in `resolved` for the env injection.
    let secrets: Vec<&str> = resolved.values().map(String::as_str).collect();
    match crate::exec::execute_command(&command, &args, &resolved, &secrets).await {
        Ok((exit_code, stdout, stderr, redacted_count)) => ServerMsg::ExecResult {
            exit_code,
            stdout,
            stderr,
            redacted_count,
        },
        Err(error) => ServerMsg::ExecDenied {
            reason: format!("command execution failed: {error}"),
        },
    }
}
