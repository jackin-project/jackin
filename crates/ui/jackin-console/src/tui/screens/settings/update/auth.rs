// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings auth row and scan plans.

use super::super::effect::SettingsEffect;
use super::super::message::SettingsMessage;
use super::super::model::{ACCOUNT_KINDS, SettingsAuthState};
use std::collections::BTreeMap;

#[must_use]
pub fn settings_auth_selected_index(selected: usize, row_count: usize) -> usize {
    selected.min(row_count.saturating_sub(1))
}

/// Which row kind the Accounts tab cursor addresses. The scan action row
/// stays last so the GitHub index never moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAuthRowKind {
    /// Existing pending account at this draft index.
    Account(usize),
    /// "+ Add {kind}" row.
    AddKind(crate::tui::auth::AuthKind),
    /// GitHub CLI row.
    Github,
    /// "Scan for accounts" action row.
    Scan,
}

#[must_use]
pub fn settings_auth_row_kind(pending_len: usize, selected: usize) -> SettingsAuthRowKind {
    if selected < pending_len {
        return SettingsAuthRowKind::Account(selected);
    }
    if let Some(kind) = ACCOUNT_KINDS
        .get(selected.saturating_sub(pending_len))
        .copied()
    {
        return SettingsAuthRowKind::AddKind(kind);
    }
    if selected == pending_len + ACCOUNT_KINDS.len() {
        return SettingsAuthRowKind::Github;
    }
    SettingsAuthRowKind::Scan
}

/// Whether the cursor addresses the scan action row (last row).
#[must_use]
pub const fn settings_auth_scan_row_selected(pending_len: usize, selected: usize) -> bool {
    selected == pending_len + ACCOUNT_KINDS.len() + 1
}

/// Pure reducer for account-scan messages. Root executes the returned
/// effect (worker spawn); every other scan message mutates state only.
/// Non-scan messages are ignored here (handled by their own reducers).
#[must_use]
pub fn reduce_account_scan_message<V, M, P>(
    auth: &mut SettingsAuthState<V, M, P>,
    message: &SettingsMessage,
) -> Option<SettingsEffect> {
    match message {
        SettingsMessage::RequestAccountScan => auth.begin_account_scan(),
        SettingsMessage::AccountScanCompleted { generation, result } => {
            auth.complete_account_scan(*generation, result);
            None
        }
        SettingsMessage::CancelAccountScan => {
            auth.cancel_account_scan();
            None
        }
        SettingsMessage::FocusTabBar | SettingsMessage::FocusContent => None,
    }
}

/// Whether `candidate`'s credential source is already in the pending draft
/// under any ID. Same match arms as the config `upsert_account`
/// duplicate-source rule, so scan merges skip instead of staging a
/// save-time error.
#[must_use]
pub fn scanned_source_in_draft(
    pending: &BTreeMap<String, jackin_config::AccountConfig>,
    candidate: &jackin_config::AccountConfig,
) -> bool {
    use jackin_config::AccountCredential;
    pending.values().any(|account| {
        if account.provider != candidate.provider {
            return false;
        }
        match (&candidate.credential, &account.credential) {
            (
                AccountCredential::Profile {
                    agent: a,
                    directory: x,
                    xdg_roots: rx,
                    source_selector: sx,
                },
                AccountCredential::Profile {
                    agent: b,
                    directory: y,
                    xdg_roots: ry,
                    source_selector: sy,
                },
            ) => a == b && x == y && rx == ry && sx == sy,
            (
                AccountCredential::ApiKey {
                    value: x,
                    base_url: a,
                    ..
                },
                AccountCredential::ApiKey {
                    value: y,
                    base_url: b,
                    ..
                },
            ) => x == y && a == b,
            (
                AccountCredential::OAuthToken { agent: a, value: x },
                AccountCredential::OAuthToken { agent: b, value: y },
            ) => a == b && x == y,
            _ => false,
        }
    })
}
