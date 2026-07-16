// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Line builders for the shared 1Password picker modal.

use std::collections::HashSet;

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use jackin_core::tui_theme::text_fg;

use super::{
    FieldDisplayRow, OpPickerAccountRef, OpPickerFatalState, OpPickerFieldDisplayRef,
    OpPickerItemRef, OpPickerStage, OpPickerVaultRef,
};

/// `+ New X` creation row, styled like picker list rows.
pub fn sentinel_line(text: &str, _is_selected: bool) -> Line<'static> {
    Line::from(Span::styled(
        text.to_owned(),
        jackin_core::tui_theme::text_muted(),
    ))
}

pub fn account_lines<'a>(
    accounts: impl IntoIterator<Item = OpPickerAccountRef<'a>> + 'a,
    _selected: Option<usize>,
) -> Vec<Line<'static>> {
    accounts
        .into_iter()
        .map(|account| {
            Line::from(vec![
                Span::styled(account.email.to_owned(), Style::default().fg(text_fg())),
                Span::raw("  "),
                Span::styled(
                    format!("({})", account.url),
                    jackin_core::tui_theme::text_muted(),
                ),
            ])
        })
        .collect()
}

pub fn vault_lines<'a>(
    vaults: impl IntoIterator<Item = OpPickerVaultRef<'a>> + 'a,
    _selected: Option<usize>,
) -> Vec<Line<'static>> {
    vaults
        .into_iter()
        .map(|vault| {
            Line::from(Span::styled(
                vault.name.to_owned(),
                Style::default().fg(text_fg()),
            ))
        })
        .collect()
}

pub fn item_choice_lines<'a>(
    item_choices: impl IntoIterator<Item = Option<OpPickerItemRef<'a>>> + 'a,
    _selected: Option<usize>,
) -> Vec<Line<'static>> {
    item_choices
        .into_iter()
        .map(|choice| {
            choice.map_or_else(
                || sentinel_line("+ New item", false),
                |item| {
                    let mut spans = vec![Span::styled(
                        item.name.to_owned(),
                        Style::default().fg(text_fg()),
                    )];
                    if !item.subtitle.is_empty() {
                        let dim = jackin_core::tui_theme::text_muted();
                        spans.push(Span::styled(" (", dim));
                        spans.push(Span::styled(item.subtitle.to_owned(), dim));
                        spans.push(Span::styled(")", dim));
                    }
                    Line::from(spans)
                },
            )
        })
        .collect()
}

/// Render section-stage rows: `(root)`, named sections, then a creation
/// sentinel.
pub fn section_lines(
    choices: impl IntoIterator<Item = Option<String>>,
    _selected: Option<usize>,
) -> Vec<Line<'static>> {
    let choices: Vec<Option<String>> = choices.into_iter().collect();
    let mut lines: Vec<Line<'static>> = choices
        .into_iter()
        .map(|choice| {
            let label = choice.unwrap_or_else(|| "(root)".to_owned());
            Line::from(Span::styled(label, Style::default().fg(text_fg())))
        })
        .collect();
    lines.push(sentinel_line("+ New section", false));
    lines
}

pub fn field_lines<'a>(
    rows: impl IntoIterator<Item = FieldDisplayRow>,
    fields: impl IntoIterator<Item = OpPickerFieldDisplayRef<'a>>,
    collapsed_sections: &HashSet<String>,
    _selected: Option<usize>,
) -> Vec<Line<'static>> {
    let fields: Vec<OpPickerFieldDisplayRef<'a>> = fields.into_iter().collect();
    let label_w = fields
        .iter()
        .map(|field| field_display_label(*field).chars().count())
        .max()
        .unwrap_or(0)
        .max(8);

    rows.into_iter()
        .map(|row| match row {
            FieldDisplayRow::SectionHeader { name, field_count } => {
                section_header_line(&name, field_count, collapsed_sections)
            }
            FieldDisplayRow::Field { field_idx } => {
                let Some(field) = fields.get(field_idx).copied() else {
                    return Line::default();
                };
                field_line(field, label_w)
            }
            FieldDisplayRow::NewFieldSentinel => sentinel_line("+ New field", false),
            FieldDisplayRow::NewSectionSentinel => sentinel_line("+ New section", false),
        })
        .collect()
}

fn section_header_line(
    name: &str,
    field_count: usize,
    collapsed_sections: &HashSet<String>,
) -> Line<'static> {
    let arrow = if collapsed_sections.contains(name) {
        "\u{25b6}"
    } else {
        "\u{25bc}"
    };
    let style = jackin_core::tui_theme::text_muted();
    let count_label = format!(
        "({} {})",
        field_count,
        if field_count == 1 { "field" } else { "fields" }
    );
    Line::from(vec![
        Span::styled(arrow, style),
        Span::styled(format!(" {name}  "), style),
        Span::styled(count_label, jackin_core::tui_theme::text_muted()),
    ])
}

fn field_line(field: OpPickerFieldDisplayRef<'_>, label_w: usize) -> Line<'static> {
    let label = field_display_label(field);
    let pad = label_w.saturating_sub(label.chars().count());
    let label_style = Style::default().fg(text_fg());
    let annotation = if field.concealed {
        "(concealed)".to_owned()
    } else {
        format!("({})", field.field_type.to_lowercase())
    };
    Line::from(vec![
        Span::styled(label, label_style),
        Span::raw(format!("{}  ", " ".repeat(pad))),
        Span::styled(annotation, jackin_core::tui_theme::text_muted()),
    ])
}

fn field_display_label(field: OpPickerFieldDisplayRef<'_>) -> String {
    if field.label.is_empty() {
        field.id.to_owned()
    } else {
        field.label.to_owned()
    }
}

pub fn loading_title_stage(stage: OpPickerStage) -> OpPickerStage {
    if matches!(stage, OpPickerStage::Field) {
        OpPickerStage::Item
    } else {
        stage
    }
}

pub fn loading_descriptor(
    stage: OpPickerStage,
    multi_account: bool,
    account_email: &str,
    vault_name: &str,
    item_name: &str,
    item_subtitle: &str,
) -> String {
    match stage {
        OpPickerStage::Account => "loading accounts\u{2026}".to_owned(),
        OpPickerStage::Vault => {
            if multi_account && !account_email.is_empty() {
                format!("loading vaults from {account_email}\u{2026}")
            } else {
                "loading vaults\u{2026}".to_owned()
            }
        }
        OpPickerStage::Item => {
            format!("loading items from {vault_name}\u{2026}")
        }
        OpPickerStage::Field => {
            if item_subtitle.is_empty() {
                format!("loading {item_name}\u{2026}")
            } else {
                format!("loading {item_name} ({item_subtitle})\u{2026}")
            }
        }
        OpPickerStage::Section
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => "loading\u{2026}".to_owned(),
    }
}

pub fn fatal_body_lines(fatal: &OpPickerFatalState) -> Vec<Line<'static>> {
    match fatal {
        OpPickerFatalState::NotInstalled => vec![
            Line::from(Span::styled(
                "1Password CLI not found.",
                jackin_core::tui_theme::text_strong(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Install: brew install 1password-cli (macOS)",
                jackin_core::tui_theme::accent(),
            )),
            Line::from(Span::styled(
                "or visit 1password.com/downloads/command-line/",
                jackin_core::tui_theme::accent(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "After install, run `op signin`, then press P to retry.",
                jackin_core::tui_theme::text_muted(),
            )),
        ],
        OpPickerFatalState::NotSignedIn => vec![
            Line::from(Span::styled(
                "1Password CLI is not signed in.",
                jackin_core::tui_theme::text_strong(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Run `op signin` in your shell, then retry.",
                jackin_core::tui_theme::accent(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "jackin❯ uses your existing op session — there is no separate jackin❯ auth.",
                jackin_core::tui_theme::text_muted(),
            )),
        ],
        OpPickerFatalState::NoVaults => vec![
            Line::from(Span::styled(
                "No vaults available.",
                jackin_core::tui_theme::text_strong(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Check 1Password's app integration settings:",
                jackin_core::tui_theme::accent(),
            )),
            Line::from(Span::styled(
                "Settings \u{2192} Developer \u{2192} CLI integration.",
                jackin_core::tui_theme::accent(),
            )),
        ],
        OpPickerFatalState::GenericFatal { message } => {
            let truncated: String = message.chars().take(120).collect();
            vec![
                Line::from(Span::styled(
                    "1Password CLI error.",
                    jackin_core::tui_theme::text_strong(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    truncated,
                    jackin_core::tui_theme::text_muted(),
                )),
            ]
        }
    }
}
