/// Input from the attached client terminal.
///
/// Two parallel models are supported:
///
/// - **Palette key (default `Ctrl+J`)** — one keystroke opens the
///   command palette and the operator picks an action from a list,
///   launcher-style. This is the primary UX and the only model the
///   default status-bar hint advertises. Collision-safe because raw-mode
///   `Enter` sends `\r` (not `\n`), bracketed paste protects pasted `\n`,
///   and a literal `Ctrl+J Ctrl+J` forwards one LF byte to the PTY.
///
/// - **Prefix key (opt-in via `JACKIN_PREFIX=C-b`)** — tmux-style
///   prefix + command-key for operators who prefer direct keyboard
///   navigation. Disabled by default.
///
/// Both models can run simultaneously when both env vars are set.
/// `JACKIN_PALETTE_KEY=none` disables the palette key.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Data(Vec<u8>),
    MousePress {
        col: u16,
        row: u16,
        button: u8,
    },
    PrefixCommand(PrefixCommand),
    /// Direct one-key shortcut → open the palette dialog. Distinct from
    /// `PrefixCommand::Palette`, which fires only after the prefix
    /// gesture; the daemon collapses both into the same dialog open.
    OpenPalette,
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
    /// Optional tmux-style prefix byte. `None` disables prefix mode.
    prefix: Option<u8>,
    /// Optional one-key palette shortcut. `None` disables direct palette.
    palette_key: Option<u8>,
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
        Self::new(default_prefix(), default_palette_key())
    }
}

impl InputParser {
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
    /// next command key. Used by the status bar to swap the right-side
    /// hint to `prefix…` for the duration of the prefix gesture.
    pub fn is_awaiting_prefix(&self) -> bool {
        matches!(self.state, State::PrefixAwait)
    }

    /// Whether the prefix-mode (`Ctrl+B …`) is active. Affects the
    /// status-bar hint format.
    pub fn prefix_enabled(&self) -> bool {
        self.prefix.is_some()
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
                    if Some(b) == self.palette_key {
                        // Ctrl+J (or configured key) → immediate palette
                        // open. The collision risk with literal LF is
                        // mitigated by bracketed paste (paste content
                        // protected) and by raw-mode Enter sending `\r`
                        // not `\n`. Operators needing literal LF set
                        // `JACKIN_PALETTE_KEY=none`.
                        flush(&mut data, &mut events);
                        events.push(InputEvent::OpenPalette);
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
        // Lone Esc — a single `\x1b` byte with no following sequence
        // byte — must reach the dialog layer as a `Data` event so
        // dismiss-on-Esc works. Without this flush the parser stays
        // in `EscStart` indefinitely and `\x1b` is buffered forever.
        // Multi-byte CSI / OSC / DCS sequences are still buffered
        // across chunks because their state is `Csi` / `Osc` /
        // `OtherEsc`, not `EscStart`.
        if matches!(self.state, State::EscStart) && !self.seq.is_empty() {
            events.push(InputEvent::Data(std::mem::take(&mut self.seq)));
            self.state = State::Idle;
        }
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

/// Prefix mode is **opt-in**: returns `Some(byte)` when `JACKIN_PREFIX`
/// is set to a parseable key, `None` otherwise. The default
/// `Ctrl+J` palette key is the primary UX.
fn default_prefix() -> Option<u8> {
    let s = std::env::var("JACKIN_PREFIX").ok()?;
    if s.eq_ignore_ascii_case("none") {
        return None;
    }
    parse_prefix(&s)
}

/// Palette key defaults to `Ctrl+J` (`0x0A`). Set `JACKIN_PALETTE_KEY`
/// to override; set it to the literal string `none` to disable the
/// direct-palette shortcut entirely.
fn default_palette_key() -> Option<u8> {
    match std::env::var("JACKIN_PALETTE_KEY") {
        Err(_) => Some(0x0A),
        Ok(s) if s.eq_ignore_ascii_case("none") => None,
        Ok(s) => parse_prefix(&s).or(Some(0x0A)),
    }
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

    fn parse_all_default(input: &[u8]) -> Vec<InputEvent> {
        InputParser::default().parse(input)
    }

    fn parse_all_prefix_only(input: &[u8]) -> Vec<InputEvent> {
        InputParser::new(Some(0x02), None).parse(input)
    }

    #[test]
    fn ctrl_j_opens_palette_by_default() {
        let events = parse_all_default(b"\n");
        assert_eq!(events, vec![InputEvent::OpenPalette]);
    }

    #[test]
    fn palette_key_disabled_lets_lf_through() {
        let events = InputParser::new(None, None).parse(b"\n");
        assert_eq!(events, vec![InputEvent::Data(b"\n".to_vec())]);
    }

    #[test]
    fn pasted_text_with_lf_does_not_open_palette() {
        // Bracketed paste protects the LF inside paste content.
        let mut parser = InputParser::default();
        let events = parser.parse(b"\x1b[200~hello\nworld\n\x1b[201~");
        let opens = events
            .iter()
            .filter(|e| matches!(e, InputEvent::OpenPalette))
            .count();
        assert_eq!(opens, 0, "palette must not open inside bracketed paste");
    }

    #[test]
    fn lone_prefix_is_consumed_when_prefix_enabled() {
        let events = parse_all_prefix_only(b"\x02");
        assert!(
            events.is_empty(),
            "lone prefix must not emit any event: {events:?}"
        );
    }

    #[test]
    fn double_prefix_forwards_one_literal() {
        let events = parse_all_prefix_only(b"\x02\x02");
        assert_eq!(events, vec![InputEvent::Data(vec![0x02])]);
    }

    #[test]
    fn prefix_c_opens_new_tab() {
        let events = parse_all_prefix_only(b"\x02c");
        assert_eq!(
            events,
            vec![InputEvent::PrefixCommand(PrefixCommand::NewTab)]
        );
    }

    #[test]
    fn prefix_space_opens_palette() {
        let events = parse_all_prefix_only(b"\x02 ");
        assert_eq!(
            events,
            vec![InputEvent::PrefixCommand(PrefixCommand::Palette)]
        );
    }

    #[test]
    fn prefix_d_detaches() {
        let events = parse_all_prefix_only(b"\x02d");
        assert_eq!(
            events,
            vec![InputEvent::PrefixCommand(PrefixCommand::Detach)]
        );
    }

    #[test]
    fn bracketed_paste_contents_are_forwarded_with_markers() {
        let mut parser = InputParser::new(Some(0x02), None);
        let mut events = parser.parse(b"\x1b[200~hello\x02world\n\x1b[201~");
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
        let events = parse_all_default(b"\x1b[A");
        match &events[..] {
            [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[A"),
            other => panic!("unexpected events {other:?}"),
        }
    }

    #[test]
    fn shift_enter_csi_u_round_trips() {
        // CSI-u extended-keys encoding: `\x1b[13;2u` = Shift+Enter.
        let events = parse_all_default(b"\x1b[13;2u");
        match &events[..] {
            [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[13;2u"),
            other => panic!("Shift+Enter must round-trip: {other:?}"),
        }
    }

    #[test]
    fn focus_event_is_classified() {
        let events = parse_all_default(b"\x1b[I");
        assert_eq!(events, vec![InputEvent::FocusIn]);
        let events = parse_all_default(b"\x1b[O");
        assert_eq!(events, vec![InputEvent::FocusOut]);
    }

    #[test]
    fn sgr_mouse_press_is_decoded() {
        let events = parse_all_default(b"\x1b[<0;5;3M");
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
