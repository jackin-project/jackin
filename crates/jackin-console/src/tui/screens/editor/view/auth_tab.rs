//! Auth tab lines, geometry, widths, `EditorAuthLineRow` and render helpers extracted
//! from the view coordinator. Items re-exported from parent to preserve `super::*`
//! call sites in tests and qualified calls from frame.rs (via `render_auth_tab` etc).

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::components::editor_rows::{
    AUTH_LABEL_COL_WIDTH, AuthSourceDisplay, AuthSourceFolderDisplay, AuthSourceFolderKind,
    AuthSourceValue, action_row_style, auth_source_display_for_required_env, disclosure_style,
};
use crate::tui::screens::editor::model::{AuthRow, FieldFocus};

use super::WorkspaceEditorState;

// Structural exception: editor rows are form/table rows with labels, values,
// disclosures, masked secrets, and action sentinels, so they cannot use the
// flat picker renderer even though they share its focus-gated cursor contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditorAuthLineRow {
    AuthKind { label: String },
    WorkspaceMode { mode_label: String, inherited: bool },
    WorkspaceSource { display: AuthSourceDisplay },
    WorkspaceSourceFolder { display: AuthSourceFolderDisplay },
    RoleHeader { role: String, expanded: bool },
    RoleMode { mode_label: String },
    RoleSource { display: AuthSourceDisplay },
    RoleSourceFolder { display: AuthSourceFolderDisplay },
    AddSentinel { eligible: usize },
    Spacer,
}

#[must_use]
pub(crate) fn auth_lines(
    rows: &[EditorAuthLineRow],
    cursor: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| render_auth_line(show_cursor && (i == cursor), row))
        .collect()
}

#[must_use]
pub(crate) fn auth_display_row(
    row: &AuthRow<crate::tui::auth::AuthKind>,
    synthesized: &jackin_config::AppConfig,
    workspace_name: &str,
) -> EditorAuthLineRow {
    match row {
        AuthRow::AuthKindRow { kind } => EditorAuthLineRow::AuthKind {
            label: kind.label().to_owned(),
        },
        AuthRow::WorkspaceMode { kind } => {
            let ws = synthesized.workspaces.get(workspace_name);
            let explicit =
                ws.and_then(|ws| crate::tui::auth_config::explicit_workspace_auth_mode(ws, *kind));
            let mode = explicit.unwrap_or_else(|| {
                crate::tui::auth_config::resolve_panel_mode(synthesized, *kind, workspace_name, "")
            });
            EditorAuthLineRow::WorkspaceMode {
                mode_label: crate::tui::components::auth_panel::mode_str(mode).to_owned(),
                inherited: explicit.is_none(),
            }
        }
        AuthRow::WorkspaceSource { kind } => EditorAuthLineRow::WorkspaceSource {
            display: editor_auth_source_display(synthesized, workspace_name, "", *kind),
        },
        AuthRow::WorkspaceSourceFolder { kind } => EditorAuthLineRow::WorkspaceSourceFolder {
            display: crate::tui::auth_config::editor_source_folder_display(
                synthesized,
                workspace_name,
                "",
                *kind,
            ),
        },
        AuthRow::RoleHeader { role, expanded } => EditorAuthLineRow::RoleHeader {
            role: role.clone(),
            expanded: *expanded,
        },
        AuthRow::RoleMode { role, kind } => {
            let mode = crate::tui::auth_config::resolve_panel_mode(
                synthesized,
                *kind,
                workspace_name,
                role,
            );
            EditorAuthLineRow::RoleMode {
                mode_label: crate::tui::components::auth_panel::mode_str(mode).to_owned(),
            }
        }
        AuthRow::RoleSource { role, kind } => EditorAuthLineRow::RoleSource {
            display: editor_auth_source_display(synthesized, workspace_name, role, *kind),
        },
        AuthRow::RoleSourceFolder { role, kind } => EditorAuthLineRow::RoleSourceFolder {
            display: crate::tui::auth_config::editor_source_folder_display(
                synthesized,
                workspace_name,
                role,
                *kind,
            ),
        },
        AuthRow::AddSentinel { eligible } => EditorAuthLineRow::AddSentinel {
            eligible: *eligible,
        },
        AuthRow::Spacer => EditorAuthLineRow::Spacer,
    }
}

#[must_use]
#[allow(clippy::type_complexity)]
pub(crate) fn auth_state_lines<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let synthesized = state.synthesize_app_config_for_auth(config);
    let workspace_name = state.workspace_name_for_panel();
    let rows = state.auth_flat_rows(config);

    let FieldFocus::Row(cursor) = state.active_field;
    let max_idx = rows.len().saturating_sub(1);
    let cursor_clamped = cursor.min(max_idx);

    let display_rows: Vec<EditorAuthLineRow> = rows
        .iter()
        .map(|row| auth_display_row(row, &synthesized, &workspace_name))
        .collect();
    auth_lines(&display_rows, cursor_clamped, show_cursor)
}

#[must_use]
#[allow(clippy::type_complexity)]
pub(crate) fn auth_state_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) -> super::EditorTabContentGeometry {
    let rows = state.auth_flat_rows(config);
    let synthesized = state.synthesize_app_config_for_auth(config);
    let workspace_name = state.workspace_name_for_panel();
    let content_width = rows
        .iter()
        .map(|row| {
            let display_row = auth_display_row(row, &synthesized, &workspace_name);
            editor_auth_line_width(&display_row)
        })
        .max()
        .unwrap_or(0);
    super::EditorTabContentGeometry {
        content_width,
        content_height: rows.len(),
    }
}

fn editor_auth_source_display(
    synthesized: &jackin_config::AppConfig,
    workspace_name: &str,
    role: &str,
    kind: crate::tui::auth::AuthKind,
) -> AuthSourceDisplay {
    let mode = crate::tui::auth_config::resolve_panel_mode(synthesized, kind, workspace_name, role);
    let env_name = kind.required_env_var(mode);

    let value = env_name
        .and_then(|env_name| {
            crate::tui::auth_config::panel_auth_source_value(
                synthesized,
                workspace_name,
                role,
                env_name,
                kind,
            )
        })
        .map(|value| match value {
            jackin_core::EnvValue::OpRef(r) => AuthSourceValue::OpRefPath(r.path.clone()),
            jackin_core::EnvValue::Plain(s) => AuthSourceValue::Plain(s.clone()),
            jackin_core::EnvValue::Extended(e) => AuthSourceValue::Plain(e.value.clone()),
        });

    auth_source_display_for_required_env(
        env_name,
        value,
        crate::tui::components::auth_panel::mode_str(mode),
    )
}

#[must_use]
pub(crate) fn editor_auth_line_width(row: &EditorAuthLineRow) -> usize {
    match row {
        EditorAuthLineRow::AuthKind { label } => super::padded_width(&format!("  {label}")),
        EditorAuthLineRow::WorkspaceMode {
            mode_label,
            inherited,
        } => {
            let suffix = if *inherited { " (inherited)" } else { "" };
            super::padded_width(&format!(
                "  {:<AUTH_LABEL_COL_WIDTH$}{mode_label}{suffix}",
                "Mode"
            ))
        }
        EditorAuthLineRow::WorkspaceSource { display } => {
            auth_source_line_width("Source", display, 0)
        }
        EditorAuthLineRow::WorkspaceSourceFolder { display } => {
            source_folder_line_width("Source folder", display, 0)
        }
        EditorAuthLineRow::RoleHeader { role, .. } => {
            super::padded_width(&format!("\u{25bc} Role: {role}"))
        }
        EditorAuthLineRow::RoleMode { mode_label } => super::padded_width(&format!(
            "      {:<AUTH_LABEL_COL_WIDTH$}{mode_label}",
            "Mode"
        )),
        EditorAuthLineRow::RoleSource { display } => auth_source_line_width("Source", display, 6),
        EditorAuthLineRow::RoleSourceFolder { display } => {
            source_folder_line_width("Source folder", display, 6)
        }
        EditorAuthLineRow::AddSentinel { .. } => super::padded_width("  + Override for a role"),
        EditorAuthLineRow::Spacer => 0,
    }
}

fn render_auth_line(selected: bool, row: &EditorAuthLineRow) -> Line<'static> {
    let bold_white = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let dim_green = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let phosphor = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);

    match row {
        EditorAuthLineRow::AuthKind { label } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::raw(cursor_col),
                Span::styled(label.clone(), bold_white),
            ])
        }
        EditorAuthLineRow::WorkspaceMode {
            mode_label,
            inherited,
        } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            let suffix = if *inherited { " (inherited)" } else { "" };
            Line::from(vec![
                Span::raw(cursor_col),
                Span::styled(format!("{:<AUTH_LABEL_COL_WIDTH$}", "Mode"), bold_white),
                Span::styled(mode_label.clone(), phosphor),
                Span::styled(suffix.to_owned(), dim_green),
            ])
        }
        EditorAuthLineRow::WorkspaceSource { display } => {
            render_auth_source_line("Source", display, 0, selected)
        }
        EditorAuthLineRow::WorkspaceSourceFolder { display } => {
            render_source_folder_line("Source folder", display, 0, selected)
        }
        EditorAuthLineRow::RoleHeader { role, expanded } => {
            let glyph = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
            Line::from(vec![
                Span::styled(glyph.to_owned(), disclosure_style()),
                Span::styled(format!(" Role: {role}"), disclosure_style()),
            ])
        }
        EditorAuthLineRow::RoleMode { mode_label } => Line::from(vec![
            Span::raw("      "),
            Span::styled(format!("{:<AUTH_LABEL_COL_WIDTH$}", "Mode"), bold_white),
            Span::styled(mode_label.clone(), phosphor),
        ]),
        EditorAuthLineRow::RoleSource { display } => {
            render_auth_source_line("Source", display, 6, false)
        }
        EditorAuthLineRow::RoleSourceFolder { display } => {
            render_source_folder_line("Source folder", display, 6, false)
        }
        EditorAuthLineRow::AddSentinel { .. } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::styled(cursor_col, action_row_style(selected)),
                Span::styled("+ Override for a role", action_row_style(selected)),
            ])
        }
        EditorAuthLineRow::Spacer => Line::from(""),
    }
}

fn source_folder_line_width(
    label: &str,
    display: &AuthSourceFolderDisplay,
    indent: usize,
) -> usize {
    let gutter_width = if indent == 0 { 2 } else { indent };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let prefix_width = gutter_width + super::text_width(&format!("{label:<label_width$}"));
    let value = source_folder_display_text(display);
    super::padded_width_cols(prefix_width + super::text_width(&value), gutter_width)
}

fn render_source_folder_line(
    label: &str,
    display: &AuthSourceFolderDisplay,
    indent: usize,
    selected: bool,
) -> Line<'static> {
    let cursor_col = if selected { "\u{25b8} " } else { "  " };
    let prefix = if indent == 0 {
        cursor_col.to_owned()
    } else {
        " ".repeat(indent)
    };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let value = source_folder_display_text(display);
    Line::from(vec![
        Span::raw(prefix),
        Span::styled(
            format!("{label:<label_width$}"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value, Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM)),
    ])
}

fn source_folder_display_text(display: &AuthSourceFolderDisplay) -> String {
    match display.kind {
        AuthSourceFolderKind::Default => format!("default: {}", display.path),
        AuthSourceFolderKind::Explicit => display.path.clone(),
        AuthSourceFolderKind::Inherited => format!("inherited: {}", display.path),
    }
}

fn auth_source_line_width(label: &str, display: &AuthSourceDisplay, indent: usize) -> usize {
    let gutter_width = if indent == 0 { 2 } else { indent };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let prefix_width = gutter_width + super::text_width(&format!("{label:<label_width$}"));
    let value_width = match display {
        AuthSourceDisplay::NotRequired => super::text_width("not required"),
        AuthSourceDisplay::OpRefPath(path) => {
            super::text_width("[op] ")
                + crate::tui::op_breadcrumb::parse_path_breadcrumb(path).map_or_else(
                    || super::text_width("<unparseable path - re-pick>"),
                    |parts| crate::tui::op_breadcrumb::breadcrumb_display_width(&parts),
                )
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            super::text_width(&"\u{25cf}".repeat((*chars).clamp(1, 12)))
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => super::text_width(&format!("unset  ({env_name} for {mode_label})")),
    };
    super::padded_width_cols(prefix_width + value_width, gutter_width)
}

fn render_auth_source_line(
    label: &str,
    display: &AuthSourceDisplay,
    indent: usize,
    selected: bool,
) -> Line<'static> {
    let cursor_col = if selected { "\u{25b8} " } else { "  " };
    let prefix = if indent == 0 {
        cursor_col.to_owned()
    } else {
        " ".repeat(indent)
    };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let mut spans = vec![
        Span::raw(prefix),
        Span::styled(
            format!("{label:<label_width$}"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    match display {
        AuthSourceDisplay::NotRequired => {
            spans.push(Span::styled(
                "not required",
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
        }
        AuthSourceDisplay::OpRefPath(path) => {
            spans.push(Span::styled(
                "[op] ",
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
            crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans(&mut spans, path);
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            spans.push(Span::styled(
                "\u{25cf}".repeat((*chars).clamp(1, 12)),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => {
            spans.push(Span::styled(
                format!("unset  ({env_name} for {mode_label})"),
                Style::default().fg(jackin_tui::theme::DANGER_RED),
            ));
        }
    }

    Line::from(spans)
}
