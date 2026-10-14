// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn editor_exit_save_discard_state() -> crate::tui::components::SaveDiscardState {
    crate::tui::components::SaveDiscardState::new("Save changes before leaving?")
}
