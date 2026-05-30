//! Mouse protocol encoding for the capsule multiplexer.

use crate::session::Session;
use vt100::{MouseProtocolEncoding, MouseProtocolMode};

pub(crate) fn pane_wheel_cursor_fallback_reason(session: &Session) -> Option<&'static str> {
    if session.mouse_enabled() {
        return None;
    }
    if session.screen().alternate_screen() {
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
    mode: MouseProtocolMode,
    button: u8,
    press: bool,
) -> bool {
    if mode == MouseProtocolMode::None {
        return false;
    }
    if is_wheel_button(button) {
        return true;
    }

    let motion = button & 0b100000 != 0;
    let passive_motion = motion && button & 0b11 == 3;
    match mode {
        MouseProtocolMode::None => false,
        MouseProtocolMode::Press => press && !motion,
        MouseProtocolMode::PressRelease => !motion,
        MouseProtocolMode::ButtonMotion => !passive_motion,
        MouseProtocolMode::AnyMotion => true,
    }
}

pub(crate) fn mouse_event_encoding_for_session(
    session: &Session,
    button: u8,
    press: bool,
) -> Option<MouseProtocolEncoding> {
    if mouse_event_allowed_for_mode(session.mouse_protocol_mode(), button, press) {
        return Some(session.mouse_protocol_encoding());
    }
    None
}

pub(crate) fn encode_mouse_for_protocol(
    button: u8,
    col: u16,
    row: u16,
    press: bool,
    encoding: MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    match encoding {
        MouseProtocolEncoding::Sgr => {
            let final_byte = if press { 'M' } else { 'm' };
            Some(format!("\x1b[<{button};{col};{row}{final_byte}").into_bytes())
        }
        MouseProtocolEncoding::Default | MouseProtocolEncoding::Utf8 => {
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

pub(crate) fn encode_wheel_cursor_fallback(session: &Session, button: u8) -> Option<Vec<u8>> {
    if !is_wheel_button(button) || session.mouse_enabled() {
        return None;
    }
    let seq = if session.screen().application_cursor() {
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
    encoding: MouseProtocolEncoding,
) -> Option<()> {
    match encoding {
        MouseProtocolEncoding::Default => {
            out.push(u8::try_from(value).ok()?);
        }
        MouseProtocolEncoding::Utf8 => {
            let ch = char::from_u32(value)?;
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
        MouseProtocolEncoding::Sgr => unreachable!("SGR does not use xterm fields"),
    }
    Some(())
}
