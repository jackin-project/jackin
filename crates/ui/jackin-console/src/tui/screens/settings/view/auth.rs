// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth tab lines.

use super::super::model::SettingsAuthState;

use super::super::model::SettingsEnvState;

use ratatui::text::Line;

use crate::tui::components::editor_rows::{AuthLineRow, auth_lines as shared_auth_lines};

#[must_use]
pub fn auth_lines(
    rows: &[AuthLineRow],
    selected_row: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    shared_auth_lines(rows, selected_row, show_cursor)
}

#[must_use]
pub fn auth_state_lines<AuthModal, EnvModal, PendingOpCommit>(
    auth: &SettingsAuthState<jackin_core::EnvValue, AuthModal, PendingOpCommit>,
    env: &SettingsEnvState<jackin_core::EnvValue, EnvModal>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let _ = env;
    let mut rows: Vec<AuthLineRow> = auth
        .pending
        .iter()
        .map(|(id, account)| {
            let source = match &account.credential {
                jackin_config::AccountCredential::Profile { directory, .. } => {
                    format!(
                        "profile: {}",
                        jackin_core::shorten_home(&directory.to_string_lossy())
                    )
                }
                jackin_config::AccountCredential::ApiKey { .. } => "API key: ••••••••".to_owned(),
                jackin_config::AccountCredential::OAuthToken { .. } => {
                    "OAuth token: ••••••••".to_owned()
                }
            };
            let defaults = auth
                .bindings
                .iter()
                .filter(|(_, value)| *value == id)
                .map(|(agent, _)| agent.slug())
                .collect::<Vec<_>>()
                .join(", ");
            let state = if account.enabled {
                "enabled"
            } else {
                "disabled"
            };
            let default_label = if defaults.is_empty() {
                String::new()
            } else {
                format!(" · default: {defaults}")
            };
            let scanned_label = if auth.scan.scanned_ids.contains(id) {
                " · scanned"
            } else {
                ""
            };
            AuthLineRow::AuthKind {
                label: format!(
                    "{} [{}] · {state}{default_label} · {} · {source}{scanned_label}",
                    account.name, id, account.provider
                ),
            }
        })
        .collect();
    rows.extend(
        super::super::model::ACCOUNT_KINDS
            .iter()
            .map(|kind| AuthLineRow::AuthKind {
                label: format!("+ Add {} account", kind.label()),
            }),
    );
    rows.push(AuthLineRow::AuthKind {
        label: format!("GitHub CLI · {}", auth.github.auth_forward),
    });
    rows.push(AuthLineRow::AuthKind {
        label: if auth.scan.in_flight {
            "Scanning for accounts…".to_owned()
        } else {
            "Scan for accounts…".to_owned()
        },
    });
    let mut lines = auth_lines(&rows, auth.selected, show_cursor);
    lines.extend(account_scan_status_lines(&auth.scan));
    lines
}

/// Trailing non-selectable status lines for the last scan: the merge
/// summary plus one line per discovery issue. Metadata only (agents,
/// error categories, directories) — never credential values.
pub(crate) fn account_scan_status_lines(
    scan: &super::super::model::AccountScanState,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(summary) = &scan.last_summary {
        let first_run = if summary.fresh_install {
            " (first run)"
        } else {
            ""
        };
        lines.push(Line::from(format!(
            "  Scan: joined {}, already present {}{first_run}",
            summary.joined.len(),
            summary.skipped.len(),
        )));
    }
    for issue in &scan.issues {
        let agent = issue.agent;
        let error = issue.error;
        lines.push(Line::from(format!(
            "  Scan issue: {agent}: {error} ({})",
            issue.directory.display()
        )));
    }
    lines
}
