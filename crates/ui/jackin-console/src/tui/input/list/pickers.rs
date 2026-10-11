// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Inline and account picker handling.

use super::super::InputOutcome;
use super::dispatch_manager;
use crate::tui::components::account_picker::AccountPickerOutcome;
use crossterm::event::KeyEvent;

use crate::tui::prompts::{no_eligible_account_message, sort_account_choices_by_id};

use crate::tui::state::ManagerState;
use crate::tui::state::update::ManagerMessage;
use crate::tui::update::{
    InlineAccountFollowupPlan, InlinePickerDismissal, InlinePickerPlan, InlinePickerShellPlan,
    apply_inline_account_picker_plan, apply_inline_picker_dismissal_plan,
    inline_account_followup_plan, inline_picker_dismissal_plan, inline_picker_plan,
    inline_picker_shell_plan,
};

pub fn handle_inline_role_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(picker) = state.inline_role_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match inline_picker_shell_plan(key, true) {
        InlinePickerShellPlan::ScrollHorizontal(delta) => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
            InputOutcome::Continue
        }
        InlinePickerShellPlan::Exit => InputOutcome::ExitJackin,
        InlinePickerShellPlan::Delegate => match inline_picker_plan(picker.handle_key(key)) {
            InlinePickerPlan::Commit(role) => {
                dispatch_manager(state, ManagerMessage::DismissInlineRolePicker);
                InputOutcome::LaunchWithAgent(role)
            }
            InlinePickerPlan::Dismiss => {
                dispatch_manager(state, ManagerMessage::DismissInlineRolePicker);
                InputOutcome::Continue
            }
            InlinePickerPlan::Continue => InputOutcome::Continue,
        },
    }
}

pub fn handle_inline_agent_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some((_, picker)) = state.inline_agent_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match inline_picker_shell_plan(key, false) {
        InlinePickerShellPlan::ScrollHorizontal(delta) => {
            dispatch_manager(state, ManagerMessage::ScrollListHorizontal(delta));
            InputOutcome::Continue
        }
        InlinePickerShellPlan::Exit => InputOutcome::ExitJackin,
        InlinePickerShellPlan::Delegate => match inline_picker_plan(picker.handle_key(key)) {
            InlinePickerPlan::Commit(agent) => {
                dispatch_manager(state, ManagerMessage::DismissInlineAgentPicker);
                InputOutcome::LaunchWithRuntimeAgent(agent)
            }
            InlinePickerPlan::Dismiss => {
                dispatch_manager(state, ManagerMessage::DismissInlineAgentPicker);
                InputOutcome::Continue
            }
            InlinePickerPlan::Continue => InputOutcome::Continue,
        },
    }
}

/// Handle key events while the new-session agent picker is open in the left
/// sidebar. Commit filters the stored live admission rows to the selected
/// agent: an empty result opens an actionable error popup instead of
/// dispatching an account-less session, a single candidate dispatches its
/// exact instance ID directly, and several open the account picker.
/// Cancel/Esc dismisses.
pub fn handle_new_session_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some((container, picker, providers)) = state.inline_new_session_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match inline_picker_plan(picker.handle_key(key)) {
        InlinePickerPlan::Commit(agent) => {
            let container = container.clone();
            let mut accounts: Vec<_> = providers
                .iter()
                .filter(|account| account.agents.contains(&agent))
                .cloned()
                .collect();
            // Re-sort: the open path stores id order, but any producer
            // (tests, a future daemon-queried list) must render the same
            // deterministic picker.
            sort_account_choices_by_id(&mut accounts);
            if accounts.is_empty() {
                let message = new_session_no_account_message(agent, &container, state);
                dispatch_manager(
                    state,
                    ManagerMessage::OpenListErrorPopup {
                        title: no_eligible_account_error_title().into(),
                        message,
                    },
                );
                return InputOutcome::Continue;
            }
            let plan = inline_account_followup_plan(container, agent, accounts);
            dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
            match plan {
                InlineAccountFollowupPlan::StartSession {
                    context,
                    agent,
                    account,
                } => {
                    let Some(account) = account else {
                        let message = new_session_no_account_message(agent, &context, state);
                        dispatch_manager(
                            state,
                            ManagerMessage::OpenListErrorPopup {
                                title: no_eligible_account_error_title().into(),
                                message,
                            },
                        );
                        return InputOutcome::Continue;
                    };
                    let Some(instance_id) = account.instance_id else {
                        let message = new_session_no_account_message(agent, &context, state);
                        dispatch_manager(
                            state,
                            ManagerMessage::OpenListErrorPopup {
                                title: no_eligible_account_error_title().into(),
                                message,
                            },
                        );
                        return InputOutcome::Continue;
                    };
                    InputOutcome::NewSessionWithAccount {
                        container: context,
                        agent,
                        instance_id,
                    }
                }
                InlineAccountFollowupPlan::OpenAccountPicker(picker) => {
                    apply_inline_account_picker_plan(state, picker);
                    InputOutcome::Continue
                }
            }
        }
        InlinePickerPlan::Dismiss => {
            dispatch_manager(state, ManagerMessage::DismissInlineSessionPicker);
            InputOutcome::Continue
        }
        InlinePickerPlan::Continue => InputOutcome::Continue,
    }
}

pub(crate) fn no_eligible_account_error_title() -> &'static str {
    "No eligible account"
}

/// Actionable zero-eligible-account text for the new-session commit: names
/// the agent and the workspace when the target container still maps to one,
/// otherwise the container itself.
pub(crate) fn new_session_no_account_message(
    agent: jackin_core::Agent,
    container: &str,
    state: &ManagerState<'_>,
) -> String {
    let scope = state
        .instances
        .iter()
        .find(|entry| entry.container_base == container)
        .and_then(|entry| entry.workspace_name.as_deref())
        .map_or_else(
            || format!("container {container:?}"),
            |name| format!("workspace {name:?}"),
        );
    no_eligible_account_message(agent, scope)
}

/// Handle key events while the inline provider picker is open (shown after
/// agent selection when multiple providers are available). Enter commits;
/// Esc cancels; Up/Down navigate.
pub fn handle_inline_account_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(picker) = state.inline_account_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match picker.handle_key(key.into()) {
        AccountPickerOutcome::Commit {
            context,
            agent,
            provider,
        } => {
            dispatch_manager(state, ManagerMessage::DismissInlineAccountPicker);
            let Some(instance_id) = provider.instance_id else {
                let message = new_session_no_account_message(agent, &context, state);
                dispatch_manager(
                    state,
                    ManagerMessage::OpenListErrorPopup {
                        title: no_eligible_account_error_title().into(),
                        message,
                    },
                );
                return InputOutcome::Continue;
            };
            InputOutcome::NewSessionWithAccount {
                container: context,
                agent,
                instance_id,
            }
        }
        AccountPickerOutcome::Cancel => {
            dispatch_manager(state, ManagerMessage::DismissInlineAccountPicker);
            InputOutcome::Continue
        }
        AccountPickerOutcome::Continue => InputOutcome::Continue,
    }
}

pub fn handle_launch_account_picker(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(picker) = state.launch_account_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match picker.handle_key(key.into()) {
        AccountPickerOutcome::Commit {
            context,
            agent,
            provider,
        } => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::LaunchAccount),
            );
            InputOutcome::LaunchWithAccount {
                selector: context,
                agent,
                selection: provider.into_launch_selection(),
            }
        }
        AccountPickerOutcome::Cancel => {
            dispatch_manager(state, ManagerMessage::DismissLaunchAccountPicker);
            InputOutcome::Continue
        }
        AccountPickerOutcome::Continue => InputOutcome::Continue,
    }
}
