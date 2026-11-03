// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const OSC52: &[u8] = b"\x1b]52;";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Prefix {
        matched: usize,
        sent: usize,
    },
    Body,
    BodyEscape,
}

#[derive(Debug, Default)]
struct Stream {
    state: State,
}

impl Stream {
    fn filter(&mut self, bytes: &[u8], release_lone_escape: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            self.step(byte, &mut out);
        }
        if release_lone_escape
            && let State::Prefix {
                matched: 1,
                sent: 0,
            } = self.state
        {
            out.push(ESC);
            self.state = State::Prefix {
                matched: 1,
                sent: 1,
            };
        }
        out
    }

    fn step(&mut self, byte: u8, out: &mut Vec<u8>) {
        match self.state {
            State::Ground if byte == ESC => {
                self.state = State::Prefix {
                    matched: 1,
                    sent: 0,
                };
            }
            State::Ground => out.push(byte),
            State::Prefix { matched, sent } if byte == OSC52[matched] => {
                self.state = if matched + 1 == OSC52.len() {
                    State::Body
                } else {
                    State::Prefix {
                        matched: matched + 1,
                        sent,
                    }
                };
            }
            State::Prefix { matched, sent } => {
                out.extend_from_slice(&OSC52[sent..matched]);
                self.state = State::Ground;
                self.step(byte, out);
            }
            State::Body => {
                self.state = match byte {
                    BEL => State::Ground,
                    ESC => State::BodyEscape,
                    _ => State::Body,
                };
            }
            State::BodyEscape => {
                self.state = State::Ground;
                if byte != b'\\' {
                    self.step(ESC, out);
                    self.step(byte, out);
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ClipboardGuard {
    sandbox: Stream,
    operator: Stream,
}

impl ClipboardGuard {
    pub fn from_sandbox(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.sandbox.filter(bytes, false)
    }

    pub fn from_operator(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.operator.filter(bytes, true)
    }
}
