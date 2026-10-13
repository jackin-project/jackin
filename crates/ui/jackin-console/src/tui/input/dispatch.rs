// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Key dispatch for the workspace manager.
mod confirms;
mod keys;
#[cfg(test)]
mod tests;
pub(crate) use confirms::{
    handle_confirm_delete_key, handle_confirm_instance_purge_key, handle_keyboard_help,
};
pub use keys::handle_key;

#[cfg(test)]
pub(crate) use crate::tui::state::ManagerStage;
#[cfg(test)]
pub(crate) use crate::tui::state::ManagerState;
#[cfg(test)]
pub(crate) use jackin_config::AppConfig;
#[cfg(test)]
pub(crate) use jackin_core::JackinPaths;
