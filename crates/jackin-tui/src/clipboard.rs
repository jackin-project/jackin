// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use junie_tui::shell::NativeClipboard;

#[derive(Debug)]
pub struct HostClipboard;

impl NativeClipboard for HostClipboard {
    fn write(&self, _text: &str) -> bool {
        false
    }
}
