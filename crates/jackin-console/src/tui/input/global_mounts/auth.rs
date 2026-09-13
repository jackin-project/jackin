//! Settings Auth tab key and modal handlers.

use super::{
    AuthForm, AuthFormFocus, AuthFormKeyPlan, AuthFormTarget, GlobalMountConfirm, KeyCode,
    KeyEvent, ManagerMessage, ManagerStage, ManagerState, SettingsAuthKeyPlan, SettingsAuthOutcome,
    SettingsModal, auth_credential_input_state, auth_form_key_plan_with_source_folder,
    auth_source_picker_state, confirm_modal, dispatch_manager, open_settings_save_preview,
    settings_update,
};
use crate::tui::components::file_browser::page_rows_for_modal;
use crate::tui::update::{
    AuthSourceFolderPickerPlan, InlinePickerPlan, SourcePickerPlan, auth_source_folder_picker_plan,
    inline_picker_plan, source_picker_plan,
};

fn record_missing_auth_return_path() {
    let _recorded = jackin_telemetry::record_error(
        jackin_telemetry::schema::enums::ErrorType::TelemetryInstrumentationFault,
    );
}

pub(super) fn handle_auth_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    if matches!(key.code, KeyCode::Delete | KeyCode::Char('d' | 'D')) {
        if let ManagerStage::Settings(settings) = &mut state.stage {
            settings.auth.delete_selected_account();
        }
        return;
    }
    if matches!(key.code, KeyCode::Char('e' | 'E')) {
        if let ManagerStage::Settings(settings) = &mut state.stage {
            settings.auth.toggle_selected_account_enabled();
        }
        return;
    }
    use crate::tui::screens::settings::model::AccountTextField;
    let text_field = match key.code {
        KeyCode::Char('f' | 'F') => Some(AccountTextField::DefaultAgent),
        KeyCode::Char('r' | 'R') => Some(AccountTextField::Name),
        KeyCode::Char('b' | 'B') => Some(AccountTextField::BaseUrl),
        KeyCode::Char('m' | 'M') => Some(AccountTextField::Model),
        _ => None,
    };
    if let Some(field) = text_field {
        if let ManagerStage::Settings(settings) = &mut state.stage
            && let Some((id, account)) = settings.auth.pending.iter().nth(settings.auth.selected)
        {
            let (label, value) = match (field, &account.credential) {
                (AccountTextField::DefaultAgent, _) if account.enabled => (
                    "Toggle default for agent (claude/codex/amp/kimi/opencode/grok)",
                    jackin_core::Agent::ALL
                        .iter()
                        .copied()
                        .find(|agent| account.supports_agent(*agent))
                        .map_or_else(String::new, |agent| agent.slug().to_owned()),
                ),
                (AccountTextField::Name, _) => ("Account name", account.name.clone()),
                (
                    AccountTextField::BaseUrl,
                    jackin_config::AccountCredential::ApiKey { base_url, .. },
                ) => (
                    "API base URL (empty for default)",
                    base_url.clone().unwrap_or_default(),
                ),
                (
                    AccountTextField::Model,
                    jackin_config::AccountCredential::ApiKey { model, .. },
                ) => (
                    "Model (empty for agent default)",
                    model.clone().unwrap_or_default(),
                ),
                _ => return,
            };
            settings.auth.editing_account = Some(id.clone());
            settings.auth.editing_text = Some(field);
            settings.auth.set_modal(SettingsModal::AuthTextInput {
                state: Box::new(crate::tui::components::TextInputState::new(label, value)),
            });
        }
        return;
    }
    let plan = settings_update::settings_auth_key_plan(key.code, settings.is_dirty(), false, true);
    match plan {
        SettingsAuthKeyPlan::ClearKind => {
            dispatch_manager(state, ManagerMessage::ClearSettingsAuthKind);
        }
        SettingsAuthKeyPlan::MoveSelection { delta } => {
            dispatch_manager(state, ManagerMessage::MoveSettingsAuthSelection { delta });
        }
        SettingsAuthKeyPlan::EnterKind => {
            if let ManagerStage::Settings(settings) = &mut state.stage {
                open_settings_auth_form(&mut settings.auth, &settings.env);
            }
        }
        SettingsAuthKeyPlan::ConfirmDiscard => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            if settings.is_dirty() {
                settings
                    .mounts
                    .modals
                    .open(confirm_modal(GlobalMountConfirm::Discard));
            }
        }
        SettingsAuthKeyPlan::ReturnToList => {
            dispatch_manager(state, ManagerMessage::ReturnToList);
        }
        SettingsAuthKeyPlan::OpenForm => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_auth_form(&mut settings.auth, &settings.env);
        }
        SettingsAuthKeyPlan::Save => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        SettingsAuthKeyPlan::Noop => {}
    }
}

pub(crate) fn open_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &crate::tui::state::SettingsEnvState<'_>,
) {
    let _ = env;
    use jackin_config::AccountCredential;
    auth.editing_text = None;
    let existing = auth
        .pending
        .iter()
        .nth(auth.selected)
        .map(|(id, account)| (id.clone(), account.clone()));
    let (kind, mode, credential, folder) = if auth.selected
        == auth.pending.len() + crate::tui::screens::settings::model::ACCOUNT_KINDS.len()
    {
        auth.editing_account = None;
        let mode = match auth.github.auth_forward {
            jackin_config::GithubAuthMode::Sync => crate::tui::auth::AuthMode::Sync,
            jackin_config::GithubAuthMode::Token => crate::tui::auth::AuthMode::Token,
            jackin_config::GithubAuthMode::Ignore => crate::tui::auth::AuthMode::Ignore,
        };
        (
            crate::tui::auth::AuthKind::Github,
            mode,
            auth.github.env.get("GH_TOKEN").cloned(),
            None,
        )
    } else if let Some((id, account)) = existing {
        auth.editing_account = Some(id);
        let kind = crate::tui::screens::settings::model::account_kind(&account);
        match account.credential {
            AccountCredential::Profile { directory, .. } => (
                kind,
                crate::tui::auth::AuthMode::Sync,
                None,
                Some(directory),
            ),
            AccountCredential::ApiKey { value, .. } => {
                (kind, crate::tui::auth::AuthMode::ApiKey, Some(value), None)
            }
            AccountCredential::OAuthToken { value, .. } => (
                kind,
                crate::tui::auth::AuthMode::OAuthToken,
                Some(value),
                None,
            ),
        }
    } else {
        auth.editing_account = None;
        let Some(kind) = crate::tui::screens::settings::model::ACCOUNT_KINDS
            .get(auth.selected.saturating_sub(auth.pending.len()))
            .copied()
        else {
            return;
        };
        let mode = if matches!(
            kind,
            crate::tui::auth::AuthKind::Zai | crate::tui::auth::AuthKind::Minimax
        ) {
            crate::tui::auth::AuthMode::ApiKey
        } else {
            crate::tui::auth::AuthMode::Sync
        };
        (kind, mode, None, None)
    };
    auth.selected_kind = Some(kind);
    let form = AuthForm::from_existing(kind, mode, credential).with_source_folder(
        folder,
        Some(
            crate::tui::components::editor_rows::AuthSourceFolderDisplay {
                kind: crate::tui::components::editor_rows::AuthSourceFolderKind::Explicit,
                path: "Select a profile folder".to_owned(),
            },
        ),
    );
    let literal_buffer = form.literal_buffer();
    auth.set_modal(SettingsModal::AuthForm {
        target: AuthFormTarget::Workspace { kind },
        state: Box::new(form),
        focus: AuthFormFocus::Mode,
        literal_buffer,
    });
}

/// Source-folder validation callback used by the settings auth modal.
type SourceFolderValidator =
    dyn Fn(Option<crate::tui::auth::AuthKind>, &std::path::Path) -> Result<(), String>;

pub fn handle_settings_auth_modal(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    term_size: ratatui::layout::Rect,
    validate_source_folder: &SourceFolderValidator,
) -> SettingsAuthOutcome {
    let Some(mut modal) = auth.take_modal() else {
        return SettingsAuthOutcome::Continue;
    };
    match &mut modal {
        SettingsModal::AuthForm {
            target,
            state,
            focus,
            literal_buffer: _,
        } => {
            if key.code == KeyCode::Esc {
                return SettingsAuthOutcome::Continue;
            }
            let plan = auth_form_key_plan_with_source_folder(
                *focus,
                key.code,
                state.shows_source_folder(),
                state.shows_credential_block(),
                state.can_save(),
            );
            match plan {
                AuthFormKeyPlan::Stay => {}
                AuthFormKeyPlan::Focus(next) => *focus = next,
                AuthFormKeyPlan::CycleMode => state.cycle_mode(),
                AuthFormKeyPlan::OpenCredentialSource => {
                    let Some(env_var) = state.mode.and_then(|m| state.kind.required_env_var(m))
                    else {
                        auth.set_modal(modal);
                        return SettingsAuthOutcome::Continue;
                    };
                    auth.open_child_modal(
                        modal,
                        SettingsModal::AuthSourcePicker {
                            state: auth_source_picker_state(env_var, op_available),
                        },
                    );
                    return SettingsAuthOutcome::Continue;
                }
                AuthFormKeyPlan::OpenSourceFolderBrowser => {
                    auth.set_modal(modal);
                    return SettingsAuthOutcome::OpenAuthSourceFolderBrowser;
                }
                AuthFormKeyPlan::Save => {
                    persist_settings_auth_form(auth, env, state);
                    return SettingsAuthOutcome::Continue;
                }
                AuthFormKeyPlan::Cancel => return SettingsAuthOutcome::Continue,
                AuthFormKeyPlan::Reset => {
                    clear_settings_auth_kind(auth, env, target);
                    return SettingsAuthOutcome::Continue;
                }
            }
            auth.set_modal(modal);
        }
        SettingsModal::AuthSourcePicker { state } => {
            let outcome = state.handle_key(key);
            match source_picker_plan(outcome) {
                SourcePickerPlan::Plain => {
                    let literal = auth
                        .modals
                        .parents()
                        .last()
                        .and_then(|m| {
                            if let SettingsModal::AuthForm { literal_buffer, .. } = m {
                                Some(literal_buffer.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    auth.set_modal(SettingsModal::AuthTextInput {
                        state: Box::new(auth_credential_input_state(literal)),
                    });
                }
                SourcePickerPlan::Op => {
                    auth.set_modal(SettingsModal::AuthOpPicker {
                        state: Box::new(crate::tui::op_picker::OpPickerState::new_with_cache(
                            op_cache,
                        )),
                    });
                }
                SourcePickerPlan::Dismiss => restore_settings_auth_form(auth),
                SourcePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        SettingsModal::AuthTextInput { state } => {
            match inline_picker_plan(state.handle_key(key.into())) {
                InlinePickerPlan::Commit(value) => {
                    if let Err(error) = commit_settings_auth_text(auth, value) {
                        auth.set_error(error);
                        auth.set_modal(modal);
                    }
                }
                InlinePickerPlan::Dismiss => restore_settings_auth_form(auth),
                InlinePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        SettingsModal::AuthSourceFolderPicker { state } => {
            let page_rows = page_rows_for_modal(term_size, state);
            let browser_outcome = state.handle_key_with_page_rows(key, Some(page_rows));
            match browser_outcome {
                crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
                | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
                | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
                    auth.set_modal(modal);
                    return SettingsAuthOutcome::ApplyFileBrowserOutcome(browser_outcome);
                }
                other => {
                    match auth_source_folder_picker_plan(other) {
                        AuthSourceFolderPickerPlan::Commit(path) => {
                            match validate_source_folder(auth.selected_kind(), &path) {
                                Ok(()) => apply_source_folder_to_settings_auth_form(auth, path),
                                // Wrong folder for this agent: keep the picker open and
                                // raise the standard error dialog (promoted from
                                // `auth.error`) over it, rather than committing a folder
                                // that yields no credentials. Dismissing the dialog
                                // leaves the picker so the operator can pick another.
                                Err(reason) => {
                                    auth.set_error(reason);
                                    auth.set_modal(modal);
                                }
                            }
                        }
                        AuthSourceFolderPickerPlan::Close => restore_settings_auth_form(auth),
                        AuthSourceFolderPickerPlan::KeepModal => {
                            auth.set_modal(modal);
                        }
                    }
                }
            }
        }
        SettingsModal::AuthOpPicker { state } => {
            let outcome = state.handle_key(key);
            match crate::tui::update::op_picker_inline_plan(outcome) {
                // Browse-mode caller: only `Existing` is reachable.
                InlinePickerPlan::Commit(
                    crate::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("settings-auth browse OpPicker runs in Browse mode"),
                InlinePickerPlan::Commit(crate::tui::op_picker::OpPickerSelection::Existing(
                    op_ref,
                )) => {
                    // Close the OpPicker — the auth form stays stashed on
                    // modal_parents so the _committed / _failed helpers find it.
                    // Dispatch already took the current picker. Preserve its
                    // parent until asynchronous validation completes.
                    return SettingsAuthOutcome::ValidateOpRef(op_ref);
                }
                InlinePickerPlan::Dismiss => restore_settings_auth_form(auth),
                InlinePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        _ => unreachable!("auth input handler received a non-auth settings modal"),
    }
    SettingsAuthOutcome::Continue
}

fn commit_settings_auth_text(
    auth: &mut crate::tui::state::SettingsAuthState,
    value: String,
) -> Result<(), String> {
    use crate::tui::screens::settings::model::AccountTextField;
    let Some(field) = auth.editing_text else {
        apply_plain_text_to_settings_auth_form(auth, &value);
        return Ok(());
    };
    let id = auth
        .editing_account
        .clone()
        .ok_or_else(|| "Account no longer exists".to_owned())?;
    if field == AccountTextField::DefaultAgent {
        let agent = jackin_core::Agent::from_slug(value.trim())
            .ok_or_else(|| "Unknown coding agent".to_owned())?;
        auth.toggle_account_default(&id, agent)?;
        auth.editing_text = None;
        return Ok(());
    }
    let account = auth
        .pending
        .get_mut(&id)
        .ok_or_else(|| "Account no longer exists".to_owned())?;
    match field {
        AccountTextField::DefaultAgent => unreachable!("default handled before metadata"),
        AccountTextField::Name => {
            if value.trim().is_empty() {
                return Err("Account name cannot be empty".into());
            }
            account.name = value;
        }
        AccountTextField::BaseUrl | AccountTextField::Model => {
            let jackin_config::AccountCredential::ApiKey {
                base_url, model, ..
            } = &mut account.credential
            else {
                return Err("Endpoint and model require an API account".into());
            };
            let target = if field == AccountTextField::BaseUrl {
                base_url
            } else {
                model
            };
            *target = (!value.trim().is_empty()).then(|| value.trim().to_owned());
        }
    }
    auth.editing_text = None;
    Ok(())
}

/// Translate a Create-mode `OpPicker` commit into a global
/// [`PendingTokenGenerate`](crate::tui::state::PendingTokenGenerate)
/// request that the `run_console` loop drains to mint the token.
/// `Existing` cannot occur in Create mode; a Cancel (or stray
/// `Existing`) just closes the chain. On `Continue` the picker is still
/// drilling, so the marker stays armed and the modal stays open.
fn restore_settings_auth_form(auth: &mut crate::tui::state::SettingsAuthState) {
    auth.restore_pending_auth_form();
}

/// Restore the account form with the supplied credential staged for save.
pub fn apply_plain_text_to_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    value: &str,
) {
    let Some(SettingsModal::AuthForm {
        target, mut state, ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    state.set_literal(value.to_owned());
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer: value.to_owned(),
    });
}

pub(crate) fn apply_source_folder_to_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    path: std::path::PathBuf,
) {
    let Some(SettingsModal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    state.set_source_folder(path);
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

/// Apply a committed op picker selection to the settings auth form after the
/// 1Password read has already succeeded on the `spawn_blocking` thread. Called
/// from the `run_console` poll loop — the read was verified asynchronously so
/// Touch ID / the 1Password desktop dialog did not freeze the TUI reactor.
///
/// The auth form is on `auth.modal_parents` — pop it, set the `OpRef` without
/// re-reading, and re-mount with focus on Save.
pub fn apply_op_picker_to_settings_auth_form_committed(
    auth: &mut crate::tui::state::SettingsAuthState,
    op_ref: jackin_core::OpRef,
) {
    let Some(SettingsModal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    // The read already succeeded; set the ref directly without re-reading.
    state.set_op_ref(op_ref);
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

fn persist_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    form: &AuthForm,
) {
    let Some(outcome) = form.commit() else {
        return;
    };
    let _ = env;
    use crate::tui::auth::{AuthKind, AuthMode};
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};
    if form.kind == AuthKind::Github {
        auth.github.auth_forward = match outcome.mode {
            AuthMode::Sync => jackin_config::GithubAuthMode::Sync,
            AuthMode::Token => jackin_config::GithubAuthMode::Token,
            AuthMode::Ignore => jackin_config::GithubAuthMode::Ignore,
            _ => {
                auth.set_error("Unsupported GitHub authentication");
                return;
            }
        };
        auth.github.env.remove("GH_TOKEN");
        if let Some(value) = outcome.env_value {
            auth.github.env.insert("GH_TOKEN".into(), value);
        }
        auth.selected_kind = None;
        return;
    }
    let provider = match form.kind {
        AuthKind::Claude => AiProvider::Anthropic,
        AuthKind::Codex => AiProvider::OpenAi,
        AuthKind::Amp => AiProvider::Amp,
        AuthKind::Kimi => AiProvider::Moonshot,
        AuthKind::Opencode => AiProvider::Opencode,
        AuthKind::Grok => AiProvider::Xai,
        AuthKind::Zai => AiProvider::Zai,
        AuthKind::Minimax => AiProvider::Minimax,
        AuthKind::Github => {
            auth.set_error("GitHub is not an AI provider");
            return;
        }
    };
    let credential = match outcome.mode {
        AuthMode::Sync => {
            let Some(directory) = outcome.source_folder else {
                auth.set_error("Select a profile folder");
                return;
            };
            let Some(agent) = crate::tui::auth_config::auth_kind_agent(form.kind) else {
                return;
            };
            AccountCredential::Profile { agent, directory }
        }
        AuthMode::ApiKey => {
            let Some(value) = outcome.env_value else {
                return;
            };
            let base_url = auth
                .editing_account
                .as_ref()
                .and_then(|id| auth.pending.get(id))
                .and_then(|a| {
                    if let AccountCredential::ApiKey { base_url, .. } = &a.credential {
                        base_url.clone()
                    } else {
                        None
                    }
                });
            let model = auth
                .editing_account
                .as_ref()
                .and_then(|id| auth.pending.get(id))
                .and_then(|a| {
                    if let AccountCredential::ApiKey { model, .. } = &a.credential {
                        model.clone()
                    } else {
                        None
                    }
                });
            AccountCredential::ApiKey {
                value,
                base_url,
                model,
            }
        }
        AuthMode::OAuthToken => {
            let Some(value) = outcome.env_value else {
                return;
            };
            AccountCredential::OAuthToken {
                agent: jackin_core::Agent::Claude,
                value,
            }
        }
        _ => {
            auth.set_error("Choose a profile, API key, or OAuth token");
            return;
        }
    };
    let id = auth.editing_account.clone().unwrap_or_else(|| {
        let mut suffix = 1;
        loop {
            let id = format!("{}-{suffix}", provider.slug());
            if !auth.pending.contains_key(&id) {
                break id;
            }
            suffix += 1;
        }
    });
    let name = auth
        .pending
        .get(&id)
        .map_or_else(|| id.clone(), |a| a.name.clone());
    let enabled = auth.pending.get(&id).is_none_or(|account| account.enabled);
    auth.pending.insert(
        id.clone(),
        AccountConfig {
            enabled,
            name,
            provider,
            credential,
        },
    );
    auth.selected = auth.pending.keys().position(|key| key == &id).unwrap_or(0);
    auth.selected_kind = None;
}

fn clear_settings_auth_kind(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    target: &AuthFormTarget,
) {
    let AuthFormTarget::Workspace { kind } = target else {
        return;
    };
    let _ = env;
    if *kind == crate::tui::auth::AuthKind::Github {
        auth.github = jackin_config::GithubAuthConfig::default();
    }
    if let Some(id) = auth.editing_account.take() {
        auth.pending.remove(&id);
        auth.bindings.retain(|_, value| value != &id);
    }
    auth.clamp_selected_row();
}

#[cfg(test)]
mod tests;
