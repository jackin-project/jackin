// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    SendAttachInput(Vec<u8>),
    ResizeAttachClient { rows: u16, cols: u16 },
}
