//! Settings screen view helpers.

use super::model::SettingsEnvScope;
use super::model::SettingsAuthRow;
use super::model::SettingsTab;

#[must_use]
pub fn tab_labels(active: SettingsTab) -> Vec<(&'static str, bool)> {
    SettingsTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}

#[must_use]
pub fn env_scope_label(scope: &SettingsEnvScope) -> &str {
    match scope {
        SettingsEnvScope::Global => "global",
        SettingsEnvScope::Role(role) => role.as_str(),
    }
}

#[must_use]
pub fn env_forbidden_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "global env".to_string(),
        SettingsEnvScope::Role(role) => format!("role {role}"),
    }
}

#[must_use]
pub fn content_height_with_error_rows(height: usize, has_error: bool) -> usize {
    if has_error {
        height.saturating_add(2)
    } else {
        height
    }
}

#[must_use]
pub fn auth_content_height<K, M>(
    selected_kind: Option<K>,
    rows: &[SettingsAuthRow<K, M>],
    mode_needs_credential: impl Fn(K, &M) -> bool,
    has_error: bool,
) -> usize
where
    K: Copy + PartialEq,
{
    let height = match selected_kind {
        None => rows.len(),
        Some(kind) => rows.iter().find(|row| row.kind == kind).map_or(0, |row| {
            if mode_needs_credential(kind, &row.mode) {
                3
            } else {
                2
            }
        }),
    };
    content_height_with_error_rows(height, has_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Plain,
        Credential,
    }

    #[test]
    fn auth_content_height_lists_all_kinds_before_drill_in() {
        let rows = vec![
            SettingsAuthRow {
                kind: Kind::Plain,
                mode: false,
            },
            SettingsAuthRow {
                kind: Kind::Credential,
                mode: true,
            },
        ];

        assert_eq!(auth_content_height(None, &rows, |_, mode| *mode, false), 2);
    }

    #[test]
    fn auth_content_height_drill_in_tracks_credential_row_and_error() {
        let rows = vec![SettingsAuthRow {
            kind: Kind::Credential,
            mode: true,
        }];

        assert_eq!(
            auth_content_height(Some(Kind::Credential), &rows, |_, mode| *mode, true),
            5
        );
    }
}
