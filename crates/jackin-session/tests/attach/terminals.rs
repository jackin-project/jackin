// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, Osc52, Term};
use alacritty_terminal::vte::ansi::Processor;
use jackin_session::ClipboardGuard;
use termwiz::escape::Action;
use termwiz::escape::osc::OperatingSystemCommand;
use termwiz::escape::parser::Parser;

const BEL: u8 = 0x07;
const ST: &[u8] = b"\x1b\\";
const PAYLOADS: [&[u8]; 3] = [b";c;SGVsbG8=", b";c;?", b";p;QUJD"];

#[derive(Clone, Default)]
struct Listener(Arc<AtomicUsize>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        if matches!(event, Event::ClipboardStore(..) | Event::ClipboardLoad(..)) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

struct Screen;

impl Dimensions for Screen {
    fn total_lines(&self) -> usize {
        24
    }
    fn screen_lines(&self) -> usize {
        24
    }
    fn columns(&self) -> usize {
        80
    }
}

fn alacritty(bytes: &[u8]) -> usize {
    let listener = Listener::default();
    let config = Config {
        osc52: Osc52::CopyPaste,
        ..Config::default()
    };
    let mut term = Term::new(config, &Screen, listener.clone());
    let mut processor: Processor = Processor::new();
    processor.advance(&mut term, bytes);
    listener.0.load(Ordering::SeqCst)
}

fn termwiz(bytes: &[u8]) -> usize {
    Parser::new()
        .parse_as_vec(bytes)
        .into_iter()
        .filter(|action| {
            matches!(
                action,
                Action::OperatingSystemCommand(command)
                    if matches!(
                        **command,
                        OperatingSystemCommand::SetSelection(..)
                            | OperatingSystemCommand::QuerySelection(..)
                            | OperatingSystemCommand::ClearSelection(..)
                    )
            )
        })
        .count()
}

fn events(bytes: &[u8]) -> (usize, usize) {
    (alacritty(bytes), termwiz(bytes))
}

fn introducers() -> Vec<Vec<u8>> {
    vec![
        b"\x1b]".to_vec(),
        b"\x1b\x7f]".to_vec(),
        b"\x1b\x00]".to_vec(),
        b"\x1b\x07]".to_vec(),
        b"\x1b\x0b]".to_vec(),
        b"\x1b\x1b]".to_vec(),
        b"\x9d".to_vec(),
        b"\xc2\x9d".to_vec(),
        b"\x1b\x18\x1b]".to_vec(),
        b"\x1b[1\x1b]".to_vec(),
        b"\x1bP1\x1b]".to_vec(),
        b"\x1b_x\x1b]".to_vec(),
        b"\x1b(\x1b]".to_vec(),
    ]
}

fn numbers() -> Vec<Vec<u8>> {
    let mut all = vec![b"52".to_vec(), b"052".to_vec(), b"0052".to_vec()];
    for c0 in (0x00..=0x06_u8)
        .chain(0x08..=0x17)
        .chain([0x19, 0x1c, 0x1d, 0x1e, 0x1f, 0x7f])
    {
        all.push(vec![b'5', c0, b'2']);
        all.push(vec![c0, b'5', b'2']);
    }
    all
}

fn terminators() -> Vec<Vec<u8>> {
    vec![
        vec![BEL],
        ST.to_vec(),
        vec![0x9c],
        vec![0xc2, 0x9c],
        vec![0x18],
        vec![0x1a],
        b"\x1bx".to_vec(),
        Vec::new(),
    ]
}

fn hostile() -> Vec<Vec<u8>> {
    let mut all = Vec::new();
    for introducer in introducers() {
        for number in numbers() {
            for payload in PAYLOADS {
                for terminator in terminators() {
                    all.push(
                        [
                            b"ab".as_slice(),
                            &introducer,
                            &number,
                            payload,
                            &terminator,
                            b"cd".as_slice(),
                        ]
                        .concat(),
                    );
                }
            }
        }
    }
    all.push(b"\x1bPtmux;\x1b\x1b]52;c;SGVsbG8=\x07\x1b\\".to_vec());
    all.push(b"\x1b]0;t\x07\x1b]52;c;SGVsbG8=\x07\x1b]2;u\x07".to_vec());
    all.push(b"\x1b]52;c;QUJD\x1b]52;c;SGVsbG8=\x07".to_vec());
    for lead in [0xe2_u8, 0xc3, 0xf0, 0xe0] {
        all.push([&[lead, 0x9d][..], b"52;c;SGVsbG8=\x07".as_slice()].concat());
        all.push([&[lead][..], b"\x9d52;c;SGVsbG8=\x07".as_slice()].concat());
    }
    all
}

fn through(guard: &mut ClipboardGuard, bytes: &[u8], at: usize) -> Vec<u8> {
    let mut out = guard.from_sandbox(&bytes[..at]);
    out.extend(guard.from_sandbox(&bytes[at..]));
    out
}

#[test]
fn the_corpus_really_reaches_the_clipboard_of_both_parsers_unfiltered() {
    let (mut alacritty_hits, mut termwiz_hits) = (0, 0);
    for input in hostile() {
        let (a, t) = events(&input);
        alacritty_hits += usize::from(a > 0);
        termwiz_hits += usize::from(t > 0);
    }
    assert!(alacritty_hits > 100, "alacritty stored {alacritty_hits}");
    assert!(termwiz_hits > 100, "termwiz stored {termwiz_hits}");
}

#[test]
fn no_filtered_sandbox_output_reaches_the_clipboard_of_alacritty_or_wezterm() {
    for input in hostile() {
        let mut guard = ClipboardGuard::default();
        let mut filtered = guard.from_sandbox(&input);
        filtered.extend(guard.from_sandbox(b"\x07z"));
        assert_eq!(events(&filtered), (0, 0), "{input:?} became {filtered:?}");
    }
}

#[test]
fn a_hostile_sequence_split_at_every_byte_is_still_harmless() {
    for input in hostile().into_iter().step_by(7) {
        for at in 0..=input.len() {
            let mut guard = ClipboardGuard::default();
            let mut filtered = through(&mut guard, &input, at);
            filtered.extend(guard.from_sandbox(b"\x07z"));
            assert_eq!(
                events(&filtered),
                (0, 0),
                "{input:?} split at {at} became {filtered:?}"
            );
        }
    }
}

#[test]
fn no_filtered_operator_input_reaches_the_clipboard_of_alacritty_or_wezterm() {
    for input in hostile() {
        let mut guard = ClipboardGuard::default();
        let mut filtered = guard.from_operator(&input);
        filtered.extend(guard.from_operator(b"\x07z"));
        assert_eq!(events(&filtered), (0, 0), "{input:?} became {filtered:?}");
    }
}

#[test]
fn two_hostile_sequences_in_a_row_leave_the_second_one_dead_too() {
    let two = [
        b"\x1b\x7f]52;c;QUJD\x07".as_slice(),
        b"\x1b]52;c;SGVsbG8=\x07".as_slice(),
    ]
    .concat();
    let mut guard = ClipboardGuard::default();
    let filtered = guard.from_sandbox(&two);
    assert_eq!(events(&filtered), (0, 0), "{filtered:?}");
}
