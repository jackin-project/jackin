//! Settings screen view helpers.

use super::model::SettingsEnvScope;
use super::model::SettingsAuthRow;
use super::model::SettingsTab;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

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
pub fn general_lines(
    selected_row: usize,
    pending_coauthor_trailer: bool,
    pending_dco: bool,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let label_bold = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let label_normal = Style::default().fg(jackin_tui::theme::WHITE);
    let value_bold = Style::default()
        .fg(jackin_tui::theme::PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);
    let value_normal = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);

    let rows: [(usize, &str, bool); 2] = [
        (0, "Co-author trailer", pending_coauthor_trailer),
        (1, "DCO sign-off", pending_dco),
    ];

    rows.iter()
        .map(|(i, label, pending)| {
            let selected = show_cursor && (selected_row == *i);
            let prefix = if selected { "\u{25b8} " } else { "  " };
            let ls = if selected { label_bold } else { label_normal };
            let vs = if selected { value_bold } else { value_normal };
            let value = if *pending { "enabled" } else { "disabled" };
            Line::from(vec![
                Span::styled(prefix, ls),
                Span::styled(format!("{label:<26}"), ls),
                Span::styled(value, vs),
            ])
        })
        .collect()
}

pub fn clamp_mounts_scroll_x_for_frame(area: Rect, content_width: usize, scroll_x: &mut u16) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        content_width,
        jackin_tui::components::scrollable_panel::viewport_width(chunks[2]),
        scroll_x,
    );
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
    fn general_lines_highlight_selected_setting() {
        let lines = general_lines(1, true, false, true);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
        assert_eq!(lines[0].spans[2].content.as_ref(), "enabled");
        assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} ");
        assert_eq!(lines[1].spans[2].content.as_ref(), "disabled");
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
