// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestRole(String);

impl RoleChoice for TestRole {
    fn key(&self) -> String {
        self.0.clone()
    }
}

pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

pub(super) fn roles(keys: &[&str]) -> Vec<TestRole> {
    keys.iter().map(|k| TestRole((*k).to_owned())).collect()
}

pub(super) fn dump(state: &RolePickerState<TestRole>, w: u16, h: u16) -> String {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        let area = Rect::new(0, 0, w, h);
        render(f, area, state);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}
