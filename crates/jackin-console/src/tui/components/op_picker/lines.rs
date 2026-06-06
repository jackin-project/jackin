//! Line builders for the shared 1Password picker modal.

use std::collections::HashSet;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use jackin_tui::theme::{PHOSPHOR_GREEN, WHITE};

use super::{
    FieldDisplayRow, OpPickerAccountRef, OpPickerFatalState, OpPickerFieldDisplayRef,
    OpPickerItemRef, OpPickerStage, OpPickerVaultRef,
};

/// `+ New X` creation row, styled like picker list rows.
pub fn sentinel_line(text: &str, is_selected: bool) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        jackin_tui::theme::DIM
    };
    Line::from(Span::styled(format!("{prefix}{text}"), style))
}

#[allow(unfulfilled_lint_expectations)]
#[expect(
    single_use_lifetimes,
    reason = "impl Trait cannot use anonymous lifetimes for borrowed ref DTOs on stable Rust"
)]
pub fn account_lines<'a>(
    accounts: impl IntoIterator<Item = OpPickerAccountRef<'a>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    accounts
        .into_iter()
        .enumerate()
        .map(|(i, account)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let label_style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(vec![
                Span::styled(format!("{prefix}{}", account.email), label_style),
                Span::raw("  "),
                Span::styled(format!("({})", account.url), jackin_tui::theme::DIM),
            ])
        })
        .collect()
}

#[allow(unfulfilled_lint_expectations)]
#[expect(
    single_use_lifetimes,
    reason = "impl Trait cannot use anonymous lifetimes for borrowed ref DTOs on stable Rust"
)]
pub fn vault_lines<'a>(
    vaults: impl IntoIterator<Item = OpPickerVaultRef<'a>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    vaults
        .into_iter()
        .enumerate()
        .map(|(i, vault)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(Span::styled(format!("{prefix}{}", vault.name), style))
        })
        .collect()
}

#[allow(unfulfilled_lint_expectations)]
#[expect(
    single_use_lifetimes,
    reason = "impl Trait cannot use anonymous lifetimes for borrowed ref DTOs on stable Rust"
)]
pub fn item_choice_lines<'a>(
    item_choices: impl IntoIterator<Item = Option<OpPickerItemRef<'a>>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    item_choices
        .into_iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_selected = Some(i) == selected;
            choice.map_or_else(
                || sentinel_line("+ New item", is_selected),
                |item| {
                    let prefix = if is_selected { "\u{25b8} " } else { "  " };
                    let title_style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(WHITE)
                    };
                    let mut spans = vec![
                        Span::styled(prefix, title_style),
                        Span::styled(item.name.to_owned(), title_style),
                    ];
                    if !item.subtitle.is_empty() {
                        let dim = jackin_tui::theme::DIM;
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
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    let choices: Vec<Option<String>> = choices.into_iter().collect();
    let sentinel_idx = choices.len();
    let mut lines: Vec<Line<'static>> = choices
        .into_iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            let label = choice.unwrap_or_else(|| "(root)".to_owned());
            Line::from(Span::styled(format!("{prefix}{label}"), style))
        })
        .collect();
    lines.push(sentinel_line(
        "+ New section",
        Some(sentinel_idx) == selected,
    ));
    lines
}

pub fn field_lines<'a>(
    rows: impl IntoIterator<Item = FieldDisplayRow>,
    fields: impl IntoIterator<Item = OpPickerFieldDisplayRef<'a>>,
    collapsed_sections: &HashSet<String>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    let fields: Vec<OpPickerFieldDisplayRef<'a>> = fields.into_iter().collect();
    let label_w = fields
        .iter()
        .map(|field| field_display_label(*field).chars().count())
        .max()
        .unwrap_or(0)
        .max(8);

    rows.into_iter()
        .enumerate()
        .map(|(row_i, row)| {
            let is_selected = Some(row_i) == selected;
            match row {
                FieldDisplayRow::SectionHeader { name, field_count } => {
                    section_header_line(&name, field_count, collapsed_sections, is_selected)
                }
                FieldDisplayRow::Field { field_idx } => {
                    let Some(field) = fields.get(field_idx).copied() else {
                        return Line::default();
                    };
                    field_line(field, label_w, is_selected)
                }
                FieldDisplayRow::NewFieldSentinel => sentinel_line("+ New field", is_selected),
                FieldDisplayRow::NewSectionSentinel => sentinel_line("+ New section", is_selected),
            }
        })
        .collect()
}

fn section_header_line(
    name: &str,
    field_count: usize,
    collapsed_sections: &HashSet<String>,
    is_selected: bool,
) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8}  " } else { "   " };
    let arrow = if collapsed_sections.contains(name) {
        "\u{25b6}"
    } else {
        "\u{25bc}"
    };
    let style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        jackin_tui::theme::DIM
    };
    let count_label = format!(
        "({} {})",
        field_count,
        if field_count == 1 { "field" } else { "fields" }
    );
    Line::from(vec![
        Span::styled(prefix, style),
        Span::styled(arrow, style),
        Span::styled(format!(" {name}  "), style),
        Span::styled(count_label, jackin_tui::theme::DIM),
    ])
}

fn field_line(
    field: OpPickerFieldDisplayRef<'_>,
    label_w: usize,
    is_selected: bool,
) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let label = field_display_label(field);
    let pad = label_w.saturating_sub(label.chars().count());
    let label_style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let annotation = if field.concealed {
        "(concealed)".to_owned()
    } else {
        format!("({})", field.field_type.to_lowercase())
    };
    Line::from(vec![
        Span::styled(format!("{prefix}{label}"), label_style),
        Span::raw(format!("{}  ", " ".repeat(pad))),
        Span::styled(annotation, jackin_tui::theme::DIM),
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
                jackin_tui::theme::BOLD_WHITE,
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Install: brew install 1password-cli (macOS)",
                jackin_tui::theme::GREEN,
            )),
            Line::from(Span::styled(
                "or visit 1password.com/downloads/command-line/",
                jackin_tui::theme::GREEN,
            )),
            Line::from(""),
            Line::from(Span::styled(
                "After install, run `op signin`, then press P to retry.",
                jackin_tui::theme::DIM,
            )),
        ],
        OpPickerFatalState::NotSignedIn => vec![
            Line::from(Span::styled(
                "1Password CLI is not signed in.",
                jackin_tui::theme::BOLD_WHITE,
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Run `op signin` in your shell, then retry.",
                jackin_tui::theme::GREEN,
            )),
            Line::from(""),
            Line::from(Span::styled(
                "jackin' uses your existing op session — there is no separate jackin' auth.",
                jackin_tui::theme::DIM,
            )),
        ],
        OpPickerFatalState::NoVaults => vec![
            Line::from(Span::styled(
                "No vaults available.",
                jackin_tui::theme::BOLD_WHITE,
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Check 1Password's app integration settings:",
                jackin_tui::theme::GREEN,
            )),
            Line::from(Span::styled(
                "Settings \u{2192} Developer \u{2192} CLI integration.",
                jackin_tui::theme::GREEN,
            )),
        ],
        OpPickerFatalState::GenericFatal { message } => {
            let truncated: String = message.chars().take(120).collect();
            vec![
                Line::from(Span::styled(
                    "1Password CLI error.",
                    jackin_tui::theme::BOLD_WHITE,
                )),
                Line::from(""),
                Line::from(Span::styled(truncated, jackin_tui::theme::DIM)),
            ]
        }
    }
}
