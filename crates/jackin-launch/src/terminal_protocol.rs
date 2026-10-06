// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-terminal protocol encoding used by the launch adapter.

/// Encode the requested host pointer shape through `TermRock`'s typed OSC API.
#[must_use]
pub fn encode_pointer_shape(pointer: bool) -> Vec<u8> {
    let shape = if pointer {
        termrock::osc::PointerShape::Pointer
    } else {
        termrock::osc::PointerShape::Default
    };
    termrock::osc::encode_pointer(shape)
}

/// Encode a system-clipboard write through `TermRock`'s typed OSC API.
#[must_use]
pub fn encode_clipboard_write(payload: &str) -> Vec<u8> {
    termrock::osc::encode_clipboard(termrock::osc::ClipboardWrite {
        selection: termrock::osc::ClipboardSelection::Clipboard,
        text: payload,
    })
}
