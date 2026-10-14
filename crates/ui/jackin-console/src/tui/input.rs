// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub mod auth;
pub mod dispatch;
pub mod editor;
pub mod global_mounts;
pub mod list;
pub mod mouse;
pub mod prelude;
pub mod save;

pub use global_mounts::{
    apply_op_picker_to_settings_auth_form_committed, apply_plain_text_to_settings_auth_form,
};

pub type InputOutcome = crate::tui::message::ConsoleInputOutcome<
    jackin_core::RoleSelector,
    jackin_core::Agent,
    crate::tui::message::ConsoleInstanceAction<jackin_core::Agent>,
    jackin_core::LaunchSelection,
>;

pub mod fixtures;
