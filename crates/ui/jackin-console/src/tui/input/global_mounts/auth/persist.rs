// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings auth form persistence.

use crate::tui::state::AuthForm;

use crate::tui::state::AuthFormTarget;

pub(crate) fn persist_settings_auth_form(
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
        AuthKind::Antigravity | AuthKind::Gemini => AiProvider::Google,
        AuthKind::Cursor => AiProvider::Cursor,
        AuthKind::Muse => AiProvider::Meta,
        AuthKind::Omp | AuthKind::Hermes => {
            // No native provider: edits keep the existing account's
            // provider; only brand-new console accounts are refused (the
            // console has no provider picker yet — use the CLI).
            let existing = auth
                .editing_account
                .as_ref()
                .and_then(|id| auth.pending.get(id))
                .map(|account| account.provider);
            let Some(provider) = existing else {
                auth.set_error("omp/Hermes accounts need an explicit provider; add them with `jackin account add --provider`");
                return;
            };
            provider
        }
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
            let source_selector = auth
                .editing_account
                .as_ref()
                .and_then(|id| auth.pending.get(id))
                .and_then(|account| match &account.credential {
                    AccountCredential::Profile {
                        agent: existing_agent,
                        directory: existing_directory,
                        source_selector,
                        ..
                    } if *existing_agent == agent && *existing_directory == directory => {
                        source_selector.clone()
                    }
                    _ => None,
                });
            AccountCredential::Profile {
                agent,
                directory,
                xdg_roots: None,
                source_selector,
            }
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

pub(crate) fn clear_settings_auth_kind(
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
