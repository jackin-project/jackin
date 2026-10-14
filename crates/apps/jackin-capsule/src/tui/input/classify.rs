// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{InputEvent, parse_csi_u_key, parse_xterm_modify_other_keys};

pub(crate) enum CsiClassification {
    Unknown,
    Suppress,
    Event(InputEvent),
}

pub(crate) fn classify_csi(seq: &[u8], palette_key: Option<u8>) -> CsiClassification {
    if seq == b"\x1b[I" {
        return CsiClassification::Event(InputEvent::FocusIn);
    }
    if seq == b"\x1b[O" {
        return CsiClassification::Event(InputEvent::FocusOut);
    }
    if let Some(rest) = seq
        .strip_prefix(b"\x1b[")
        .and_then(|body| body.strip_suffix(b"u"))
    {
        let Some((codepoint, modifier, event)) = parse_csi_u_key(rest) else {
            return CsiClassification::Unknown;
        };
        if event == Some(3) {
            return CsiClassification::Suppress;
        }
        if codepoint == 27 && modifier.unwrap_or(1) == 1 {
            return CsiClassification::Event(InputEvent::Data(b"\x1b".to_vec()));
        }
        if matches!(event, None | Some(1 | 2))
            && let Some(control) = csi_u_control_byte(codepoint, modifier)
        {
            return dispatch_control_byte(control, palette_key)
                .map_or(CsiClassification::Unknown, |event| {
                    CsiClassification::Event(event)
                });
        }
    }
    if matches!(seq.last(), Some(b't')) {
        return CsiClassification::Suppress;
    }
    if let Some((13, 2)) = parse_xterm_modify_other_keys(seq) {
        return CsiClassification::Event(InputEvent::Data(b"\x1b[13;2u".to_vec()));
    }
    if let Some((codepoint, modifier)) = parse_xterm_modify_other_keys(seq)
        && let Some(control) = csi_u_control_byte(codepoint, Some(modifier))
    {
        return dispatch_control_byte(control, palette_key)
            .map_or(CsiClassification::Unknown, |event| {
                CsiClassification::Event(event)
            });
    }
    if let Some(rest) = seq.strip_prefix(b"\x1b[1;")
        && let Some(&final_byte) = rest.last()
        && matches!(final_byte, b'A' | b'B' | b'C' | b'D')
    {
        let body = &rest[..rest.len() - 1];
        let (mod_part, event) = match body.iter().position(|&b| b == b':') {
            Some(i) => {
                let ev = std::str::from_utf8(&body[i + 1..])
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(1);
                (&body[..i], ev)
            }
            None => (body, 1u32),
        };
        let modifier: u32 = std::str::from_utf8(mod_part)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        if event == 3 {
            return CsiClassification::Suppress;
        }

        if modifier == 4 {
            let action = match final_byte {
                b'A' => crate::tui::keymap::ResizePaneAction::Up,
                b'B' => crate::tui::keymap::ResizePaneAction::Down,
                b'C' => crate::tui::keymap::ResizePaneAction::Right,
                b'D' => crate::tui::keymap::ResizePaneAction::Left,
                _ => unreachable!("kitty arrow parser only calls resize mapping for arrow bytes"),
            };
            return CsiClassification::Event(action.to_input_event());
        }

        if modifier == 1 && body.contains(&b':') {
            let mut plain = b"\x1b[".to_vec();
            plain.push(final_byte);
            return CsiClassification::Event(InputEvent::Data(plain));
        }
    }
    if let Some(rest) = seq.strip_prefix(b"\x1b[<")
        && let Some(final_byte) = rest.last()
        && matches!(final_byte, b'M' | b'm')
    {
        let body = &rest[..rest.len() - 1];
        let params: Option<Vec<u32>> = body
            .split(|&b| b == b';')
            .map(|p| std::str::from_utf8(p).ok().and_then(|s| s.parse().ok()))
            .collect();
        if let Some(p) = params
            && p.len() >= 3
        {
            let button = u8::try_from(p[0]).unwrap_or(u8::MAX);
            let col = u16::try_from(p[1]).unwrap_or(u16::MAX).saturating_sub(1);
            let row = u16::try_from(p[2]).unwrap_or(u16::MAX).saturating_sub(1);
            if *final_byte == b'M' {
                return CsiClassification::Event(InputEvent::MousePress { col, row, button });
            }
            return CsiClassification::Event(InputEvent::MouseRelease { col, row, button });
        }
    }
    CsiClassification::Unknown
}

pub(crate) fn dispatch_control_byte(control: u8, palette_key: Option<u8>) -> Option<InputEvent> {
    if Some(control) == palette_key {
        return Some(InputEvent::OpenPalette);
    }
    if let Some(chord) = crate::tui::keymap::raw_bytes_to_chord(&[control])
        && let Some(action) = crate::tui::keymap::CAPSULE_GLOBAL_KEYMAP.dispatch(chord)
    {
        return Some(action.to_input_event());
    }
    None
}

pub(crate) fn csi_u_control_byte(codepoint: u32, modifier: Option<u32>) -> Option<u8> {
    if (0x00..=0x1f).contains(&codepoint) {
        return u8::try_from(codepoint).ok();
    }
    let modifier = modifier?;
    let ctrl_held = modifier.saturating_sub(1) & 0b100 != 0;
    if !ctrl_held {
        return None;
    }
    match u8::try_from(codepoint).ok()? {
        byte @ (b'a'..=b'z' | b'A'..=b'Z') => Some(byte.to_ascii_lowercase() - b'a' + 1),
        b'\\' => Some(0x1C),
        b']' => Some(0x1D),
        b'^' => Some(0x1E),
        b'_' => Some(0x1F),
        _ => None,
    }
}

pub(crate) fn classify_x10_mouse(seq: &[u8]) -> Option<InputEvent> {
    if seq.len() != 6 || !seq.starts_with(b"\x1b[M") {
        return None;
    }
    let button = seq[3].checked_sub(32)?;
    let col = u16::from(seq[4]).checked_sub(33)?;
    let row = u16::from(seq[5]).checked_sub(33)?;
    if button & 0b11 == 3 && button & 0b10_0000 == 0 {
        return Some(InputEvent::MouseRelease {
            col,
            row,
            button: 0,
        });
    }
    Some(InputEvent::MousePress { col, row, button })
}
