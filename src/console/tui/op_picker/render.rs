//! Render adapter for [`super::OpPickerState`].

use ratatui::{Frame, layout::Rect, text::Line};

use super::{OpLoadState, OpPickerStage, OpPickerState};
use jackin_console::tui::components::op_picker::{
    OpPickerAccountRef, OpPickerFieldDisplayRef, OpPickerItemRef, OpPickerRenderState,
    OpPickerVaultRef, account_lines, field_lines, item_choice_lines, render_picker,
    section_lines, selected_index_for_stage, vault_lines,
};
use jackin_tui::components::TextInputState;

pub fn render(frame: &mut Frame, area: Rect, state: &OpPickerState) {
    render_picker(frame, area, state);
}

impl OpPickerRenderState for OpPickerState {
    fn stage(&self) -> OpPickerStage {
        self.stage
    }

    fn load_state(&self) -> &OpLoadState {
        &self.load_state
    }

    fn filter_buffer(&self) -> &str {
        &self.filter_buf
    }

    fn account_count(&self) -> usize {
        self.accounts.len()
    }

    fn selected_account_email(&self) -> &str {
        self.selected_account
            .as_ref()
            .map_or("", |account| account.email.as_str())
    }

    fn selected_vault_name(&self) -> &str {
        self.selected_vault
            .as_ref()
            .map_or("", |vault| vault.name.as_str())
    }

    fn selected_item_name(&self) -> &str {
        self.selected_item
            .as_ref()
            .map_or("", |item| item.name.as_str())
    }

    fn selected_item_subtitle(&self) -> &str {
        self.selected_item
            .as_ref()
            .map_or("", |item| item.subtitle.as_str())
    }

    fn naming_stage_input(&self) -> Option<&TextInputState<'static>> {
        OpPickerState::naming_stage_input(self)
    }

    fn account_lines(&self) -> Vec<Line<'static>> {
        account_lines(
            self.filtered_accounts()
                .into_iter()
                .map(|account| OpPickerAccountRef {
                    email: &account.email,
                    url: &account.url,
                }),
            self.account_list_state.selected,
        )
    }

    fn vault_lines(&self) -> Vec<Line<'static>> {
        vault_lines(
            self.filtered_vaults()
                .into_iter()
                .map(|vault| OpPickerVaultRef {
                    id: &vault.id,
                    name: &vault.name,
                }),
            self.vault_list_state.selected,
        )
    }

    fn item_lines(&self) -> Vec<Line<'static>> {
        item_choice_lines(
            self.filtered_item_choices().into_iter().map(|choice| {
                choice.map(|item| OpPickerItemRef {
                    id: &item.id,
                    name: &item.name,
                    subtitle: &item.subtitle,
                })
            }),
            self.item_list_state.selected,
        )
    }

    fn section_lines(&self) -> Vec<Line<'static>> {
        section_lines(self.section_choices(), self.section_list_state.selected)
    }

    fn field_lines(&self) -> Vec<Line<'static>> {
        field_lines(
            self.build_field_display_rows(),
            self.filtered_fields()
                .into_iter()
                .map(|field| OpPickerFieldDisplayRef {
                    id: &field.id,
                    label: &field.label,
                    field_type: &field.field_type,
                    concealed: field.concealed,
                }),
            &self.collapsed_sections,
            self.field_list_state.selected,
        )
    }

    fn selected_index(&self) -> Option<usize> {
        selected_index_for_stage(
            self.stage,
            self.account_list_state.selected,
            self.vault_list_state.selected,
            self.item_list_state.selected,
            self.section_list_state.selected,
            self.field_list_state.selected,
        )
    }
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
