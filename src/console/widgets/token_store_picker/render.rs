//! Render path for [`super::TokenStorePickerState`].

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::super::scrollable::render_selected_lines_in_area;
use super::super::text_input;
use super::super::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};
use super::{
    OpLoadState, OpPickerError, OpPickerFatalState, TokenStorePickerState, TokenStoreStage,
};

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
    let vault = state
        .selected_vault
        .as_ref()
        .map(|v| v.name.as_str())
        .unwrap_or("");

    match state.stage {
        TokenStoreStage::Account => "1Password · choose account".to_string(),
        TokenStoreStage::Vault => {
            if multi && !acct.is_empty() {
                format!("{acct}")
            } else {
                "1Password · choose vault".to_string()
            }
        }
        TokenStoreStage::ItemChoice => {
            if multi && !acct.is_empty() {
                format!("{acct} \u{2192} {vault}")
            } else {
                vault.to_string()
            }
        }
        TokenStoreStage::NewItemName => {
            if multi && !acct.is_empty() {
                format!("{acct} \u{2192} {vault} \u{2192} new item")
            } else if !vault.is_empty() {
                format!("{vault} \u{2192} new item")
            } else {
                "new item".to_string()
            }
        }
        TokenStoreStage::ExistingFieldChoice | TokenStoreStage::FieldLabel => {
            let item_name = state
                .selected_item
                .as_ref()
                .map(|i| i.name.as_str())
                .unwrap_or("");
            if !item_name.is_empty() {
                if multi && !acct.is_empty() {
                    format!("{acct} \u{2192} {vault} \u{2192} {item_name}")
                } else if !vault.is_empty() {
                    format!("{vault} \u{2192} {item_name}")
                } else {
                    item_name.to_string()
                }
            } else if multi && !acct.is_empty() {
                format!("{acct} \u{2192} {vault} \u{2192} new item")
            } else if !vault.is_empty() {
                format!("{vault} \u{2192} new item")
            } else {
                "new item".to_string()
            }
        }
    }
}

fn modal_block(title: impl Into<String>) -> Block<'static> {
    let title_text: String = title.into();
    let title_span = Span::styled(
        format!(" {title_text} "),
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .title(title_span)
}

fn render_fatal(frame: &mut Frame, area: Rect, fatal: &OpPickerFatalState) {
    let (heading, body) = match fatal {
        OpPickerFatalState::NotInstalled => (
            "1Password CLI not found",
            "Install `op` and ensure it is on PATH.\nhttps://developer.1password.com/docs/cli/get-started/",
        ),
        OpPickerFatalState::NotSignedIn => (
            "Not signed in to 1Password",
            "Run `op signin` in your terminal, then retry.",
        ),
        OpPickerFatalState::NoVaults => (
            "No vaults found",
            "The selected account has no vaults accessible to this session.",
        ),
        OpPickerFatalState::GenericFatal { message } => ("1Password error", message.as_str()),
    };

    let block = modal_block("Token storage — error");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(heading).style(Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(body)
            .style(Style::default().fg(PHOSPHOR_DIM))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        chunks[1],
    );
}

fn render_loading(frame: &mut Frame, area: Rect, state: &TokenStorePickerState, spinner_tick: u8) {
    let spinner = SPINNER_FRAMES[(spinner_tick as usize) % SPINNER_FRAMES.len()];
    let loading_label = match state.stage {
        TokenStoreStage::Account => "Loading accounts",
        TokenStoreStage::Vault => "Loading vaults",
        TokenStoreStage::ItemChoice => "Loading items",
        TokenStoreStage::ExistingFieldChoice => "Loading fields",
        TokenStoreStage::NewItemName | TokenStoreStage::FieldLabel => "Loading",
    };
    let title = breadcrumb(state);
    let block = modal_block(format!("Token storage — {title}"));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(format!("{spinner} {loading_label}…"))
            .style(Style::default().fg(PHOSPHOR_DIM)),
        inner,
    );
}

fn render_pane(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let title = breadcrumb(state);
    let block = modal_block(format!("Token storage — {title}"));
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

fn render_hint_footer(frame: &mut Frame, area: Rect, hint: &str) {
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(PHOSPHOR_DIM)),
        area,
    );
}

fn render_filter_row(frame: &mut Frame, area: Rect, filter: &str) {
    let text = if filter.is_empty() {
        Span::styled("Type to filter…", Style::default().fg(PHOSPHOR_DARK))
    } else {
        Span::styled(filter, Style::default().fg(PHOSPHOR_DIM))
    };
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

fn render_account_list(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_filter_row(frame, chunks[0], &state.filter_buf);

    let accounts = state.filtered_accounts();
    let selected = state.account_list_state.selected.unwrap_or(0);
    let lines: Vec<Line> = accounts
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let style = if i == selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(format!("  {} ({})", a.email, a.url), style))
        })
        .collect();
    render_selected_lines_in_area(frame, chunks[1], lines, Some(selected));
    render_hint_footer(
        frame,
        chunks[2],
        "↑↓ navigate  Enter — select  Esc — cancel",
    );
}

fn render_vault_list(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_filter_row(frame, chunks[0], &state.filter_buf);

    let vaults = state.filtered_vaults();
    let selected = state.vault_list_state.selected.unwrap_or(0);
    let lines: Vec<Line> = vaults
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let style = if i == selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(format!("  {}", v.name), style))
        })
        .collect();
    render_selected_lines_in_area(frame, chunks[1], lines, Some(selected));
    render_hint_footer(frame, chunks[2], "↑↓ navigate  Enter — select  Esc — back");
}

fn render_item_choice(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_filter_row(frame, chunks[0], &state.filter_buf);

    let choices = state.filtered_item_choices();
    let selected = state.item_list_state.selected.unwrap_or(0);
    let lines: Vec<Line> = choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_sel = i == selected;
            match choice {
                None => {
                    let style = if is_sel {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(PHOSPHOR_DIM)
                    };
                    Line::from(Span::styled("  [ + New item ]", style))
                }
                Some(item) => {
                    let style = if is_sel {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let label = if item.subtitle.is_empty() {
                        format!("  {}", item.name)
                    } else {
                        format!("  {}  ({})", item.name, item.subtitle)
                    };
                    Line::from(Span::styled(label, style))
                }
            }
        })
        .collect();
    render_selected_lines_in_area(frame, chunks[1], lines, Some(selected));
    render_hint_footer(
        frame,
        chunks[2],
        "↑↓ navigate  Enter — select  Esc — back to vault",
    );
}

fn render_field_choice(frame: &mut Frame, area: Rect, state: &TokenStorePickerState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_filter_row(frame, chunks[0], &state.filter_buf);

    let choices = state.filtered_field_choices();
    let selected = state.field_list_state.selected.unwrap_or(0);
    let lines: Vec<Line> = choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_sel = i == selected;
            match choice {
                None => {
                    let style = if is_sel {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(PHOSPHOR_DIM)
                    };
                    Line::from(Span::styled("  [ + New field ]", style))
                }
                Some(field) => {
                    let style = if is_sel {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    Line::from(Span::styled(format!("  {}", field.label), style))
                }
            }
        })
        .collect();
    render_selected_lines_in_area(frame, chunks[1], lines, Some(selected));
    render_hint_footer(
        frame,
        chunks[2],
        "↑↓ navigate  Enter — select  Esc — back to item",
    );
}
