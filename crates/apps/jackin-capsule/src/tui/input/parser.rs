// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    CsiClassification, InputBindings, InputEvent, classify_csi, classify_x10_mouse, prefix_binding,
};

#[derive(Debug)]
pub struct InputParser {
    prefix: Option<u8>,

    palette_key: Option<u8>,
    state: State,
    seq: Vec<u8>,
    in_paste: bool,
}

pub(crate) const MAX_ESC_SEQ_LEN: usize = 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum State {
    Idle,
    PrefixAwait,
    EscStart,
    Csi,
    X10Mouse,
    Osc,
    OtherEsc,

    Ss3,
}

impl Default for InputParser {
    fn default() -> Self {
        let bindings = InputBindings::default();
        Self::new(bindings.prefix, bindings.palette_key)
    }
}

impl InputParser {
    #[must_use]
    pub fn new(prefix: Option<u8>, palette_key: Option<u8>) -> Self {
        Self {
            prefix,
            palette_key,
            state: State::Idle,
            seq: Vec::new(),
            in_paste: false,
        }
    }

    #[must_use]
    pub fn is_awaiting_prefix(&self) -> bool {
        matches!(self.state, State::PrefixAwait)
    }

    #[must_use]
    pub fn prefix_enabled(&self) -> bool {
        self.prefix.is_some()
    }

    #[must_use]
    pub fn palette_key(&self) -> Option<u8> {
        self.palette_key
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "ANSI-input parser: per-byte classification + per-event-type \
                  (Key / Mouse / Focus / Paste / Data) branches with state- \
                  machine arms. The nesting is the bytewise state machine."
    )]
    pub fn parse(&mut self, bytes: &[u8]) -> Vec<InputEvent> {
        let mut events = Vec::new();
        let mut data: Vec<u8> = Vec::new();

        for &b in bytes {
            if self.in_paste {
                data.push(b);
                if data.ends_with(PASTE_END) {
                    flush(&mut data, &mut events);
                    self.in_paste = false;
                }
                continue;
            }

            match self.state {
                State::Idle => {
                    if Some(b) == self.palette_key {
                        flush(&mut data, &mut events);
                        events.push(InputEvent::OpenPalette);
                    } else if let Some(chord) = crate::tui::keymap::raw_bytes_to_chord(&[b])
                        && let Some(action) =
                            crate::tui::keymap::CAPSULE_GLOBAL_KEYMAP.dispatch(chord)
                    {
                        flush(&mut data, &mut events);
                        events.push(action.to_input_event());
                    } else if Some(b) == self.prefix {
                        flush(&mut data, &mut events);
                        self.state = State::PrefixAwait;
                    } else if b == 0x1B {
                        flush(&mut data, &mut events);
                        self.seq.clear();
                        self.seq.push(b);
                        self.state = State::EscStart;
                    } else {
                        data.push(b);
                    }
                }
                State::PrefixAwait => {
                    if Some(b) == self.prefix {
                        if let Some(p) = self.prefix {
                            data.push(p);
                        }
                    } else if let Some(cmd) = prefix_binding(b) {
                        events.push(InputEvent::PrefixCommand(cmd));
                    }
                    self.state = State::Idle;
                }
                State::EscStart => {
                    if Some(b) == self.palette_key {
                        self.seq.clear();
                        events.push(InputEvent::OpenPalette);
                        self.state = State::Idle;
                        continue;
                    }
                    self.seq.push(b);
                    match b {
                        b'[' => self.state = State::Csi,
                        b']' => self.state = State::Osc,
                        b'O' => self.state = State::Ss3,
                        b'P' | b'_' | b'X' | b'^' => self.state = State::OtherEsc,
                        _ => {
                            events.push(InputEvent::Data(std::mem::take(&mut self.seq)));
                            self.state = State::Idle;
                        }
                    }
                }
                State::Ss3 => {
                    self.seq.push(b);
                    let seq = std::mem::take(&mut self.seq);
                    match classify_csi(&seq, self.palette_key) {
                        CsiClassification::Event(ev) => events.push(ev),
                        CsiClassification::Suppress => {}
                        CsiClassification::Unknown => events.push(InputEvent::Data(seq)),
                    }
                    self.state = State::Idle;
                }
                State::Csi => {
                    if self.seq.len() >= MAX_ESC_SEQ_LEN {
                        self.seq.clear();
                        self.state = State::Idle;
                        continue;
                    }
                    self.seq.push(b);
                    if matches!(b, 0x40..=0x7E) {
                        if self.seq.as_slice() == b"\x1b[M" {
                            self.state = State::X10Mouse;
                            continue;
                        }

                        let seq = std::mem::take(&mut self.seq);
                        if seq == PASTE_START {
                            events.push(InputEvent::Data(seq));
                            self.in_paste = true;
                        } else {
                            match classify_csi(&seq, self.palette_key) {
                                CsiClassification::Event(ev) => events.push(ev),
                                CsiClassification::Suppress => {}
                                CsiClassification::Unknown => events.push(InputEvent::Data(seq)),
                            }
                        }
                        self.state = State::Idle;
                    }
                }
                State::X10Mouse => {
                    self.seq.push(b);
                    if self.seq.len() == 6 {
                        let seq = std::mem::take(&mut self.seq);
                        match classify_x10_mouse(&seq) {
                            Some(ev) => events.push(ev),
                            None => events.push(InputEvent::Data(seq)),
                        }
                        self.state = State::Idle;
                    }
                }
                State::Osc => {
                    if self.seq.len() >= MAX_ESC_SEQ_LEN {
                        self.seq.clear();
                        self.state = State::Idle;
                        continue;
                    }
                    self.seq.push(b);
                    if b == 0x07
                        || (b == 0x5C
                            && self.seq.len() >= 2
                            && self.seq[self.seq.len() - 2] == 0x1B)
                    {
                        events.push(InputEvent::Data(std::mem::take(&mut self.seq)));
                        self.state = State::Idle;
                    }
                }
                State::OtherEsc => {
                    if self.seq.len() >= MAX_ESC_SEQ_LEN {
                        self.seq.clear();
                        self.state = State::Idle;
                        continue;
                    }
                    self.seq.push(b);
                    if b == 0x07
                        || (b == 0x5C
                            && self.seq.len() >= 2
                            && self.seq[self.seq.len() - 2] == 0x1B)
                    {
                        events.push(InputEvent::Data(std::mem::take(&mut self.seq)));
                        self.state = State::Idle;
                    }
                }
            }
        }
        flush(&mut data, &mut events);

        events
    }

    pub fn flush_pending_esc(&mut self) -> Vec<InputEvent> {
        if matches!(self.state, State::EscStart) && !self.seq.is_empty() {
            let seq = std::mem::take(&mut self.seq);
            self.state = State::Idle;
            return vec![InputEvent::Data(seq)];
        }
        Vec::new()
    }

    #[must_use]
    pub fn esc_pending(&self) -> bool {
        matches!(self.state, State::EscStart) && !self.seq.is_empty()
    }
}

pub(crate) const PASTE_START: &[u8] = b"\x1b[200~";
pub(crate) const PASTE_END: &[u8] = b"\x1b[201~";

pub(crate) fn flush(data: &mut Vec<u8>, events: &mut Vec<InputEvent>) {
    if !data.is_empty() {
        events.push(InputEvent::Data(std::mem::take(data)));
    }
}
