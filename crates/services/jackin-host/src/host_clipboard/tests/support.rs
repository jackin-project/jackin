// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn bracketed(content: &str) -> Vec<u8> {
    let mut input = b"\x1b[200~".to_vec();
    input.extend_from_slice(content.as_bytes());
    input.extend_from_slice(b"\x1b[201~");
    input
}
