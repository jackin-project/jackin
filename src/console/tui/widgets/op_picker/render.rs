//! Render path for [`super::OpPickerState`].

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::super::{PHOSPHOR_DIM, PHOSPHOR_GREEN, SPINNER_FRAMES, WHITE};
use super::{OpLoadState, OpPickerError, OpPickerFatalState, OpPickerStage, OpPickerState};
use jackin_console::widgets::op_picker::{
    OpPickerAccountRef, OpPickerItemRef, OpPickerVaultRef, account_lines, breadcrumb_title,
    fatal_body_lines, field_lines, item_choice_lines, loading_descriptor, loading_title_stage,
    section_lines, vault_lines,
};
use jackin_tui::components::scrollable_panel::render_selected_lines_in_area;
use jackin_tui::components::{Panel, PanelFocus};

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
    let title_with_spaces = format!(" {title} ");
    let block = Panel::new()
        .title(&title_with_spaces)
        .focus(PanelFocus::Focused)
        .block();
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
    // Pre-v2 op may omit email/url; render as empty rather than panicking.
    account_lines(
        state
            .filtered_accounts()
            .into_iter()
            .map(|account| OpPickerAccountRef {
                email: &account.email,
                url: &account.url,
            }),
        state.account_list_state.selected,
    )
}

fn render_vault_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    vault_lines(
        state
            .filtered_vaults()
            .into_iter()
            .map(|vault| OpPickerVaultRef {
                id: &vault.id,
                name: &vault.name,
            }),
        state.vault_list_state.selected,
    )
}

fn render_item_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    // Use the choice list so the trailing `+ New item` sentinel (Create
    // mode) is rendered and selectable at the same index the handler uses.
    item_choice_lines(
        state.filtered_item_choices().into_iter().map(|choice| {
            choice.map(|item| OpPickerItemRef {
                id: &item.id,
                name: &item.name,
                subtitle: &item.subtitle,
            })
        }),
        state.item_list_state.selected,
    )
}

/// Section stage (Create mode): `(root)`, each named section, then a
/// trailing `+ New section` sentinel — same selected/prefix styling as the
/// vault/item lists.
fn render_section_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    section_lines(state.section_choices(), state.section_list_state.selected)
}

fn render_field_lines(state: &OpPickerState) -> Vec<Line<'static>> {
    field_lines(
        state.build_field_display_rows(),
        state.filtered_fields().into_iter().map(|field| {
            jackin_console::widgets::op_picker::OpPickerFieldDisplayRef {
                id: &field.id,
                label: &field.label,
                field_type: &field.field_type,
                concealed: field.concealed,
            }
        }),
        &state.collapsed_sections,
        state.field_list_state.selected,
    )
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
    let title = breadcrumb_title(
        loading_title_stage(state.stage),
        multi_account,
        account_email,
        v_name,
        i_name,
    );
    let title_with_spaces = format!(" {title} ");
    let block = Panel::new()
        .title(&title_with_spaces)
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let glyph = SPINNER_FRAMES[(tick as usize) % SPINNER_FRAMES.len()];
    let descriptor = loading_descriptor(
        state.stage,
        multi_account,
        account_email,
        v_name,
        i_name,
        i_subtitle,
    );

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
    let block = Panel::new()
        .title(" 1Password ")
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(fatal_body_lines(fatal)).alignment(Alignment::Center),
        rows[1],
    );
}

#[cfg(test)]
mod tests {
    use super::OpPickerStage;

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
