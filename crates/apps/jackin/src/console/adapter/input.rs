// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod dispatch;

pub use dispatch::handle_key;
pub(crate) use jackin_console::tui::input::mouse::{clickable_at, handle_mouse_with_config};

pub type InputOutcome = jackin_console::tui::message::ConsoleInputOutcome<
    jackin_core::RoleSelector,
    jackin_core::Agent,
    crate::console::ConsoleInstanceAction,
    jackin_core::LaunchSelection,
>;
