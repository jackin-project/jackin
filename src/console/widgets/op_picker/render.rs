//! Render path for [`super::OpPickerState`].

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::operator_env::OpField;

use super::super::{PHOSPHOR_DIM, PHOSPHOR_GREEN, SPINNER_FRAMES, WHITE};
use super::{
    FieldDisplayRow, OpLoadState, OpPickerError, OpPickerFatalState, OpPickerStage, OpPickerState,
};
use jackin_tui::components::scrollable_panel::render_selected_lines_in_area;

pub fn render(frame: &mut Frame, area: Rect, state: &OpPickerState) {
    frame.render_widget(ratatui::widgets::Clear, area);
    match &state.load_state {
        OpLoadState::Error(OpPickerError::Fatal(fatal)) => render_fatal(frame, area, fatal),
        OpLoadState::Loading { spinner_tick } => render_loading(frame, area, state, *spinner_tick),
        OpLoadState::Idle
        | OpLoadState::Ready
        | OpLoadState::Error(OpPickerError::Recoverable { .. }) => {
            render_pane(frame, area, state);
        }
    }
}

/// Multi-account titles lead with the chosen account's email so the
/// operator can see which account they're drilling into; single-
/// account titles omit it (no ambiguity to resolve).
pub fn breadcrumb_title(
    stage: OpPickerStage,
    multi_account: bool,
    account_email: &str,
    vault_name: &str,
    item_name: &str,
) -> String {
    match stage {
        OpPickerStage::Account => "1Password".to_string(),
        OpPickerStage::Vault => {
            if multi_account {
                account_email.to_string()
            } else {
                "1Password".to_string()
            }
        }
        // Naming sub-stages render as a plain labelled input box (no
        // breadcrumb); this arm exists only for match exhaustiveness and
        // is reached for the Item list pane.
        OpPickerStage::Item
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name}")
            } else {
                vault_name.to_string()
            }
        }
        // Section stage sits between Item and Field; its breadcrumb shows
        // the chosen item (the section is the choice being made here).
        OpPickerStage::Section | OpPickerStage::Field => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name} \u{2192} {item_name}")
            } else {
                format!("{vault_name} \u{2192} {item_name}")
            }
        }
    }
}

pub fn modal_block<'a>(title: impl Into<String>) -> Block<'a> {
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

#[allow(clippy::too_many_lines)]
fn render_pane(frame: &mut Frame, area: Rect, state: &OpPickerState) {
    let multi_account = state.accounts.len() > 1;
    let account_email = state
        .selected_account
        .as_ref()
        .map_or("", |a| a.email.as_str());
    let v_name = state
        .selected_vault
        .as_ref()
        .map_or("", |v| v.name.as_str());
    let i_name = state.selected_item.as_ref().map_or("", |i| i.name.as_str());

    // Naming sub-stages are a plain labelled input box — the same shared
    // dialog every "type one value" prompt uses. No breadcrumb frame.
    if let Some(input) = state.naming_stage_input() {
        jackin_tui::components::text_input::render_text_input(frame, area, input);
        return;
    }

    let title = breadcrumb_title(state.stage, multi_account, account_email, v_name, i_name);
    let block = modal_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let banner_height: u16 = match &state.load_state {
        OpLoadState::Error(OpPickerError::Recoverable { .. }) => 2,
        _ => 0,
    };

    let constraints = [
        Constraint::Length(banner_height), // optional banner
        Constraint::Length(1),             // filter row
        Constraint::Length(1),             // spacer
        Constraint::Min(1),                // list
    ];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    if banner_height > 0
        && let OpLoadState::Error(OpPickerError::Recoverable { message }) = &state.load_state
    {
        let truncated: String = message.chars().take(120).collect();
        let line = Line::from(vec![
            Span::styled(
                "Error: ",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(truncated, Style::default().fg(PHOSPHOR_DIM)),
        ]);
        frame.render_widget(Paragraph::new(line), rows[0]);
    }

    jackin_tui::components::render_filter_input(frame, rows[1], &state.filter_buf);

    // List rows. Naming sub-stages are handled above and never reach here.
    let list_lines: Vec<Line<'static>> = match state.stage {
        OpPickerStage::Account => render_account_lines(state),
        OpPickerStage::Vault => render_vault_lines(state),
        OpPickerStage::Item => render_item_lines(state),
        OpPickerStage::Section => render_section_lines(state),
        OpPickerStage::Field => render_field_lines(state),
        OpPickerStage::NewItemName | OpPickerStage::FieldLabel | OpPickerStage::NewSectionName => {
            Vec::new()
        }
    };
    if list_lines.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "(no matches)",
            Style::default().fg(PHOSPHOR_DIM),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(para, rows[3]);
    } else {
        let selected = match state.stage {
            OpPickerStage::Account => state.account_list_state.selected,
            OpPickerStage::Vault => state.vault_list_state.selected,
            OpPickerStage::Item => state.item_list_state.selected,
            OpPickerStage::Section => state.section_list_state.selected,
            OpPickerStage::Field => state.field_list_state.selected,
            OpPickerStage::NewItemName
            | OpPickerStage::FieldLabel
            | OpPickerStage::NewSectionName => None,
        };
        render_selected_lines_in_area(frame, rows[3], list_lines, selected);
    }
}

fn render_account_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    // Server order — do not alphabetize. `op` lists accounts in
    // sign-in order; preserve it.
    let visible = state.filtered_accounts();
    let selected = state.account_list_state.selected;
    visible
        .into_iter()
        .enumerate()
        .map(|(i, a)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let label_style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            // Pre-v2 op may omit email/url; render as empty rather
            // than panicking.
            Line::from(vec![
                Span::styled(format!("{prefix}{}", a.email), label_style),
                Span::raw("  "),
                Span::styled(format!("({})", a.url), Style::default().fg(PHOSPHOR_DIM)),
            ])
        })
        .collect()
}

fn render_vault_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    let visible = state.filtered_vaults();
    let selected = state.vault_list_state.selected;
    visible
        .into_iter()
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
        .collect()
}

fn render_item_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    let selected = state.item_list_state.selected;
    // Use the choice list so the trailing `+ New item` sentinel (Create
    // mode) is rendered and selectable at the same index the handler uses.
    state
        .filtered_item_choices()
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
                    // Subtitle stays dim even on the focused row so the
                    // title remains the primary anchor. Empty subtitle →
                    // no parens.
                    let mut spans = vec![
                        Span::styled(prefix, title_style),
                        Span::styled(item.name.clone(), title_style),
                    ];
                    if !item.subtitle.is_empty() {
                        let dim = Style::default().fg(PHOSPHOR_DIM);
                        spans.push(Span::styled(" (", dim));
                        spans.push(Span::styled(item.subtitle.clone(), dim));
                        spans.push(Span::styled(")", dim));
                    }
                    Line::from(spans)
                },
            )
        })
        .collect()
}

/// Section stage (Create mode): `(root)`, each named section, then a
/// trailing `+ New section` sentinel — same selected/prefix styling as the
/// vault/item lists.
fn render_section_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    let selected = state.section_list_state.selected;
    let choices = state.section_choices();
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
            let label = choice.unwrap_or_else(|| "(root)".to_string());
            Line::from(Span::styled(format!("{prefix}{label}"), style))
        })
        .collect();
    lines.push(sentinel_line(
        "+ New section",
        Some(sentinel_idx) == selected,
    ));
    lines
}

/// `+ New X` creation row, styled like the existing list rows
/// (`PHOSPHOR_GREEN` + BOLD selected, `PHOSPHOR_DIM` otherwise).
fn sentinel_line(text: &str, is_selected: bool) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_DIM)
    };
    Line::from(Span::styled(format!("{prefix}{text}"), style))
}

fn render_field_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    let visible: Vec<&OpField> = state.filtered_fields();
    let selected = state.field_list_state.selected;
    let label_w = visible
        .iter()
        .map(|f| display_label(f).chars().count())
        .max()
        .unwrap_or(0)
        .max(8);

    state
        .build_field_display_rows()
        .into_iter()
        .enumerate()
        .map(|(row_i, row)| {
            let is_selected = Some(row_i) == selected;
            match row {
                FieldDisplayRow::SectionHeader { name, field_count } => {
                    let prefix = if is_selected { "\u{25b8}  " } else { "   " };
                    let arrow = if state.collapsed_sections.contains(&name) {
                        "\u{25b6}"
                    } else {
                        "\u{25bc}"
                    };
                    let style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(PHOSPHOR_DIM)
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
                        Span::styled(count_label, Style::default().fg(PHOSPHOR_DIM)),
                    ])
                }
                FieldDisplayRow::Field { field_idx } => {
                    // Defensive: a row/field-list desync must not panic the
                    // whole TUI mid-render. Matches the `.get()` guard in
                    // `handle_field_key`; an empty line is dropped on the
                    // next rebuild.
                    let Some(f) = visible.get(field_idx) else {
                        return Line::default();
                    };
                    let prefix = if is_selected { "\u{25b8} " } else { "  " };
                    let label = display_label(f);
                    let pad = label_w.saturating_sub(label.chars().count());
                    let label_style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(WHITE)
                    };
                    let annotation = if f.concealed {
                        "(concealed)".to_string()
                    } else {
                        format!("({})", f.field_type.to_lowercase())
                    };
                    Line::from(vec![
                        Span::styled(format!("{prefix}{label}"), label_style),
                        Span::raw(format!("{}  ", " ".repeat(pad))),
                        Span::styled(annotation, Style::default().fg(PHOSPHOR_DIM)),
                    ])
                }
                FieldDisplayRow::NewFieldSentinel => sentinel_line("+ New field", is_selected),
                FieldDisplayRow::NewSectionSentinel => sentinel_line("+ New section", is_selected),
            }
        })
        .collect()
}

fn display_label(f: &OpField) -> String {
    if f.label.is_empty() {
        f.id.clone()
    } else {
        f.label.clone()
    }
}

fn render_loading(frame: &mut Frame, area: Rect, state: &OpPickerState, tick: u8) {
    // Field-stage exception: the title shows the parent (account →
    // vault) so the body can carry `loading <item>…` with the
    // disambiguating subtitle. Title = where you are, body = what
    // you're descending into.
    let multi_account = state.accounts.len() > 1;
    let account_email = state
        .selected_account
        .as_ref()
        .map_or("", |a| a.email.as_str());
    let v_name = state
        .selected_vault
        .as_ref()
        .map_or("", |v| v.name.as_str());
    let i_name = state.selected_item.as_ref().map_or("", |i| i.name.as_str());
    let i_subtitle = state
        .selected_item
        .as_ref()
        .map_or("", |i| i.subtitle.as_str());
    let title_stage = if matches!(state.stage, OpPickerStage::Field) {
        OpPickerStage::Item
    } else {
        state.stage
    };
    let title = breadcrumb_title(title_stage, multi_account, account_email, v_name, i_name);
    let block = modal_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let glyph = SPINNER_FRAMES[(tick as usize) % SPINNER_FRAMES.len()];
    let descriptor = match state.stage {
        OpPickerStage::Account => "loading accounts\u{2026}".to_string(),
        OpPickerStage::Vault => {
            if multi_account && !account_email.is_empty() {
                format!("loading vaults from {account_email}\u{2026}")
            } else {
                "loading vaults\u{2026}".to_string()
            }
        }
        OpPickerStage::Item => {
            format!("loading items from {v_name}\u{2026}")
        }
        OpPickerStage::Field => {
            if i_subtitle.is_empty() {
                format!("loading {i_name}\u{2026}")
            } else {
                format!("loading {i_name} ({i_subtitle})\u{2026}")
            }
        }
        // The Section stage only becomes current after the field load
        // completes; a lingering Loading state here is the field load.
        // Naming stages never load. Both fall back to a neutral descriptor.
        OpPickerStage::Section
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => "loading\u{2026}".to_string(),
    };

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

pub fn render_fatal(frame: &mut Frame, area: Rect, fatal: &OpPickerFatalState) {
    let block = modal_block("1Password");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let body_lines = match fatal {
        OpPickerFatalState::NotInstalled => vec![
            Line::from(Span::styled(
                "1Password CLI not found.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Install: brew install 1password-cli (macOS)",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(Span::styled(
                "or visit 1password.com/downloads/command-line/",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "After install, run `op signin`, then press P to retry.",
                Style::default().fg(PHOSPHOR_DIM),
            )),
        ],
        OpPickerFatalState::NotSignedIn => vec![
            Line::from(Span::styled(
                "1Password CLI is not signed in.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Run `op signin` in your shell, then retry.",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "jackin' uses your existing op session — there is no separate jackin' auth.",
                Style::default().fg(PHOSPHOR_DIM),
            )),
        ],
        OpPickerFatalState::NoVaults => vec![
            Line::from(Span::styled(
                "No vaults available.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Check 1Password's app integration settings:",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(Span::styled(
                "Settings \u{2192} Developer \u{2192} CLI integration.",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
        ],
        OpPickerFatalState::GenericFatal { message } => {
            let truncated: String = message.chars().take(120).collect();
            vec![
                Line::from(Span::styled(
                    "1Password CLI error.",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(truncated, Style::default().fg(PHOSPHOR_DIM))),
            ]
        }
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(body_lines).alignment(Alignment::Center),
        rows[1],
    );
}

#[cfg(test)]
mod tests {
    use super::{OpPickerStage, breadcrumb_title};

    // ── Breadcrumb formatting ─────────────────────────────────────────

    #[test]
    fn breadcrumb_omits_pane_type_suffix_multi_account() {
        // Multi-account: <email> for vault, <email> → <vault> for items,
        // <email> → <vault> → <item> for fields. No trailing pane type.
        let title = breadcrumb_title(
            OpPickerStage::Vault,
            true,
            "alice@example.com",
            "ignored",
            "ignored",
        );
        assert_eq!(title, "alice@example.com");
        assert!(!title.contains("Vaults"), "no `Vaults` suffix: {title}");

        let title = breadcrumb_title(
            OpPickerStage::Item,
            true,
            "alice@example.com",
            "Personal",
            "",
        );
        assert_eq!(title, "alice@example.com \u{2192} Personal");
        assert!(!title.contains("Items"));

        let title = breadcrumb_title(
            OpPickerStage::Field,
            true,
            "alice@example.com",
            "Personal",
            "API Keys",
        );
        assert_eq!(
            title,
            "alice@example.com \u{2192} Personal \u{2192} API Keys"
        );
        assert!(!title.contains("Fields"));
    }

    #[test]
    fn breadcrumb_single_account_uses_brand_or_bare_context() {
        // Single-account: Vault pane shows the bare brand; Item/Field
        // show the vault/item context without a leading email.
        let v = breadcrumb_title(OpPickerStage::Vault, false, "", "Personal", "");
        assert_eq!(v, "1Password");

        let i = breadcrumb_title(OpPickerStage::Item, false, "", "Personal", "API Keys");
        assert_eq!(i, "Personal");

        let f = breadcrumb_title(OpPickerStage::Field, false, "", "Personal", "API Keys");
        assert_eq!(f, "Personal \u{2192} API Keys");
    }

    #[test]
    fn breadcrumb_account_pane_is_bare_brand() {
        // Account pane never has an email prefix (it lists accounts).
        let title = breadcrumb_title(OpPickerStage::Account, true, "ignored", "", "");
        assert_eq!(title, "1Password");
    }

    // ── Loading-panel breadcrumb ──────────────────────────────────────

    /// During the 1-3s `op` subprocess that loads items, the loading
    /// panel must render the full Account → Vault breadcrumb in its title
    /// — not the bare brand `1Password`. This was the operator complaint
    /// that motivated the request-time stage advance: previously the
    /// title only showed the breadcrumb after the load completed.
    #[test]
    fn loading_panel_title_during_item_load_shows_breadcrumb() {
        use super::super::{OpAccount, OpLoadState, OpPickerState, OpVault};
        use ratatui::{Terminal, backend::TestBackend};

        let mut state = OpPickerState::default();
        state.accounts = vec![
            OpAccount {
                id: "a1".into(),
                email: "alice@example.com".into(),
                url: "alice.1password.com".into(),
            },
            OpAccount {
                id: "a2".into(),
                email: "bob@example.com".into(),
                url: "bob.1password.com".into(),
            },
        ];
        state.selected_account = Some(state.accounts[0].clone());
        state.selected_vault = Some(OpVault {
            id: "v-personal".into(),
            name: "Personal".into(),
        });
        state.stage = OpPickerStage::Item;
        state.load_state = OpLoadState::Loading { spinner_tick: 0 };

        let backend = TestBackend::new(80, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = f.area();
            super::render(f, area, &state);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut dump = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                dump.push_str(buf[(x, y)].symbol());
            }
            dump.push('\n');
        }

        // Title bar carries the full Account → Vault breadcrumb.
        assert!(
            dump.contains("alice@example.com"),
            "loading panel title must show the account email; dump:\n{dump}"
        );
        assert!(
            dump.contains("Personal"),
            "loading panel title must show the vault name; dump:\n{dump}"
        );
        assert!(
            dump.contains('\u{2192}'),
            "loading panel title must include the breadcrumb arrow `→`; dump:\n{dump}"
        );

        // Loading body message is pane-specific: "loading items from
        // <vault>". The previous "loading items from IT…" wording is
        // preserved.
        assert!(
            dump.contains("loading items from Personal"),
            "loading body must read `loading items from <vault>`; dump:\n{dump}"
        );
    }

    /// Field-load is the one stage where the title shows the PARENT
    /// context rather than the full path: the item being descended into
    /// belongs in the body (where it can carry its disambiguating
    /// subtitle). Principle: title = where you are, body = what you're
    /// descending into.
    #[test]
    fn picker_field_load_title_shows_parent_and_body_includes_subtitle() {
        use super::super::{OpAccount, OpLoadState, OpPickerState, OpVault};
        use crate::operator_env::OpItem;
        use ratatui::{Terminal, backend::TestBackend};

        let mut state = OpPickerState::default();
        state.accounts = vec![
            OpAccount {
                id: "a1".into(),
                email: "alexey@zhokhov.com".into(),
                url: "z.1password.com".into(),
            },
            OpAccount {
                id: "a2".into(),
                email: "alexey@chainargos.com".into(),
                url: "c.1password.com".into(),
            },
        ];
        state.selected_account = Some(state.accounts[1].clone());
        state.selected_vault = Some(OpVault {
            id: "v-chainargos".into(),
            name: "ChainArgos".into(),
        });
        state.selected_item = Some(OpItem {
            id: "i-redshift".into(),
            name: "ChainArgos Redshift".into(),
            subtitle: "donbeave".into(),
        });
        state.stage = OpPickerStage::Field;
        state.load_state = OpLoadState::Loading { spinner_tick: 0 };

        let backend = TestBackend::new(80, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = f.area();
            super::render(f, area, &state);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut dump = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                dump.push_str(buf[(x, y)].symbol());
            }
            dump.push('\n');
        }

        // Title bar shows the parent context ONLY (account → vault).
        // Pull out just the top border row to check title content
        // without false matches against the body line.
        let top_row: String = (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>();
        assert!(
            top_row.contains("alexey@chainargos.com"),
            "field-load title must show the account email; top row:\n{top_row}"
        );
        assert!(
            top_row.contains("ChainArgos"),
            "field-load title must show the vault name; top row:\n{top_row}"
        );
        assert!(
            !top_row.contains("Redshift"),
            "field-load title must NOT include the item name (it belongs in the body); \
             top row:\n{top_row}"
        );

        // Body names the item being descended into, with subtitle.
        assert!(
            dump.contains("loading ChainArgos Redshift (donbeave)"),
            "field-load body must read `loading <item> (<subtitle>)…`; dump:\n{dump}"
        );
        // The old "loading fields from <item>…" wording must be gone.
        assert!(
            !dump.contains("loading fields from"),
            "field-load body must drop the redundant `fields from` prefix; dump:\n{dump}"
        );
    }

    /// Field-load body without a subtitle: just `loading <item>…`,
    /// no parens.
    #[test]
    fn picker_field_load_body_no_subtitle() {
        use super::super::{OpAccount, OpLoadState, OpPickerState, OpVault};
        use crate::operator_env::OpItem;
        use ratatui::{Terminal, backend::TestBackend};

        let mut state = OpPickerState::default();
        state.accounts = vec![OpAccount {
            id: "a1".into(),
            email: "single@example.com".into(),
            url: "x.1password.com".into(),
        }];
        state.selected_account = Some(state.accounts[0].clone());
        state.selected_vault = Some(OpVault {
            id: "v".into(),
            name: "Personal".into(),
        });
        state.selected_item = Some(OpItem {
            id: "i-note".into(),
            name: "Standalone Note".into(),
            subtitle: String::new(),
        });
        state.stage = OpPickerStage::Field;
        state.load_state = OpLoadState::Loading { spinner_tick: 0 };

        let backend = TestBackend::new(80, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = f.area();
            super::render(f, area, &state);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut dump = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                dump.push_str(buf[(x, y)].symbol());
            }
            dump.push('\n');
        }

        assert!(
            dump.contains("loading Standalone Note"),
            "no-subtitle field-load body must read `loading <item>…`; dump:\n{dump}"
        );
        assert!(
            !dump.contains("loading Standalone Note ("),
            "no-subtitle field-load must not render an empty `()` segment; \
             dump:\n{dump}"
        );
    }
}
