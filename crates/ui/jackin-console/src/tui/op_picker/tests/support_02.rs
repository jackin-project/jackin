// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
impl ParityStub {
    pub(super) fn new() -> Self {
        Self {
            vaults: Vec::new(),
            items: std::collections::HashMap::new(),
            fields: std::collections::HashMap::new(),
            sections: std::collections::HashMap::new(),
        }
    }

    pub(super) fn with_vault(mut self, name: &str, id: &str) -> Self {
        self.vaults.push(OpVault {
            id: id.to_owned(),
            name: name.to_owned(),
        });
        self
    }

    pub(super) fn with_item(
        mut self,
        vault_id: &str,
        name: &str,
        id: &str,
        subtitle: &str,
    ) -> Self {
        self.items
            .entry(vault_id.to_owned())
            .or_default()
            .push(OpItem {
                id: id.to_owned(),
                name: name.to_owned(),
                subtitle: subtitle.to_owned(),
            });
        self
    }

    pub(super) fn with_field_with_reference(
        mut self,
        item_id: &str,
        label: &str,
        id: &str,
        concealed: bool,
        reference: &str,
    ) -> Self {
        self.fields
            .entry(item_id.to_owned())
            .or_default()
            .push(OpField {
                id: id.to_owned(),
                section_id: None,
                label: label.to_owned(),
                field_type: if concealed {
                    "CONCEALED".into()
                } else {
                    "STRING".into()
                },
                concealed,
                reference: reference.to_owned(),
            });
        self
    }

    pub(super) fn with_section_field_with_reference(
        self,
        item_id: &str,
        label: &str,
        id: &str,
        concealed: bool,
        section_id: &str,
        reference: &str,
    ) -> Self {
        let mut stub = self.with_field_with_reference(item_id, label, id, concealed, reference);
        if let Some(field) = stub
            .fields
            .get_mut(item_id)
            .and_then(|fields| fields.last_mut())
        {
            field.section_id = Some(section_id.to_owned());
        }
        stub
    }

    pub(super) fn with_section(mut self, item_id: &str, section: OpSection) -> Self {
        self.sections
            .entry(item_id.to_owned())
            .or_default()
            .push(section);
        self
    }
}

impl OpStructRunner for ParityStub {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        Ok(vec![])
    }
    fn vault_list(&self, _account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        Ok(self.vaults.clone())
    }
    fn item_list(&self, vault_id: &str, _account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(self.items.get(vault_id).cloned().unwrap_or_default())
    }
    fn item_get(
        &self,
        item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<OpField>> {
        Ok(jackin_core::OpItemDetail {
            fields: self.fields.get(item_id).cloned().unwrap_or_default(),
            sections: self.sections.get(item_id).cloned().unwrap_or_default(),
        })
    }
}

pub(super) fn render_picker_text(s: &OpPickerState, w: u16, h: u16) -> String {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        crate::tui::components::op_picker::render_picker(f, Rect::new(0, 0, w, h), s);
    })
    .unwrap();
    let buf = term.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn assert_modal_title(s: &OpPickerState, expected: &str) {
    let text = render_picker_text(s, 120, 30);
    assert!(
        text.contains(expected),
        "modal title must paint {expected:?}; buffer:\n{text}"
    );
}
