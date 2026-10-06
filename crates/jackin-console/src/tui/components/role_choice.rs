// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `RoleChoice` impl for `jackin_core::RoleSelector`.
//!
//! Lives here (not in `jackin-core`) to satisfy the orphan rule: `RoleChoice`
//! is defined in this crate; `RoleSelector` is defined in `jackin-core`.

use jackin_core::RoleSelector;

use super::role_picker::RoleChoice;

impl RoleChoice for RoleSelector {
    fn key(&self) -> String {
        self.to_string()
    }
}
