// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod confirms;
mod keys;
pub(crate) use confirms::{
    handle_confirm_delete_key, handle_confirm_instance_purge_key, handle_keyboard_help,
};
pub use keys::handle_key;
