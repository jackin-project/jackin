/// Input from the attached client terminal: prefix-key state machine.
///
/// The parser walks raw bytes from the client and classifies them into
/// three categories of `InputEvent`:
///   - `Data` — forward verbatim to the focused pane's PTY.
///   - `PrefixCommand` — a tmux-style action the multiplexer handles.
///   - `MousePress` — SGR mouse, hit-tested by the daemon.
///
/// Default prefix is `Ctrl+B` (`0x02`), matching tmux. The prefix byte
/// itself is forwarded to the PTY only when typed twice (`prefix + prefix`
/// → one literal prefix byte). `Ctrl+J` (`0x0A`) is reserved as the line
/// feed character and is never a default binding — that collision is the
/// regression Phase 3b is designed to prevent.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Data(Vec<u8>),
    MousePress { col: u16, row: u16, button: u8 },
    PrefixCommand(PrefixCommand),
    FocusIn,
    FocusOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixCommand {
    NewTab,
    NextTab,
    PrevTab,
    JumpTab(usize),
    SplitTopBottom,
    SplitSideBySide,
    MoveFocus(ArrowDir),
    ZoomToggle,
    KillPane,
    KillTab,
    Detach,
    Palette,
    Redraw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug)]
pub struct InputParser {
    prefix: u8,
    state: State,
    seq: Vec<u8>,
    in_paste: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    Idle,
    PrefixAwait,
    EscStart,
    Csi,
    Osc,
    OtherEsc,
}

impl Default for InputParser {
    fn default() -> Self {
        Self::new(default_prefix())
    }
}

impl InputParser {
    pub fn new(prefix: u8) -> Self {
        Self {
            prefix,
            state: State::Idle,
            seq: Vec::new(),
            in_paste: false,
        }
    }

    /// Parse a chunk of client bytes into a stream of events.
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
                    if b == self.prefix {
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
                    if b == self.prefix {
                        // Literal prefix forwarded to PTY.
                        data.push(self.prefix);
                    } else if let Some(cmd) = prefix_binding(b) {
                        events.push(InputEvent::PrefixCommand(cmd));
                    }
                    // Always return to Idle after one key.
                    self.state = State::Idle;
                }
                State::EscStart => {
                    self.seq.push(b);
                    match b {
                        b'[' => self.state = State::Csi,
                        b']' => self.state = State::Osc,
                        b'P' | b'_' | b'X' | b'^' => self.state = State::OtherEsc,
                        _ => {
                            // ESC + single byte sequences (e.g. ESC O X = SS3).
                            // Emit and return to Idle.
                            events.push(InputEvent::Data(std::mem::take(&mut self.seq)));
                            self.state = State::Idle;
                        }
                    }
                }
                State::Csi => {
                    self.seq.push(b);
                    if matches!(b, 0x40..=0x7E) {
                        // Final byte; classify the sequence.
                        let seq = std::mem::take(&mut self.seq);
                        if seq == PASTE_START {
                            // Forward the start marker; treat following bytes
                            // as paste content until PASTE_END arrives.
                            events.push(InputEvent::Data(seq));
                            self.in_paste = true;
                        } else if let Some(ev) = classify_csi(&seq) {
                            events.push(ev);
                        } else {
                            events.push(InputEvent::Data(seq));
                        }
                        self.state = State::Idle;
                    }
                }
                State::Osc => {
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
}

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

fn flush(data: &mut Vec<u8>, events: &mut Vec<InputEvent>) {
    if !data.is_empty() {
        events.push(InputEvent::Data(std::mem::take(data)));
    }
}

fn default_prefix() -> u8 {
    if let Ok(s) = std::env::var("JACKIN_PREFIX")
        && let Some(b) = parse_prefix(&s)
    {
        return b;
    }
    0x02 // Ctrl+B
}

/// Accept `C-a` … `C-z` (case-insensitive), a single ASCII control char
/// in hex form `0xNN`, or a single literal byte. Returns `None` on parse
/// error so the caller falls back to the default.
pub fn parse_prefix(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("C-").or_else(|| s.strip_prefix("c-")) {
        let c = rest.chars().next()?;
        if c.is_ascii_alphabetic() {
            let upper = c.to_ascii_uppercase() as u8;
            return Some(upper - b'A' + 1);
        }
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u8::from_str_radix(hex, 16).ok();
    }
    if s.len() == 1 {
        return Some(s.as_bytes()[0]);
    }
    None
}

fn prefix_binding(b: u8) -> Option<PrefixCommand> {
    use PrefixCommand::*;
    Some(match b {
        b'c' => NewTab,
        b'n' => NextTab,
        b'p' => PrevTab,
        d @ b'0'..=b'9' => JumpTab((d - b'0') as usize),
        b'"' => SplitTopBottom,
        b'%' => SplitSideBySide,
        b'h' => MoveFocus(ArrowDir::Left),
        b'j' => MoveFocus(ArrowDir::Down),
        b'k' => MoveFocus(ArrowDir::Up),
        b'l' => MoveFocus(ArrowDir::Right),
        b'z' => ZoomToggle,
        b'x' => KillPane,
        b'&' => KillTab,
        b'd' => Detach,
        b' ' | b':' => Palette,
        b'r' => Redraw,
        _ => return None,
    })
}

/// Decode a complete CSI sequence into a higher-level event when we
/// recognise it. Returns `None` to forward the bytes verbatim.
fn classify_csi(seq: &[u8]) -> Option<InputEvent> {
    // Focus in / out.
    if seq == b"\x1b[I" {
        return Some(InputEvent::FocusIn);
    }
    if seq == b"\x1b[O" {
        return Some(InputEvent::FocusOut);
    }
    // Arrow keys: ESC [ A/B/C/D — *not* intercepted; forwarded to PTY.
    // SGR mouse: ESC [ < ... M/m
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
            let button = p[0] as u8;
            let col = (p[1] as u16).saturating_sub(1);
            let row = (p[2] as u16).saturating_sub(1);
            if *final_byte == b'M' {
                return Some(InputEvent::MousePress { col, row, button });
            }
            // Mouse release — forward as-is so the agent's mouse handler
            // sees the matching `m` for its own state.
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(input: &[u8]) -> Vec<InputEvent> {
        InputParser::default().parse(input)
    }

    #[test]
    fn lone_lf_is_forwarded_to_pty() {
        // Regression for the `Ctrl+J = 0x0A` palette intercept that ate
        // every newline in the input stream.
        let events = parse_all(b"\n");
        assert_eq!(events, vec![InputEvent::Data(b"\n".to_vec())]);
    }

    #[test]
    fn pasted_text_with_lf_survives_intact() {
        let events = parse_all(b"hello\nworld\n");
        // Single Data event with full bytes.
        assert_eq!(events.len(), 1);
        match &events[0] {
            InputEvent::Data(b) => assert_eq!(b, b"hello\nworld\n"),
            _ => panic!("expected Data, got {events:?}"),
        }
    }

    #[test]
    fn lone_prefix_is_consumed() {
        let events = parse_all(b"\x02");
        assert!(
            events.is_empty(),
            "lone prefix must not emit any event: {events:?}"
        );
    }

    #[test]
    fn double_prefix_forwards_one_literal() {
        let events = parse_all(b"\x02\x02");
        assert_eq!(events, vec![InputEvent::Data(vec![0x02])]);
    }

    #[test]
    fn prefix_c_opens_new_tab() {
        let events = parse_all(b"\x02c");
        assert_eq!(
            events,
            vec![InputEvent::PrefixCommand(PrefixCommand::NewTab)]
        );
    }

    #[test]
    fn prefix_space_opens_palette() {
        let events = parse_all(b"\x02 ");
        assert_eq!(
            events,
            vec![InputEvent::PrefixCommand(PrefixCommand::Palette)]
        );
    }

    #[test]
    fn prefix_d_detaches() {
        let events = parse_all(b"\x02d");
        assert_eq!(
            events,
            vec![InputEvent::PrefixCommand(PrefixCommand::Detach)]
        );
    }

    #[test]
    fn bracketed_paste_contents_are_forwarded_with_markers() {
        let mut parser = InputParser::default();
        let mut events = parser.parse(b"\x1b[200~hello\x02world\n\x1b[201~");
        // Expect: start marker as Data, body as Data, end marker as Data.
        // The prefix byte INSIDE paste must not be intercepted.
        events.retain(|e| !matches!(e, InputEvent::Data(b) if b.is_empty()));
        let combined: Vec<u8> = events
            .iter()
            .flat_map(|e| match e {
                InputEvent::Data(b) => b.clone(),
                _ => Vec::new(),
            })
            .collect();
        assert_eq!(combined, b"\x1b[200~hello\x02world\n\x1b[201~");
    }

    #[test]
    fn arrow_key_csi_passes_through() {
        let events = parse_all(b"\x1b[A");
        match &events[..] {
            [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[A"),
            other => panic!("unexpected events {other:?}"),
        }
    }

    #[test]
    fn shift_enter_csi_u_round_trips() {
        // CSI-u extended-keys encoding: `\x1b[13;2u` = Shift+Enter.
        let events = parse_all(b"\x1b[13;2u");
        match &events[..] {
            [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[13;2u"),
            other => panic!("Shift+Enter must round-trip: {other:?}"),
        }
    }

    #[test]
    fn focus_event_is_classified() {
        let events = parse_all(b"\x1b[I");
        assert_eq!(events, vec![InputEvent::FocusIn]);
        let events = parse_all(b"\x1b[O");
        assert_eq!(events, vec![InputEvent::FocusOut]);
    }

    #[test]
    fn sgr_mouse_press_is_decoded() {
        let events = parse_all(b"\x1b[<0;5;3M");
        assert_eq!(
            events,
            vec![InputEvent::MousePress {
                col: 4,
                row: 2,
                button: 0
            }]
        );
    }

    #[test]
    fn parse_prefix_forms() {
        assert_eq!(parse_prefix("C-a"), Some(0x01));
        assert_eq!(parse_prefix("C-b"), Some(0x02));
        assert_eq!(parse_prefix("c-z"), Some(0x1A));
        assert_eq!(parse_prefix("0x02"), Some(0x02));
        assert_eq!(parse_prefix("0X1B"), Some(0x1B));
        assert_eq!(parse_prefix("Q"), Some(b'Q'));
        assert_eq!(parse_prefix("nope"), None);
    }
}
