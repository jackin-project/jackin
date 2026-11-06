// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::mem;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;
const DEL: u8 = 0x7f;
const C1_LEAD: u8 = 0xc2;
const C1_ST: u8 = 0x9c;
const C1_OSC: u8 = 0x9d;
const C1_CSI: u8 = 0x9b;
const C1_STRINGS: [u8; 4] = [0x90, 0x98, 0x9e, 0x9f];
const MAX_OSC: usize = 4096;
const MAX_CSI: usize = 256;
const MAX_REPLY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Sandbox,
    Operator,
}

#[derive(Clone, Copy, Debug)]
enum Symbol {
    Byte(u8),
    C1 { code: u8, wide: bool },
}

impl Symbol {
    fn push(self, out: &mut Vec<u8>) {
        match self {
            Symbol::Byte(byte) => out.push(byte),
            Symbol::C1 { code, wide } => {
                if wide {
                    out.push(C1_LEAD);
                }
                out.push(code);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decoder {
    Ground,
    Continuation(u8),
    Lead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Terminator {
    Bel,
    St,
}

#[derive(Debug)]
struct Osc {
    body: Vec<u8>,
    dead: bool,
    via_escape: bool,
}

impl Osc {
    fn new(via_escape: bool) -> Self {
        Self {
            body: Vec::new(),
            dead: false,
            via_escape,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.dead {
            return;
        }
        if self.body.len() >= MAX_OSC {
            self.kill();
        } else {
            self.body.push(byte);
        }
    }

    fn kill(&mut self) {
        self.dead = true;
        self.body = Vec::new();
    }
}

#[derive(Debug)]
struct Csi {
    introducer: Symbol,
    body: Vec<u8>,
    dead: bool,
}

impl Csi {
    fn new(introducer: Symbol) -> Self {
        Self {
            introducer,
            body: Vec::new(),
            dead: false,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.dead {
            return;
        }
        if self.body.len() >= MAX_CSI {
            self.dead = true;
            self.body = Vec::new();
        } else {
            self.body.push(byte);
        }
    }
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Ground,
    Escape,
    Osc(Osc),
    OscEscape(Osc),
    Csi(Csi),
    Skip,
    SkipEscape,
}

#[derive(Debug)]
struct Stream {
    direction: Direction,
    decoder: Decoder,
    state: State,
    escape_sent: bool,
}

impl Stream {
    fn new(direction: Direction) -> Self {
        Self {
            direction,
            decoder: Decoder::Ground,
            state: State::Ground,
            escape_sent: false,
        }
    }

    fn filter(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            self.feed(byte, &mut out);
        }
        if self.direction == Direction::Operator {
            self.release(&mut out);
        }
        out
    }

    fn release(&mut self, out: &mut Vec<u8>) {
        let bare_introducer = matches!(
            &self.state,
            State::Osc(osc) if osc.via_escape && osc.body.is_empty() && !osc.dead
        );
        if bare_introducer {
            self.introduce(out);
            self.state = State::Ground;
        } else if matches!(self.state, State::Escape) && !self.escape_sent {
            out.push(ESC);
            self.escape_sent = true;
        }
    }

    fn feed(&mut self, byte: u8, out: &mut Vec<u8>) {
        match self.decoder {
            Decoder::Lead => {
                self.decoder = Decoder::Ground;
                if (0x80..=0x9f).contains(&byte) {
                    self.step(
                        Symbol::C1 {
                            code: byte,
                            wide: true,
                        },
                        out,
                    );
                    return;
                }
                self.step(Symbol::Byte(C1_LEAD), out);
                if (0xa0..=0xbf).contains(&byte) {
                    self.step(Symbol::Byte(byte), out);
                } else {
                    self.feed(byte, out);
                }
            }
            Decoder::Continuation(left) if (0x80..=0xbf).contains(&byte) => {
                self.decoder = if left == 1 {
                    Decoder::Ground
                } else {
                    Decoder::Continuation(left - 1)
                };
                self.step(Symbol::Byte(byte), out);
            }
            Decoder::Ground | Decoder::Continuation(_) => {
                self.decoder = Decoder::Ground;
                match byte {
                    C1_LEAD => self.decoder = Decoder::Lead,
                    0xc3..=0xdf => {
                        self.decoder = Decoder::Continuation(1);
                        self.step(Symbol::Byte(byte), out);
                    }
                    0xe0..=0xef => {
                        self.decoder = Decoder::Continuation(2);
                        self.step(Symbol::Byte(byte), out);
                    }
                    0xf0..=0xf4 => {
                        self.decoder = Decoder::Continuation(3);
                        self.step(Symbol::Byte(byte), out);
                    }
                    0x80..=0x9f => self.step(
                        Symbol::C1 {
                            code: byte,
                            wide: false,
                        },
                        out,
                    ),
                    _ => self.step(Symbol::Byte(byte), out),
                }
            }
        }
    }

    fn step(&mut self, symbol: Symbol, out: &mut Vec<u8>) {
        match mem::take(&mut self.state) {
            State::Ground => self.ground(symbol, out),
            State::Escape => self.escape(symbol, out),
            State::Osc(osc) => self.osc(osc, symbol, out),
            State::OscEscape(osc) => self.osc_escape(osc, symbol, out),
            State::Csi(csi) => self.csi(csi, symbol, out),
            State::Skip => self.skip(symbol),
            State::SkipEscape => self.skip_escape(symbol, out),
        }
    }

    fn filters_sandbox(&self) -> bool {
        self.direction == Direction::Sandbox
    }

    fn ground(&mut self, symbol: Symbol, out: &mut Vec<u8>) {
        match symbol {
            Symbol::Byte(ESC) => self.state = State::Escape,
            Symbol::C1 { code: C1_OSC, .. } => self.state = State::Osc(Osc::new(false)),
            Symbol::C1 { code: C1_CSI, .. } if self.filters_sandbox() => {
                self.state = State::Csi(Csi::new(symbol));
            }
            Symbol::C1 { code, .. } if self.filters_sandbox() && C1_STRINGS.contains(&code) => {
                self.state = State::Skip;
            }
            other => other.push(out),
        }
    }

    fn escape(&mut self, symbol: Symbol, out: &mut Vec<u8>) {
        match symbol {
            Symbol::Byte(b']') => self.state = State::Osc(Osc::new(true)),
            Symbol::C1 { code: C1_OSC, .. } => self.state = State::Osc(Osc::new(false)),
            Symbol::Byte(b'[') if self.filters_sandbox() => {
                self.state = State::Csi(Csi::new(Symbol::Byte(ESC)));
            }
            Symbol::C1 { code: C1_CSI, .. } if self.filters_sandbox() => {
                self.state = State::Csi(Csi::new(symbol));
            }
            Symbol::Byte(b'P' | b'X' | b'^' | b'_') if self.filters_sandbox() => {
                self.state = State::Skip;
            }
            Symbol::C1 { code, .. } if self.filters_sandbox() && C1_STRINGS.contains(&code) => {
                self.state = State::Skip;
            }
            Symbol::Byte(ESC) => self.state = State::Escape,
            Symbol::Byte(CAN | SUB) => self.escape_sent = false,
            Symbol::Byte(DEL) => self.state = State::Escape,
            Symbol::Byte(control) if control < 0x20 => {
                out.push(control);
                self.state = State::Escape;
            }
            other => {
                self.introduce_escape(out);
                other.push(out);
            }
        }
    }

    fn osc(&mut self, mut osc: Osc, symbol: Symbol, out: &mut Vec<u8>) {
        match symbol {
            Symbol::Byte(BEL) => self.finish(&osc, Terminator::Bel, out),
            Symbol::Byte(ESC) => self.state = State::OscEscape(osc),
            Symbol::Byte(CAN | SUB) => self.escape_sent = false,
            Symbol::C1 { code: C1_ST, .. } => self.finish(&osc, Terminator::St, out),
            Symbol::C1 { code: C1_OSC, .. } => self.state = State::Osc(Osc::new(false)),
            Symbol::C1 { .. } => {
                osc.kill();
                self.state = State::Osc(osc);
            }
            Symbol::Byte(byte) if byte < 0x20 || byte == DEL => {
                osc.kill();
                self.state = State::Osc(osc);
            }
            Symbol::Byte(byte) => {
                osc.push(byte);
                self.state = State::Osc(osc);
            }
        }
    }

    fn csi(&mut self, mut csi: Csi, symbol: Symbol, out: &mut Vec<u8>) {
        match symbol {
            Symbol::Byte(ESC) => self.state = State::Escape,
            Symbol::Byte(CAN | SUB) => {}
            Symbol::Byte(byte @ 0x20..=0x3f) => {
                csi.push(byte);
                self.state = State::Csi(csi);
            }
            Symbol::Byte(byte @ 0x40..=0x7e) => {
                if byte != b't' && !csi.dead {
                    csi.introducer.push(out);
                    if matches!(csi.introducer, Symbol::Byte(_)) {
                        out.push(b'[');
                    }
                    out.extend_from_slice(&csi.body);
                    out.push(byte);
                }
            }
            Symbol::Byte(DEL) => self.state = State::Csi(csi),
            Symbol::Byte(control) if control < 0x20 => {
                out.push(control);
                self.state = State::Csi(csi);
            }
            other => self.ground(other, out),
        }
    }

    fn skip(&mut self, symbol: Symbol) {
        match symbol {
            Symbol::Byte(ESC) => self.state = State::SkipEscape,
            Symbol::Byte(CAN | SUB) => {}
            Symbol::C1 { code: C1_ST, .. } => {}
            _ => self.state = State::Skip,
        }
    }

    fn skip_escape(&mut self, symbol: Symbol, out: &mut Vec<u8>) {
        match symbol {
            Symbol::Byte(b'\\') => {}
            other => self.escape(other, out),
        }
    }

    fn osc_escape(&mut self, osc: Osc, symbol: Symbol, out: &mut Vec<u8>) {
        match symbol {
            Symbol::Byte(b'\\') => self.finish(&osc, Terminator::St, out),
            other => {
                self.escape_sent = false;
                self.escape(other, out);
            }
        }
    }

    fn finish(&mut self, osc: &Osc, terminator: Terminator, out: &mut Vec<u8>) {
        if !osc.dead && allowed(self.direction, &osc.body) {
            self.introduce(out);
            out.extend_from_slice(&osc.body);
            match terminator {
                Terminator::Bel => out.push(BEL),
                Terminator::St => out.extend_from_slice(&[ESC, b'\\']),
            }
        }
        self.escape_sent = false;
    }

    fn introduce_escape(&mut self, out: &mut Vec<u8>) {
        if !mem::take(&mut self.escape_sent) {
            out.push(ESC);
        }
    }

    fn introduce(&mut self, out: &mut Vec<u8>) {
        self.introduce_escape(out);
        out.push(b']');
    }
}

fn allowed(direction: Direction, body: &[u8]) -> bool {
    let Some(split) = body.iter().position(|&byte| byte == b';') else {
        return false;
    };
    let (number, rest) = (&body[..split], &body[split + 1..]);
    let Ok(text) = str::from_utf8(rest) else {
        return false;
    };
    if text.chars().any(char::is_control) {
        return false;
    }
    match (direction, number) {
        (Direction::Sandbox, b"0" | b"1" | b"2" | b"8") => true,
        (Direction::Sandbox, b"10" | b"11") => text == "?",
        (Direction::Operator, b"10" | b"11") => {
            text.len() <= MAX_REPLY
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'/'))
        }
        _ => false,
    }
}

#[derive(Debug)]
pub struct ClipboardGuard {
    sandbox: Stream,
    operator: Stream,
}

impl Default for ClipboardGuard {
    fn default() -> Self {
        Self {
            sandbox: Stream::new(Direction::Sandbox),
            operator: Stream::new(Direction::Operator),
        }
    }
}

impl ClipboardGuard {
    pub fn from_sandbox(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.sandbox.filter(bytes)
    }

    pub fn from_operator(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.operator.filter(bytes)
    }
}
