// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Mouse protocol gating and encoding: SGR, xterm, and wheel fallback.

/// `XTerm` SGR any-event mouse tracking reports passive motion as
/// button code 35 (`32` motion bit + `3` no-button code).
pub(crate) const SGR_NO_BUTTON_MOTION: u8 = 35;

pub(crate) fn pane_wheel_cursor_fallback_reason(
    mouse_enabled: bool,
    alternate_screen: bool,
) -> Option<&'static str> {
    if mouse_enabled {
        return None;
    }
    if alternate_screen {
        return Some("alternate-screen");
    }
    None
}

/// SGR mouse wheel events set bit 6 of the button byte. Every value in
/// `64..=95` is a wheel event with some combination of modifier flags
/// (shift = +4, alt = +8, ctrl = +16). Panes that did not request
/// mouse mode must not receive these bytes because they dump raw SGR at
/// prompts or disappear into TUIs that never subscribed to mouse input.
pub(crate) fn is_wheel_button(button: u8) -> bool {
    (64..96).contains(&button)
}

pub(crate) fn mouse_event_allowed_for_mode(
    mode: termpane::MouseProtocolMode,
    button: u8,
    press: bool,
) -> bool {
    if mode == termpane::MouseProtocolMode::None {
        return false;
    }
    if is_wheel_button(button) {
        return true;
    }

    let motion = button & 0b10_0000 != 0;
    let passive_motion = motion && button & 0b11 == 3;
    match mode {
        termpane::MouseProtocolMode::None => false,
        termpane::MouseProtocolMode::Press => press && !motion,
        // PressRelease = mode 1001: press + release events, no motion.
        termpane::MouseProtocolMode::PressRelease => !motion,
        // ButtonMotion = mode 1002: press + release + button-held motion, no passive motion.
        termpane::MouseProtocolMode::ButtonMotion => !passive_motion,
        // AnyEvent and AnyMotion are aliases for mode 1003: all events.
        termpane::MouseProtocolMode::AnyEvent | termpane::MouseProtocolMode::AnyMotion => true,
    }
}

pub(crate) fn mouse_event_encoding_for_mode(
    mode: termpane::MouseProtocolMode,
    encoding: termpane::MouseProtocolEncoding,
    button: u8,
    press: bool,
) -> Option<termpane::MouseProtocolEncoding> {
    if mouse_event_allowed_for_mode(mode, button, press) {
        return Some(encoding);
    }
    None
}

pub(crate) fn encode_mouse_for_protocol(
    button: u8,
    col: u16,
    row: u16,
    press: bool,
    encoding: termpane::MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    match encoding {
        termpane::MouseProtocolEncoding::Sgr => {
            let final_byte = if press { 'M' } else { 'm' };
            Some(format!("\x1b[<{button};{col};{row}{final_byte}").into_bytes())
        }
        termpane::MouseProtocolEncoding::Default
        | termpane::MouseProtocolEncoding::Utf8
        // Urxvt uses decimal coordinates but the same CSI M prefix — treat as Default.
        | termpane::MouseProtocolEncoding::Urxvt => {
            let release_button = (button & !0b11) | 3;
            let button_code = if press { button } else { release_button };
            let mut out = b"\x1b[M".to_vec();
            push_xterm_mouse_number(&mut out, u32::from(button_code) + 32, encoding)?;
            push_xterm_mouse_number(&mut out, u32::from(col) + 32, encoding)?;
            push_xterm_mouse_number(&mut out, u32::from(row) + 32, encoding)?;
            Some(out)
        }
    }
}

pub(crate) fn encode_wheel_cursor_fallback(
    mouse_enabled: bool,
    application_cursor: bool,
    button: u8,
) -> Option<Vec<u8>> {
    if !is_wheel_button(button) || mouse_enabled {
        return None;
    }
    let seq = if application_cursor {
        if (button & 1) == 0 {
            b"\x1bOA".as_slice()
        } else {
            b"\x1bOB".as_slice()
        }
    } else if (button & 1) == 0 {
        b"\x1b[A".as_slice()
    } else {
        b"\x1b[B".as_slice()
    };
    let mut out = Vec::with_capacity(seq.len() * 3);
    for _ in 0..3 {
        out.extend_from_slice(seq);
    }
    Some(out)
}

pub(crate) fn push_xterm_mouse_number(
    out: &mut Vec<u8>,
    value: u32,
    encoding: termpane::MouseProtocolEncoding,
) -> Option<()> {
    match encoding {
        termpane::MouseProtocolEncoding::Default | termpane::MouseProtocolEncoding::Urxvt => {
            out.push(u8::try_from(value).ok()?);
        }
        termpane::MouseProtocolEncoding::Utf8 => {
            let ch = char::from_u32(value)?;
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
        termpane::MouseProtocolEncoding::Sgr => unreachable!("SGR does not use xterm fields"),
    }
    Some(())
}
