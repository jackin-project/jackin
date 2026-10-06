// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn field(id: &str, section_id: Option<&str>, reference: &str) -> OpPickerField {
    OpPickerField {
        id: id.to_owned(),
        section_id: section_id.map(str::to_owned),
        label: id.to_owned(),
        field_type: "STRING".to_owned(),
        concealed: false,
        reference: reference.to_owned(),
    }
}

pub(super) struct RenderStateFixture {
    stage: OpPickerStage,
    selected: Option<usize>,
    pub(super) load_state: OpLoadState,
}

impl RenderStateFixture {
    pub(super) const fn new(stage: OpPickerStage, selected: Option<usize>) -> Self {
        Self {
            stage,
            selected,
            load_state: OpLoadState::Ready,
        }
    }
}

impl OpPickerRenderState for RenderStateFixture {
    fn stage(&self) -> OpPickerStage {
        self.stage
    }

    fn load_state(&self) -> &OpLoadState {
        &self.load_state
    }

    fn filter_buffer(&self) -> &'static str {
        ""
    }

    fn account_count(&self) -> usize {
        2
    }

    fn selected_account_email(&self) -> &'static str {
        "alice@example.com"
    }

    fn selected_vault_name(&self) -> &'static str {
        "Private"
    }

    fn selected_item_name(&self) -> &'static str {
        "Cloudflare"
    }

    fn selected_item_subtitle(&self) -> &'static str {
        "alice@example.com"
    }

    fn naming_stage_input(&self) -> Option<&TextInputState<'static>> {
        None
    }

    fn account_lines(&self) -> Vec<Line<'static>> {
        account_lines(
            [
                OpPickerAccountRef {
                    email: "alice@example.com",
                    url: "alice.1password.com",
                },
                OpPickerAccountRef {
                    email: "bob@example.com",
                    url: "bob.1password.com",
                },
            ],
            self.selected,
        )
    }

    fn vault_lines(&self) -> Vec<Line<'static>> {
        vault_lines(
            [OpPickerVaultRef {
                id: "v1",
                name: "Private",
            }],
            self.selected,
        )
    }

    fn item_lines(&self) -> Vec<Line<'static>> {
        item_choice_lines(
            [
                Some(OpPickerItemRef {
                    id: "i1",
                    name: "Cloudflare",
                    subtitle: "alice@example.com",
                }),
                Some(OpPickerItemRef {
                    id: "i2",
                    name: "GitHub",
                    subtitle: "bob@example.com",
                }),
            ],
            self.selected,
        )
    }

    fn section_lines(&self) -> Vec<Line<'static>> {
        section_lines(
            [
                None,
                Some(OpSection {
                    id: "auth-id".to_owned(),
                    label: "Auth".to_owned(),
                }),
            ],
            self.selected,
        )
    }

    fn field_lines(&self) -> Vec<Line<'static>> {
        field_lines(
            [FieldDisplayRow::Field { field_idx: 0 }],
            [OpPickerFieldDisplayRef {
                id: "f1",
                label: "password",
                field_type: "CONCEALED",
                concealed: true,
            }],
            &HashSet::new(),
            self.selected,
        )
    }

    fn selected_index(&self) -> Option<usize> {
        self.selected
    }
}

pub(super) fn render_picker_buffer(
    state: &RenderStateFixture,
    w: u16,
    h: u16,
) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_picker(f, Rect::new(0, 0, w, h), state))
        .unwrap();
    term.backend().buffer().clone()
}
