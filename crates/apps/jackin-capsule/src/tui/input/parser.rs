// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Byte-stream parser state machine: `InputParser` states and dispatch.

use super::{
    CsiClassification, InputBindings, InputEvent, classify_csi, classify_x10_mouse, prefix_binding,
};

#[derive(Debug)]
pub struct InputParser {
    /// Optional tmux-style prefix byte. `None` disables prefix mode.
    prefix: Option<u8>,
    /// Optional one-key palette shortcut. `None` disables direct palette.
    palette_key: Option<u8>,
    state: State,
    seq: Vec<u8>,
    in_paste: bool,
}

/// Cap on the in-progress CSI/OSC/SS3/OtherEsc sequence buffer. The
/// parser is stateful across `parse()` calls — an attacker (or operator
/// pasting malformed terminal output) could otherwise stream
/// `\x1b[` followed by megabytes of parameter bytes across many input
/// frames without ever sending the terminator byte, growing `self.seq`
/// unboundedly. 16 KiB is well above the largest legitimate terminal
/// escape (kitty graphics OSC payloads top out around 4 KiB chunks).
/// When the cap is hit we drop the in-flight sequence and reset to
/// Idle so a subsequent well-formed sequence resyncs cleanly.
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
    /// SS3 — `\x1b O <final>`. Application-cursor-keys mode (DEC `?1`)
    /// makes arrow keys emit SS3 sequences instead of the CSI form,
    /// and every modern agent enables that mode. Without recognising
    /// SS3 atomically the parser splits the sequence into two `Data`
    /// events and dialogs that match the 3-byte form never see the
    /// arrow.
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

    /// `true` while the parser is between the prefix byte and its
    /// next command key. Exposed so UI layers can react to prefix
    /// state without peeking into the parser state machine.
    #[must_use]
    pub fn is_awaiting_prefix(&self) -> bool {
        matches!(self.state, State::PrefixAwait)
    }

    /// Whether the prefix-mode (`Ctrl+B …`) is active. Affects the
    /// status-bar hint format.
    #[must_use]
    pub fn prefix_enabled(&self) -> bool {
        self.prefix.is_some()
    }

    /// The resolved palette-key byte, or `None` when palette mode is disabled.
    /// Used by the hint builder to render the correct key glyph when the
    /// operator has overridden `JACKIN_PALETTE_KEY`.
    #[must_use]
    pub fn palette_key(&self) -> Option<u8> {
        self.palette_key
    }

    /// Parse a chunk of client bytes into a stream of events.
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
                        // Default `Ctrl+\` (or configured key) →
                        // immediate palette open. Bracketed paste
                        // already excluded above; operators needing
                        // a literal palette byte set
                        // `JACKIN_PALETTE_KEY=none`.
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
                    // If the byte right after `ESC` is the palette
                    // shortcut, the operator's likely intent is
                    // "dismiss whatever was open, then open the
                    // menu." Discard the buffered `ESC` and fire
                    // OpenPalette so the menu opens reliably even
                    // when the two bytes arrive in the same chunk
                    // (rapid keystrokes, or after a dialog
                    // dismissed via `Esc`).
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
                            // ESC + single byte sequences that aren't
                            // CSI / OSC / SS3 / DCS. Emit and return.
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
                        // Final byte; classify the sequence.
                        let seq = std::mem::take(&mut self.seq);
                        if seq == PASTE_START {
                            // Forward the start marker; treat following bytes
                            // as paste content until PASTE_END arrives.
                            events.push(InputEvent::Data(seq));
                            self.in_paste = true;
                        } else {
                            // classify_csi returns an explicit "drop this
                            // sequence" outcome via Suppress so kitty
                            // key-release events (and any future
                            // suppress-class CSI) never reach the agent
                            // or the dialog as garbage Data bytes.
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
        // Note: an unfinished `\x1b` in `EscStart` is NOT flushed at
        // end of chunk. Doing so split `ESC [ A` across two TCP
        // chunks into a lone Esc + a stray `[A`, breaking arrow
        // keys under any pasting / slow link. The daemon arms an
        // escape-timeout timer (default 50 ms) instead — see
        // `Self::esc_pending` / `Self::flush_pending_esc`.
        events
    }

    /// Best-effort drain for a buffered `EscStart` that did not
    /// complete within the operator's escape-time. Emits the lone
    /// `\x1b` as a `Data` event and returns to `Idle` so dismiss-on-
    /// Esc works in dialogs and the agent receives the bare Esc the
    /// operator actually pressed.
    pub fn flush_pending_esc(&mut self) -> Vec<InputEvent> {
        if matches!(self.state, State::EscStart) && !self.seq.is_empty() {
            let seq = std::mem::take(&mut self.seq);
            self.state = State::Idle;
            return vec![InputEvent::Data(seq)];
        }
        Vec::new()
    }

    /// Whether the parser is mid-escape and the daemon should arm an
    /// `escape-time` timer. Cleared after `flush_pending_esc`.
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
