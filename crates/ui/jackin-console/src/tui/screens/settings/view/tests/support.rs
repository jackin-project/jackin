// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn scan_view_account() -> jackin_config::AccountConfig {
    jackin_config::AccountConfig {
        enabled: true,
        name: "Claude default".into(),
        provider: jackin_config::AiProvider::Anthropic,
        credential: jackin_config::AccountCredential::Profile {
            agent: jackin_core::Agent::Claude,
            directory: "/home/op/.claude".into(),
            xdg_roots: None,
            source_selector: None,
        },
    }
}

pub(super) fn scan_view_auth() -> crate::tui::state::SettingsAuthState {
    crate::tui::state::SettingsAuthState::from_accounts(BTreeMap::from([(
        "default-claude".to_owned(),
        scan_view_account(),
    )]))
}

pub(super) fn scan_view_env() -> crate::tui::state::SettingsEnvState<'static> {
    crate::tui::state::SettingsEnvState::from_config(&jackin_config::AppConfig::default())
}

pub(super) fn line_texts(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}
