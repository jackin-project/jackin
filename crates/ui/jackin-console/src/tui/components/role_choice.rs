// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::RoleSelector;

use super::role_picker::RoleChoice;

impl RoleChoice for RoleSelector {
    fn key(&self) -> String {
        self.to_string()
    }
}
