// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::EnvSnapshot;
use jackin_session::{ClipboardWriter, HostOs};
use junie_tui::shell::NativeClipboard;

#[derive(Debug)]
pub struct HostClipboard {
    writer: ClipboardWriter,
}

impl HostClipboard {
    pub fn detect(env: &EnvSnapshot) -> HostClipboard {
        HostClipboard::over(ClipboardWriter::detect(HostOs::current(), env))
    }

    pub fn over(writer: ClipboardWriter) -> HostClipboard {
        HostClipboard { writer }
    }
}

impl NativeClipboard for HostClipboard {
    fn write(&self, text: &str) -> bool {
        self.writer.write(text)
    }
}
