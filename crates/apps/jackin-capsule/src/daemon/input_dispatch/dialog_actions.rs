// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

    pub(crate) fn apply_dialog_action(&mut self, action: DialogAction) {
        let frame_plan = dialog_action_frame_plan(&action);
        match action {
            DialogAction::Dismiss => {
                self.dialog_pop_one();
            }
            DialogAction::Redraw | DialogAction::Consume => {}
            DialogAction::ExecConfirm {
                command,
                args,
                selected,
            } => {
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
                    ExitDirtyRow::StartNewAgent => {
                        self.apply_action(Action::OpenAgentPicker(PickerIntent::NewTab));
                        return;
                    }
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
                    ExitDirtyRow::Keep => {
                        self.control.exit_request = Some(jackin_protocol::ExitAction::Keep);
                    }
                    ExitDirtyRow::Discard => {
                        self.control.exit_request = Some(jackin_protocol::ExitAction::Discard);
                    }
                }
            }
            DialogAction::Command(cmd) => {
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
                self.export_file_to_host(
                    path,
                    reveal_after_export,
                    open_after_export,
                    self.active_focused_id(),
                );
            }
            DialogAction::RefreshUsage => {
                self.request_usage_refresh_for_provider(None);
            }
            DialogAction::SwitchUsageProvider { account_id, .. } => {
                let view = self.focused_usage_snapshot_for_account_id(&account_id);
                if let Some(dialog) = self.dialog_top_mut() {
                    *dialog = Dialog::new_usage(view);
                }
                self.request_usage_refresh_for_account_id(&account_id);
            }
            DialogAction::SplitDirection(direction) => {
                let agents = self.launch_env.available_instances.clone();
                self.dialog_push(Dialog::new_agent_picker(
                    agents,
                    PickerIntent::Split(direction),
                ));
            }
            DialogAction::PickedCloseTarget(kind) => {
                self.dialog_push(Dialog::new_confirm_action(kind));
            }
            DialogAction::ConfirmedAction(kind) => {
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

    fn focused_usage_snapshot_for_account_id(
        &mut self,
        account_id: &str,
    ) -> jackin_protocol::control::FocusedUsageView {
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

    fn request_usage_refresh_for_account_id(&mut self, account_id: &str) {
        let Some(target) = self.usage_refresh_target_for_account_id(account_id) else {
            return;
        };
        self.request_usage_refresh_for_provider(None);
        self.usage.pending_usage_refresh = Some(target);
    }

    fn usage_refresh_target_for_account_id(
        &self,
        account_id: &str,
    ) -> Option<crate::usage::UsageRefreshTarget> {
        let capability = self
            .usage
            .usage_cache
            .broker_capability_for_tab_id(account_id)?;
        self.session_supervisor
            .sessions
            .values()
            .find_map(|session| {
                if session.usage_capability.as_ref() != Some(&capability) {
                    return None;
                }
                let agent = session.agent.as_deref()?;
                if self
                    .launch_env
                    .launch_config
                    .usage_capability_for_instance(agent)
                    != Some(&capability)
                {
                    return None;
                }
                let provider = session
                    .provider
                    .as_ref()
                    .map(|provider| provider.label.clone());
                if !jackin_usage_provider_core::capability_matches_surface(
                    agent,
                    provider.as_deref(),
                    &capability,
                ) {
                    return None;
                }
                Some(crate::usage::UsageRefreshTarget {
                    agent: agent.to_owned(),
                    provider,
                    capability: capability.clone(),
                })
            })
    }
}

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
