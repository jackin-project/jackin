// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Exec credential picker dialog state (`ExecPickerState`).

/// State for the exec credential picker dialog shown by the daemon's TUI.
#[derive(Debug, Clone)]
pub struct ExecPickerState {
    pub command: String,
    pub args: Vec<String>,
    pub items: Vec<ExecPickerItem>,
    pub cursor: usize,
}

/// A single on-demand credential row in the picker. Carries the underlying
/// [`ExecBinding`] verbatim (so a confirm sends it back unchanged) plus a
/// human-readable display label and the operator's selection state.
#[derive(Debug, Clone)]
pub struct ExecPickerItem {
    /// The binding sent to the host resolver if this row is selected.
    pub binding: jackin_protocol::ExecBinding,
    /// Human-readable label (the source for `op`/`env`, the name for literals).
    /// Never a resolved secret value.
    pub display: String,
    /// Whether the operator has selected this item.
    pub selected: bool,
}

impl ExecPickerState {
    /// Build the picker for a `jackin-exec <command> [args…]` invocation from
    /// the workspace's on-demand bindings. Every binding becomes one unselected
    /// row; the operator toggles the ones the command needs. The display label
    /// is the source for `op`/`env` kinds (never a resolved secret) and the
    /// name for literals.
    #[must_use]
    pub fn from_bindings(
        command: String,
        args: Vec<String>,
        bindings: &[jackin_protocol::ExecBinding],
    ) -> Self {
        let items = bindings
            .iter()
            .map(|b| {
                // Literals have no meaningful source to show; everything else
                // displays its source (op:// path or $VAR), never a secret.
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

    /// Returns the selected items as host.sock credential bindings.
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
