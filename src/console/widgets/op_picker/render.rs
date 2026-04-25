//! Render path for [`super::OpPickerState`].
//!
//! Single entry point: [`render`] dispatches on `state.load_state` and
//! `state.stage` to draw one of:
//!   - a fatal-state instructional panel (`NotInstalled` /
//!     `NotSignedIn` / `NoVaults` / `GenericFatal`),
//!   - a Braille-spinner loading panel (`Loading`),
//!   - a pane (`Vault` / `Item` / `Field`) when `Ready` (or when the
//!     load finished with a recoverable error — the banner shows above
//!     the list).

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::operator_env::OpField;

use super::{OpLoadState, OpPickerError, OpPickerFatalState, OpPickerStage, OpPickerState};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

/// Braille spinner glyphs cycled by the `spinner_tick` field on
/// `OpLoadState::Loading`. Ten frames; advance one per render.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

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

/// Compute the modal-block title for the current pane.
///
/// Breadcrumbs deliberately omit a trailing `Vaults` / `Items` /
/// `Fields` suffix — the pane content makes the type self-evident so
/// the suffix would just add noise.
///
/// - Single-account setups don't have an `<email>` prefix to surface
///   (the operator only ever sees one account), so the Vault pane
///   shows the bare brand `1Password` and the deeper panes show the
///   vault/item context without a leading email.
/// - Multi-account setups always lead with the chosen account's email
///   so the operator can tell at a glance which account they're
///   drilling into.
fn breadcrumb_title(
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
        OpPickerStage::Item => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name}")
            } else {
                vault_name.to_string()
            }
        }
        OpPickerStage::Field => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name} \u{2192} {item_name}")
            } else {
                format!("{vault_name} \u{2192} {item_name}")
            }
        }
    }
}

/// Compute the scroll offset for the list viewport so the selected
/// row stays visible.
///
/// - If the entire list fits in `height`, the offset is `0` (no scroll).
/// - If `selected < height`, the offset is `0` so the head of the list
///   stays anchored at the top until the cursor moves below the window.
/// - Otherwise, anchor the selected row at the bottom of the visible
///   window (`offset = selected - height + 1`), clamped to
///   `total - height` so we don't scroll past the end.
///
/// Stateless — recomputed every frame. The earlier picker had no
/// viewport math at all, so vaults / items beyond the modal's height
/// were unreachable; this gives the operator predictable
/// "cursor-follows" scrolling without a separate `offset` field on
/// each pane's `ListState`.
fn viewport_offset(selected: usize, height: usize, total: usize) -> usize {
    if height == 0 || total <= height {
        return 0;
    }
    if selected < height {
        return 0;
    }
    selected.saturating_sub(height - 1).min(total - height)
}

fn modal_block<'a>(title: impl Into<String>) -> Block<'a> {
    let title_text: String = title.into();
    let title_span = Span::styled(
        format!(" {title_text} "),
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title_span)
}

/// Footer line — canonical key/text/sep styling from the rest of the
/// modal palette. `pairs` is a sequence of `(key, label)` chunks; the
/// renderer interleaves `·` separators between them.
fn footer_line(pairs: &[(&str, &str)]) -> Line<'static> {
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (k, t)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" \u{b7} ", sep_style));
        }
        spans.push(Span::styled((*k).to_string(), key_style));
        spans.push(Span::styled(format!(" {t}"), text_style));
    }
    Line::from(spans)
}

/// Render the active drill-down pane (and any recoverable error
/// banner). Title carries the breadcrumb.
#[allow(clippy::too_many_lines)]
fn render_pane(frame: &mut Frame, area: Rect, state: &OpPickerState) {
    // Multi-account: prepend the chosen account's email to vault/item/
    // field breadcrumbs so the operator can always see which account
    // they're drilling into. Single-account: same compact title as
    // before (no per-pane account context to surface).
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
    let title = breadcrumb_title(state.stage, multi_account, account_email, v_name, i_name);
    let block = modal_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Reserve a row for the recoverable banner when present; otherwise
    // the layout is the canonical "filter / spacer / list / spacer /
    // hint" stack.
    let banner_height: u16 = match &state.load_state {
        OpLoadState::Error(OpPickerError::Recoverable { .. }) => 2,
        _ => 0,
    };

    let constraints = vec![
        Constraint::Length(banner_height), // optional banner
        Constraint::Length(1),             // filter row
        Constraint::Length(1),             // spacer
        Constraint::Min(1),                // list
        Constraint::Length(1),             // spacer
        Constraint::Length(1),             // footer
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

    // Filter row: `Filter: <buf>█` — placeholder dotted underline when
    // empty, cursor block at the end when populated.
    let filter_line = if state.filter_buf.is_empty() {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(PHOSPHOR_DIM)),
            Span::styled("\u{2591}".repeat(20), Style::default().fg(PHOSPHOR_DARK)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(PHOSPHOR_DIM)),
            Span::styled(state.filter_buf.clone(), Style::default().fg(WHITE)),
            Span::styled(
                "\u{2588}",
                Style::default()
                    .fg(WHITE)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(filter_line), rows[1]);

    // List rows.
    let list_lines: Vec<Line<'static>> = match state.stage {
        OpPickerStage::Account => render_account_lines(state),
        OpPickerStage::Vault => render_vault_lines(state),
        OpPickerStage::Item => render_item_lines(state),
        OpPickerStage::Field => render_field_lines(state),
    };
    let list_para = if list_lines.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "(no matches)",
            Style::default().fg(PHOSPHOR_DIM),
        )))
        .alignment(Alignment::Center)
    } else {
        // Cursor-tracking viewport: each render re-computes the offset
        // from the selected index so the cursor stays visible whatever
        // the operator's last navigation was. The earlier picker
        // rendered the full `list_lines` as a `Paragraph` with no
        // viewport math at all — pressing `↓` past the visible window
        // moved `*_list_state.selected` but the rendered area didn't
        // scroll, so vaults / items below the modal's height became
        // unreachable. The viewport math here mirrors the canonical
        // pattern: anchor the cursor at the bottom of the window once
        // the selected row falls below the initial viewport, clamped
        // so the last row sits on the bottom edge.
        let selected = match state.stage {
            OpPickerStage::Account => state.account_list_state.selected,
            OpPickerStage::Vault => state.vault_list_state.selected,
            OpPickerStage::Item => state.item_list_state.selected,
            OpPickerStage::Field => state.field_list_state.selected,
        };
        let height = rows[3].height as usize;
        let total = list_lines.len();
        let offset = viewport_offset(selected.unwrap_or(0), height, total);
        let take = height.min(total.saturating_sub(offset));
        let visible: Vec<Line<'static>> = list_lines.into_iter().skip(offset).take(take).collect();
        Paragraph::new(visible)
    };
    frame.render_widget(list_para, rows[3]);

    // Footer hint per pane. The Vault pane's Esc semantics depend on
    // whether an Account pane is in play: with ≥2 accounts, Esc returns
    // to that pane; otherwise it cancels the picker as before.
    let pairs: Vec<(&str, &str)> = match state.stage {
        OpPickerStage::Account => vec![
            ("\u{2191}\u{2193}", "navigate"),
            ("type", "filter"),
            ("Enter", "select"),
            ("r", "refresh"),
            ("Esc", "cancel"),
        ],
        OpPickerStage::Vault => {
            let esc_label = if multi_account {
                "back to accounts"
            } else {
                "cancel"
            };
            vec![
                ("\u{2191}\u{2193}", "navigate"),
                ("type", "filter"),
                ("Enter", "select"),
                ("r", "refresh"),
                ("Esc", esc_label),
            ]
        }
        OpPickerStage::Item => vec![
            ("\u{2191}\u{2193}", "navigate"),
            ("Backspace", "clear filter"),
            ("Enter", "select"),
            ("r", "refresh"),
            ("Esc", "back to vaults"),
        ],
        OpPickerStage::Field => vec![
            ("\u{2191}\u{2193}", "navigate"),
            ("type", "filter"),
            ("Enter", "select"),
            ("r", "refresh"),
            ("Esc", "back"),
        ],
    };
    frame.render_widget(
        Paragraph::new(footer_line(&pairs)).alignment(Alignment::Center),
        rows[5],
    );
}

fn render_account_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    // Server order — do not alphabetize. `op` returns accounts in the
    // order the operator signed them in; preserve that visible ordering
    // so the picker doesn't surprise long-time multi-account users.
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
            // Display: `<email>  (<url>)`. Empty values render as empty
            // strings rather than panicking — `op` versions older than
            // the v2 schema may not include `email`/`url`.
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
    let visible = state.filtered_items();
    let selected = state.item_list_state.selected;
    visible
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(Span::styled(format!("{prefix}{}", item.name), style))
        })
        .collect()
}

fn render_field_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    let visible: Vec<&OpField> = state.filtered_fields();
    let selected = state.field_list_state.selected;
    // Pad labels to a consistent width so the type annotation aligns.
    let label_w = visible
        .iter()
        .map(|f| display_label(f).chars().count())
        .max()
        .unwrap_or(0)
        .max(8);
    visible
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            let is_selected = Some(i) == selected;
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
            let annotation = format!(
                "({})",
                if f.concealed {
                    "concealed".to_string()
                } else {
                    f.field_type.to_lowercase()
                }
            );
            Line::from(vec![
                Span::styled(format!("{prefix}{label}"), label_style),
                Span::raw(format!("{}  ", " ".repeat(pad))),
                Span::styled(annotation, Style::default().fg(PHOSPHOR_DIM)),
            ])
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
    let block = modal_block("1Password");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let glyph = SPINNER_FRAMES[(tick as usize) % SPINNER_FRAMES.len()];
    let descriptor = match state.stage {
        // Account-stage loading shouldn't normally be visible —
        // `account_list` is synchronous in the constructor — but render
        // a sensible string for completeness.
        OpPickerStage::Account => "loading accounts\u{2026}".to_string(),
        OpPickerStage::Vault => "loading vaults\u{2026}".to_string(),
        OpPickerStage::Item => {
            let v = state
                .selected_vault
                .as_ref()
                .map_or("", |v| v.name.as_str());
            format!("loading items from {v}\u{2026}")
        }
        OpPickerStage::Field => {
            let i = state.selected_item.as_ref().map_or("", |i| i.name.as_str());
            format!("loading fields for {i}\u{2026}")
        }
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let body = Line::from(vec![
        Span::styled(glyph.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
        Span::raw("  "),
        Span::styled(descriptor, Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(body).alignment(Alignment::Center), rows[1]);

    let footer = footer_line(&[("running op", "subcommand"), ("Esc", "cancel")]);
    frame.render_widget(Paragraph::new(footer).alignment(Alignment::Center), rows[3]);
}

fn render_fatal(frame: &mut Frame, area: Rect, fatal: &OpPickerFatalState) {
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
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(body_lines).alignment(Alignment::Center),
        rows[1],
    );

    let footer = footer_line(&[("Esc", "close")]);
    frame.render_widget(Paragraph::new(footer).alignment(Alignment::Center), rows[3]);
}

#[cfg(test)]
mod tests {
    use super::{OpPickerStage, breadcrumb_title, viewport_offset};

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

    // ── Viewport scrolling ────────────────────────────────────────────

    #[test]
    fn viewport_offset_returns_zero_when_list_fits() {
        // 5 items, 10-row viewport — no scroll regardless of selection.
        assert_eq!(viewport_offset(0, 10, 5), 0);
        assert_eq!(viewport_offset(4, 10, 5), 0);
    }

    #[test]
    fn viewport_offset_anchors_top_until_cursor_falls_below_window() {
        // 20 items, 5-row viewport. Cursor in rows 0..5 → no scroll.
        assert_eq!(viewport_offset(0, 5, 20), 0);
        assert_eq!(viewport_offset(4, 5, 20), 0);
    }

    #[test]
    fn viewport_offset_pins_cursor_to_bottom_when_below_initial_window() {
        // 20 items, 5-row viewport. Cursor at row 5 → offset 1 (cursor
        // sits on the last visible row, rows[1..6] → 1,2,3,4,5).
        assert_eq!(viewport_offset(5, 5, 20), 1);
        // Cursor at row 10 → offset 6 (rows 6..11; cursor at end).
        assert_eq!(viewport_offset(10, 5, 20), 6);
    }

    #[test]
    fn viewport_offset_clamps_at_end() {
        // Cursor at the last row of a 20-item list with a 5-row
        // viewport must produce offset 15 — the last visible window
        // shows rows 15..20.
        assert_eq!(viewport_offset(19, 5, 20), 15);
        // Even past the end (defensive), we don't scroll past
        // total - height.
        assert_eq!(viewport_offset(99, 5, 20), 15);
    }

    #[test]
    fn viewport_offset_is_zero_when_height_is_zero() {
        // Defensive: `Constraint::Min(1)` could collapse to 0 if the
        // modal is squeezed down to a single border row. Treat that
        // as "no viewport" and return 0.
        assert_eq!(viewport_offset(7, 0, 20), 0);
    }
}
