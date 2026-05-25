//! Render path for [`super::TokenStorePickerState`].

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::super::op_picker::OpPickerStage;
use super::super::op_picker::render::{
    breadcrumb_title, modal_block, render_fatal, render_filter_row,
};
use super::super::scrollable::render_selected_lines_in_area;
use super::super::text_input;
use super::super::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};
use super::{OpLoadState, OpPickerError, TokenStorePickerState, TokenStoreStage};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    frame.render_widget(ratatui::widgets::Clear, area);
    match &state.load_state {
        OpLoadState::Error(OpPickerError::Fatal(fatal)) => render_fatal(frame, area, fatal),
        OpLoadState::Loading { spinner_tick } => {
            render_loading(frame, area, state, *spinner_tick);
        }
        OpLoadState::Idle
        | OpLoadState::Ready
        | OpLoadState::Error(OpPickerError::Recoverable { .. }) => {
            render_pane(frame, area, state);
        }
    }
}

fn breadcrumb(state: &TokenStorePickerState) -> String {
    let multi = state.is_multi_account();
    let acct = state
        .selected_account
        .as_ref()
        .map(|a| a.email.as_str())
        .unwrap_or("");
    let vault_name = state
        .selected_vault
        .as_ref()
        .map(|v| v.name.as_str())
        .unwrap_or("");
    let item_name = state
        .selected_item
        .as_ref()
        .map(|i| i.name.as_str())
        .unwrap_or("");

    match state.stage {
        TokenStoreStage::Account => {
            breadcrumb_title(OpPickerStage::Account, multi, acct, vault_name, item_name)
        }
        TokenStoreStage::Vault => {
            breadcrumb_title(OpPickerStage::Vault, multi, acct, vault_name, item_name)
        }
        TokenStoreStage::ItemChoice => {
            breadcrumb_title(OpPickerStage::Item, multi, acct, vault_name, item_name)
        }
        TokenStoreStage::NewItemName => {
            let base = breadcrumb_title(OpPickerStage::Item, multi, acct, vault_name, item_name);
            format!("{base} \u{2192} new item")
        }
        TokenStoreStage::ExistingFieldChoice => {
            breadcrumb_title(OpPickerStage::Field, multi, acct, vault_name, item_name)
        }
        TokenStoreStage::FieldLabel => {
            if state.selected_item.is_some() {
                breadcrumb_title(OpPickerStage::Field, multi, acct, vault_name, item_name)
            } else {
                let base =
                    breadcrumb_title(OpPickerStage::Item, multi, acct, vault_name, item_name);
                format!("{base} \u{2192} new item")
            }
        }
    }
}

fn render_loading(frame: &mut Frame, area: Rect, state: &TokenStorePickerState, spinner_tick: u8) {
    let glyph = SPINNER_FRAMES[(spinner_tick as usize) % SPINNER_FRAMES.len()];
    let descriptor = match state.stage {
        TokenStoreStage::Account => "loading accounts\u{2026}".to_string(),
        TokenStoreStage::Vault => {
            let acct = state
                .selected_account
                .as_ref()
                .map(|a| a.email.as_str())
                .unwrap_or("");
            if state.is_multi_account() && !acct.is_empty() {
                format!("loading vaults from {acct}\u{2026}")
            } else {
                "loading vaults\u{2026}".to_string()
            }
        }
        TokenStoreStage::ItemChoice => {
            let vault_name = state
                .selected_vault
                .as_ref()
                .map(|v| v.name.as_str())
                .unwrap_or("");
            format!("loading items from {vault_name}\u{2026}")
        }
        TokenStoreStage::ExistingFieldChoice => {
            let item_name = state
                .selected_item
                .as_ref()
                .map(|i| i.name.as_str())
                .unwrap_or("");
            format!("loading {item_name}\u{2026}")
        }
        TokenStoreStage::NewItemName | TokenStoreStage::FieldLabel => "loading\u{2026}".to_string(),
    };
    let title = breadcrumb(state);
    let block = modal_block(format!("Token storage \u{2014} {title}"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let body = Line::from(vec![
        Span::styled(glyph.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
        Span::raw("  "),
        Span::styled(descriptor, Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(body).alignment(Alignment::Center), rows[1]);
}

fn render_pane(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let title = breadcrumb(state);
    let block = modal_block(format!("Token storage \u{2014} {title}"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match state.stage {
        TokenStoreStage::Account => render_account_list(frame, inner, state),
        TokenStoreStage::Vault => render_vault_list(frame, inner, state),
        TokenStoreStage::ItemChoice => render_item_choice(frame, inner, state),
        TokenStoreStage::NewItemName => {
            text_input::render(frame, inner, &state.item_name_input);
        }
        TokenStoreStage::ExistingFieldChoice => render_field_choice(frame, inner, state),
        TokenStoreStage::FieldLabel => {
            text_input::render(frame, inner, &state.field_label_input);
        }
    }
}

fn pane_layout(inner: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // filter row
            Constraint::Min(0),    // list
            Constraint::Length(1), // hint footer
        ])
        .split(inner);
    (chunks[0], chunks[1], chunks[2])
}

fn render_hint(frame: &mut Frame, area: Rect, hint: &str) {
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(PHOSPHOR_DIM)),
        area,
    );
}

fn render_account_list(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let (filter_area, list_area, hint_area) = pane_layout(area);
    render_filter_row(frame, filter_area, &state.filter_buf);

    let accounts = state.filtered_accounts();
    let selected = state.account_list_state.selected;
    let lines: Vec<Line> = accounts
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(vec![
                Span::styled(format!("{prefix}{}", a.email), style),
                Span::raw("  "),
                Span::styled(format!("({})", a.url), Style::default().fg(PHOSPHOR_DIM)),
            ])
        })
        .collect();
    render_selected_lines_in_area(frame, list_area, lines, selected);
    render_hint(
        frame,
        hint_area,
        "\u{2191}\u{2193} navigate  Enter \u{2014} select  R \u{2014} refresh  Esc \u{2014} cancel",
    );
}

fn render_vault_list(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let (filter_area, list_area, hint_area) = pane_layout(area);
    render_filter_row(frame, filter_area, &state.filter_buf);

    let vaults = state.filtered_vaults();
    let selected = state.vault_list_state.selected;
    let lines: Vec<Line> = vaults
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(Span::styled(format!("{prefix}{}", v.name), style))
        })
        .collect();
    render_selected_lines_in_area(frame, list_area, lines, selected);
    render_hint(
        frame,
        hint_area,
        "\u{2191}\u{2193} navigate  Enter \u{2014} select  R \u{2014} refresh  Esc \u{2014} back",
    );
}

fn render_item_choice(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let (filter_area, list_area, hint_area) = pane_layout(area);
    render_filter_row(frame, filter_area, &state.filter_buf);

    let choices = state.filtered_item_choices();
    let selected = state.item_list_state.selected;
    let lines: Vec<Line> = choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            match choice {
                None => {
                    let style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(PHOSPHOR_DIM)
                    };
                    Line::from(Span::styled(format!("{prefix}[ + New item ]"), style))
                }
                Some(item) => {
                    let title_style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(WHITE)
                    };
                    let mut spans = vec![
                        Span::styled(prefix.to_string(), title_style),
                        Span::styled(item.name.clone(), title_style),
                    ];
                    if !item.subtitle.is_empty() {
                        let dim = Style::default().fg(PHOSPHOR_DIM);
                        spans.push(Span::styled(" (".to_string(), dim));
                        spans.push(Span::styled(item.subtitle.clone(), dim));
                        spans.push(Span::styled(")".to_string(), dim));
                    }
                    Line::from(spans)
                }
            }
        })
        .collect();
    render_selected_lines_in_area(frame, list_area, lines, selected);
    render_hint(
        frame,
        hint_area,
        "\u{2191}\u{2193} navigate  Enter \u{2014} select  R \u{2014} refresh  Esc \u{2014} back",
    );
}

fn render_field_choice(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let (filter_area, list_area, hint_area) = pane_layout(area);
    render_filter_row(frame, filter_area, &state.filter_buf);

    let choices = state.filtered_field_choices();
    let selected = state.field_list_state.selected;
    let lines: Vec<Line> = choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            match choice {
                None => {
                    let style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(PHOSPHOR_DIM)
                    };
                    Line::from(Span::styled(format!("{prefix}[ + New field ]"), style))
                }
                Some(field) => {
                    let label_style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(WHITE)
                    };
                    let annotation = format!(
                        "({})",
                        if field.concealed {
                            "concealed".to_string()
                        } else {
                            field.field_type.to_lowercase()
                        }
                    );
                    Line::from(vec![
                        Span::styled(format!("{prefix}{}", field.label), label_style),
                        Span::raw("  "),
                        Span::styled(annotation, Style::default().fg(PHOSPHOR_DIM)),
                    ])
                }
            }
        })
        .collect();
    render_selected_lines_in_area(frame, list_area, lines, selected);
    render_hint(
        frame,
        hint_area,
        "\u{2191}\u{2193} navigate  Enter \u{2014} select  R \u{2014} refresh  Esc \u{2014} back",
    );
}
