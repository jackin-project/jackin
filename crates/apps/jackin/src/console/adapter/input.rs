// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

pub(crate) mod editor;

mod dispatch;

pub use dispatch::handle_key;
pub(crate) use jackin_console::tui::input::mouse::{clickable_at, handle_mouse_with_config};

pub type InputOutcome = jackin_console::tui::message::ConsoleInputOutcome<
    jackin_core::RoleSelector,
    jackin_core::Agent,
    crate::console::ConsoleInstanceAction,
    jackin_core::LaunchSelection,
>;

pub(super) mod fixtures;
