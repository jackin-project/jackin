// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console-local save/discard prompt state construction.

pub fn editor_exit_save_discard_state() -> jackin_tui::components::SaveDiscardState {
    jackin_tui::components::SaveDiscardState::new("Save changes before leaving?")
}
