/// Input from the attached client terminal.
///
/// Two parallel models are supported:
///
/// - **Palette key (default `Ctrl+\`)** — one keystroke opens the
///   command palette and the operator picks an action from a list,
///   launcher-style. This is the primary UX and the only model the
///   default status-bar hint advertises. `Ctrl+\` is the byte `0x1C`
///   — no agent uses it as an editing key, it never appears in agent
///   output, and raw-mode terminals never emit it as content (the
///   `SIGQUIT` semantic only applies in cooked mode).
///
/// - **Prefix key (opt-in via `JACKIN_PREFIX=C-b`)** — tmux-style
///   prefix + command-key for operators who prefer direct keyboard
///   navigation. Disabled by default.
///
/// Both models can run simultaneously when both env vars are set.
/// `JACKIN_PALETTE_KEY=none` disables the palette key entirely.
/// `JACKIN_PALETTE_KEY=C-j` binds the palette to `Ctrl+J`, which is
/// the same byte multi-line agents and shells use as line-continuation
/// — so the bind collides with editing in those programs; set only
/// when the trade-off is acceptable.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Data(Vec<u8>),
    MousePress {
        col: u16,
        row: u16,
        button: u8,
    },
    /// SGR mouse release (`\x1b[< ... m`). Carries the same fields as
    /// `MousePress` so the daemon can drop both press and release on
    /// the same gate: shells and pre-mount agents that never enabled
    /// any mouse protocol must not see the raw SGR bytes as input.
    MouseRelease {
        col: u16,
        row: u16,
        button: u8,
    },
    PrefixCommand(PrefixCommand),
    /// Direct one-key shortcut → open the palette dialog. Distinct from
    /// `PrefixCommand::Palette`, which fires only after the prefix
    /// gesture; the daemon collapses both into the same dialog open.
    OpenPalette,
    /// Resize the focused pane in `dir` by one step. Emitted by
    /// `Alt+Shift+Arrow` so the operator can drag a split without
    /// reaching for the mouse. Steps are ratio-based (~5%) so the
    /// gesture is independent of terminal size.
    ResizePane(ArrowDir),
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
                        // Default `Ctrl+\` (or configured key) →
                        // immediate palette open. Bracketed paste
                        // already excluded above; operators needing
                        // a literal palette byte set
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
                    match classify_csi(&seq) {
                        Some(Some(ev)) => events.push(ev),
                        Some(None) => {}
                        None => events.push(InputEvent::Data(seq)),
                    }
                    self.state = State::Idle;
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
                        } else {
                            // classify_csi returns an explicit "drop this
                            // sequence" outcome via Some(None) so kitty
                            // key-release events (and any future
                            // suppress-class CSI) never reach the agent
                            // or the dialog as garbage Data bytes.
                            match classify_csi(&seq) {
                                Some(Some(ev)) => events.push(ev),
                                Some(None) => {}
                                None => events.push(InputEvent::Data(seq)),
                            }
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
    pub fn esc_pending(&self) -> bool {
        matches!(self.state, State::EscStart) && !self.seq.is_empty()
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
/// is set to a parseable key, `None` otherwise. The direct palette
/// key (see `default_palette_key`, default `Ctrl+\`) is the primary
/// UX; the prefix-key state machine layered on top is for operators
/// who want tmux-style multi-keystroke commands.
fn default_prefix() -> Option<u8> {
    let s = std::env::var("JACKIN_PREFIX").ok()?;
    if s.eq_ignore_ascii_case("none") {
        return None;
    }
    parse_prefix(&s)
}

/// Palette key defaults to `Ctrl+\` (`0x1C`). Picked because raw-mode
/// terminals never emit it as content (cooked-mode SIGQUIT semantics
/// don't apply in raw mode), no agent uses it as an editing key, and
/// it sits one finger from `Enter` on US/UK layouts. The literal LF
/// byte (`Ctrl+J`, `0x0A`) is what agents and shells use for
/// multi-line input continuation, so we avoid it as the default.
///
/// Set `JACKIN_PALETTE_KEY` to override (e.g. `C-]`, `C-g`, `C-j`);
/// set it to the literal string `none` to disable the direct-palette
/// shortcut entirely. Parse failures log to stderr (visible under
/// `jackin load --debug`) so an operator does not silently get the
/// default after typo'ing the override.
fn default_palette_key() -> Option<u8> {
    match std::env::var("JACKIN_PALETTE_KEY") {
        Err(_) => Some(0x1C),
        Ok(s) if s.eq_ignore_ascii_case("none") => None,
        Ok(s) => match parse_prefix(&s) {
            Some(byte) => Some(byte),
            None => {
                eprintln!(
                    "[jackin-container] invalid JACKIN_PALETTE_KEY={s:?}; using default Ctrl+\\"
                );
                Some(0x1C)
            }
        },
    }
}

/// Accept:
/// - `C-a` … `C-z` (case-insensitive) — `Ctrl+letter`, maps to `0x01..=0x1A`
/// - `C-\` / `C-]` / `C-^` / `C-_` — `Ctrl+symbol`, maps to `0x1C..=0x1F`
/// - `C-Space` or `C-@` — `Ctrl+Space` / `Ctrl+@`, maps to `0x00`
/// - A single ASCII control byte in hex form `0xNN`
/// - A single literal byte
///
/// Returns `None` on parse error so the caller falls back to the default.
pub fn parse_prefix(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("C-").or_else(|| s.strip_prefix("c-")) {
        if rest.eq_ignore_ascii_case("space") || rest == "@" {
            return Some(0x00);
        }
        let c = rest.chars().next()?;
        if c.is_ascii_alphabetic() {
            let upper = c.to_ascii_uppercase() as u8;
            return Some(upper - b'A' + 1);
        }
        // ASCII control-byte mapping for non-letter `Ctrl+symbol`:
        //   Ctrl+\ → 0x1C, Ctrl+] → 0x1D, Ctrl+^ → 0x1E, Ctrl+_ → 0x1F
        return match c {
            '\\' => Some(0x1C),
            ']' => Some(0x1D),
            '^' => Some(0x1E),
            '_' => Some(0x1F),
            _ => None,
        };
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
/// Outer return shape:
///   `None`            → not classified, caller emits the raw `Data`.
///   `Some(None)`      → classified as "suppress" — emit nothing
///                       (kitty key-release events are the only producer).
///   `Some(Some(ev))`  → classified, caller emits `ev`.
fn classify_csi(seq: &[u8]) -> Option<Option<InputEvent>> {
    // Focus in / out.
    if seq == b"\x1b[I" {
        return Some(Some(InputEvent::FocusIn));
    }
    if seq == b"\x1b[O" {
        return Some(Some(InputEvent::FocusOut));
    }
    // Arrow keys.
    //
    // Three encodings arrive at this parser depending on what the
    // operator's outer terminal has been told to emit:
    //   1. Legacy:        ESC [ A/B/C/D
    //   2. xterm modifier: ESC [ 1 ; <mod> A/B/C/D
    //   3. Kitty progressive enhancement:
    //        ESC [ 1 ; <mod> : <event> A/B/C/D
    //      where event 1 = press, 2 = repeat, 3 = release.
    //
    // Encoding (3) lands here whenever a focused agent has pushed the
    // kitty keyboard protocol (`CSI > 1 u`) via OSC passthrough and
    // the daemon mirrored it onto the outer terminal — Claude Code
    // does this — and the operator subsequently presses any arrow.
    //
    // The multiplexer's dialog and most agents only understand
    // encoding (1) for navigation, so this branch normalises the
    // unmodified-press case down to the legacy form and drops key-
    // release events outright (the only emitter is kitty mode, and
    // forwarding them surfaces as visible garbage at agent prompts).
    // Modified arrows keep the legacy `ESC [ 1 ; <mod> <final>` form
    // so agents that consume Alt+Arrow / Shift+Arrow / Ctrl+Arrow
    // still see them; Alt+Shift+Arrow (mod 4) is intercepted as
    // `ResizePane` regardless of encoding to keep the multiplexer's
    // tmux-style drag-resize shortcut working.
    //
    // `Alt+Shift+Arrow` is reserved for multiplexer pane resize so it
    // does not collide with agents that consume `Alt+Arrow` (word
    // navigation) or `Shift+Arrow` (selection extend).
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

        // Drop kitty key-release entirely. Press (1) and repeat (2)
        // map to actions; anything else is treated as a press for
        // safety since older or non-conformant terminals may omit the
        // event tag.
        if event == 3 {
            return Some(None);
        }

        // Alt+Shift+Arrow → multiplexer pane resize.
        if modifier == 4 {
            let dir = match final_byte {
                b'A' => ArrowDir::Up,
                b'B' => ArrowDir::Down,
                b'C' => ArrowDir::Right,
                b'D' => ArrowDir::Left,
                _ => unreachable!(),
            };
            return Some(Some(InputEvent::ResizePane(dir)));
        }

        // No modifier and an event tag was present (kitty form) →
        // strip the kitty wrapper and emit legacy `ESC [ A/B/C/D`
        // so the dialog navigator and non-kitty agents match. When
        // the event tag was absent we leave the legacy `ESC [ 1 ; 1
        // <final>` shape alone — that form already round-trips
        // through every agent path tested.
        if modifier == 1 && body.contains(&b':') {
            let mut plain = b"\x1b[".to_vec();
            plain.push(final_byte);
            return Some(Some(InputEvent::Data(plain)));
        }
    }
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
                return Some(Some(InputEvent::MousePress { col, row, button }));
            }
            return Some(Some(InputEvent::MouseRelease { col, row, button }));
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
    fn ctrl_backslash_opens_palette_by_default() {
        let events = parse_all_default(b"\x1c");
        assert_eq!(events, vec![InputEvent::OpenPalette]);
    }

    #[test]
    fn lone_lf_passes_through_with_default_palette_key() {
        // Ctrl+J = `\n` is no longer the palette key, so multi-line
        // input continuation reaches the PTY unchanged.
        let events = parse_all_default(b"\n");
        assert_eq!(events, vec![InputEvent::Data(b"\n".to_vec())]);
    }

    #[test]
    fn palette_key_disabled_lets_ctrl_backslash_through() {
        let events = InputParser::new(None, None).parse(b"\x1c");
        assert_eq!(events, vec![InputEvent::Data(b"\x1c".to_vec())]);
    }

    #[test]
    fn pasted_text_with_palette_key_does_not_open_palette() {
        // Bracketed paste protects the palette byte inside paste content.
        let mut parser = InputParser::default();
        let events = parser.parse(b"\x1b[200~hello\x1cworld\x1c\x1b[201~");
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
    fn kitty_arrow_press_normalises_to_legacy_form() {
        // Kitty progressive-enhancement arrow Down press, no modifier:
        // `\x1b[1;1:1B`. The dialog navigator only recognises the
        // legacy `\x1b[B`, so the parser must rewrite the kitty form
        // before the byte sequence reaches Dialog::handle_key — every
        // other arrow direction follows the same rule.
        let events = parse_all_default(b"\x1b[1;1:1B");
        assert_eq!(events, vec![InputEvent::Data(b"\x1b[B".to_vec())]);
        let events = parse_all_default(b"\x1b[1;1:1A");
        assert_eq!(events, vec![InputEvent::Data(b"\x1b[A".to_vec())]);
        let events = parse_all_default(b"\x1b[1;1:1C");
        assert_eq!(events, vec![InputEvent::Data(b"\x1b[C".to_vec())]);
        let events = parse_all_default(b"\x1b[1;1:1D");
        assert_eq!(events, vec![InputEvent::Data(b"\x1b[D".to_vec())]);
    }

    #[test]
    fn kitty_arrow_repeat_is_treated_as_press() {
        // Event tag 2 (repeat) must reach the dialog / agent so a
        // held-down arrow continues scrolling instead of stalling
        // after the first emit.
        let events = parse_all_default(b"\x1b[1;1:2B");
        assert_eq!(events, vec![InputEvent::Data(b"\x1b[B".to_vec())]);
    }

    #[test]
    fn kitty_arrow_release_is_suppressed() {
        // Event tag 3 (release) must not surface as a Data event.
        // Forwarding it surfaces as a stray `\x1b[1;1:3B` visible at
        // the agent's prompt and confuses TUIs that key off press
        // events. Both the dialog and the agent only ever care about
        // press / repeat.
        let events = parse_all_default(b"\x1b[1;1:3B");
        assert!(
            events.is_empty(),
            "kitty arrow release must be dropped, got {events:?}"
        );
        let events = parse_all_default(b"\x1b[1;1:3A");
        assert!(events.is_empty());
    }

    #[test]
    fn kitty_alt_shift_arrow_is_resize_pane() {
        // Alt+Shift+Arrow stays a multiplexer-level pane-resize gesture
        // even when the outer terminal is in kitty mode — the event
        // tag is parsed, the press is acted on, the release is
        // suppressed (same shape as the no-modifier case above).
        let events = parse_all_default(b"\x1b[1;4:1B");
        assert_eq!(events, vec![InputEvent::ResizePane(ArrowDir::Down)]);
        let events = parse_all_default(b"\x1b[1;4:3B");
        assert!(
            events.is_empty(),
            "kitty alt+shift arrow release must be dropped, got {events:?}"
        );
    }

    #[test]
    fn legacy_xterm_modifier_arrow_still_round_trips() {
        // Encoding without an event tag stays untouched — agents that
        // consume the legacy modifier form (Ctrl+Arrow word nav etc.)
        // continue to receive it byte-for-byte.
        let events = parse_all_default(b"\x1b[1;5A");
        match &events[..] {
            [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[1;5A"),
            other => panic!("Ctrl+Up must round-trip: {other:?}"),
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
