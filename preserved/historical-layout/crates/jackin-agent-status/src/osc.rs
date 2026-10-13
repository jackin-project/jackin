// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Persistent decoding of terminal status evidence, independent of PTY reads.

/// Completed OSC 133 shell integration marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscShellMark {
    PromptStart,
    PromptEnd,
    PreExec,
    CommandFinished { exit_code: Option<i32> },
}

/// Status evidence emitted only after a complete BEL or ESC-backslash terminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscStatusEvent {
    Shell(OscShellMark),
    Progress(u8),
}

#[derive(Debug, Clone, Copy, Default)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    Osc,
    OscEscape,
}

/// Per-session decoder for OSC 133 and OSC 9;4.
///
/// Retains at most 64 payload bytes. Oversized or cancelled sequences produce
/// no evidence. Only seven-bit ESC ] introducers and BEL / ESC-backslash
/// terminators are recognized; C1 bytes are opaque. An ESC followed by anything
/// other than backslash abandons the unfinished OSC and starts a new escape.
/// As in the grid parser, ESC also abandons other terminal strings. Unlike the
/// grid's OSC dispatch, an unfinished OSC never becomes status evidence.
#[derive(Debug)]
pub struct OscStatusDecoder {
    state: State,
    payload: [u8; 64],
    len: usize,
    overflow: bool,
}

impl Default for OscStatusDecoder {
    fn default() -> Self {
        Self {
            state: State::Ground,
            payload: [0; 64],
            len: 0,
            overflow: false,
        }
    }
}

impl OscStatusDecoder {
    /// Decode every completed event in wire order, preserving partial state.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<OscStatusEvent> {
        let mut events = Vec::new();
        for &byte in bytes {
            if matches!(byte, 0x18 | 0x1a) {
                self.state = State::Ground;
                continue;
            }
            match self.state {
                State::Ground => {
                    if byte == 0x1b {
                        self.state = State::Escape;
                    }
                }
                State::Escape => self.escape(byte),
                State::EscapeIntermediate => match byte {
                    0x1b => self.state = State::Escape,
                    0x30..=0x7e => self.state = State::Ground,
                    _ => {}
                },
                State::Osc => match byte {
                    0x07 => {
                        self.finish(&mut events);
                        self.state = State::Ground;
                    }
                    0x1b => self.state = State::OscEscape,
                    0x00..=0x1f => {}
                    _ => {
                        if self.len == self.payload.len() {
                            self.overflow = true;
                        } else {
                            self.payload[self.len] = byte;
                            self.len += 1;
                        }
                    }
                },
                State::OscEscape => {
                    if byte == b'\\' {
                        self.finish(&mut events);
                        self.state = State::Ground;
                    } else {
                        self.state = State::Escape;
                        self.escape(byte);
                    }
                }
            }
        }
        events
    }

    fn escape(&mut self, byte: u8) {
        match byte {
            b']' => {
                self.len = 0;
                self.overflow = false;
                self.state = State::Osc;
            }
            0x20..=0x2f => self.state = State::EscapeIntermediate,
            0x30..=0x7e => self.state = State::Ground,
            _ => {}
        }
    }

    fn finish(&self, events: &mut Vec<OscStatusEvent>) {
        if !self.overflow
            && let Some(event) = parse_payload(&self.payload[..self.len])
        {
            events.push(event);
        }
    }
}

fn parse_payload(payload: &[u8]) -> Option<OscStatusEvent> {
    let mut fields = payload.split(|&byte| byte == b';');
    match fields.next()? {
        b"133" => {
            let mark = match fields.next()? {
                b"A" => OscShellMark::PromptStart,
                b"B" => OscShellMark::PromptEnd,
                b"C" => OscShellMark::PreExec,
                b"D" => {
                    let exit_code = fields
                        .next()
                        .map(|code| std::str::from_utf8(code).ok()?.parse::<i32>().ok());
                    OscShellMark::CommandFinished {
                        exit_code: match exit_code {
                            Some(code) => Some(code?),
                            None => None,
                        },
                    }
                }
                _ => return None,
            };
            Some(OscStatusEvent::Shell(mark))
        }
        b"9" if fields.next()? == b"4" => {
            let state = fields.next()?;
            // Preserve existing handling of reserved single-digit states.
            match state {
                [digit] if digit.is_ascii_digit() => Some(OscStatusEvent::Progress(digit - b'0')),
                _ => None,
            }
        }
        _ => None,
    }
}
