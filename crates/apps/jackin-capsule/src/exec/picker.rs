// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone)]
pub struct ExecPickerState {
    pub command: String,
    pub args: Vec<String>,
    pub items: Vec<ExecPickerItem>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub struct ExecPickerItem {
    pub binding: jackin_protocol::ExecBinding,

    pub display: String,

    pub selected: bool,
}

impl ExecPickerState {
    #[must_use]
    pub fn from_bindings(
        command: String,
        args: Vec<String>,
        bindings: &[jackin_protocol::ExecBinding],
    ) -> Self {
        let items = bindings
            .iter()
            .map(|b| {
                let display = if b.kind == jackin_protocol::ExecKind::Literal {
                    b.name.clone()
                } else {
                    b.source.clone()
                };
                ExecPickerItem {
                    binding: b.clone(),
                    display,
                    selected: false,
                }
            })
            .collect();
        Self {
            command,
            args,
            items,
            cursor: 0,
        }
    }

    #[must_use]
    pub fn selected_refs(&self) -> Vec<jackin_protocol::ExecBinding> {
        self.items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.binding.clone())
            .collect()
    }

    pub fn toggle_cursor(&mut self) {
        if let Some(item) = self.items.get_mut(self.cursor) {
            item.selected = !item.selected;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.items.len() {
            self.cursor += 1;
        }
    }
}
