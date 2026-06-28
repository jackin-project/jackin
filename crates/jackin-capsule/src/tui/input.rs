//! Capsule TUI input parsing: classify raw terminal bytes into palette/prefix
//! key events, mouse events, and PTY pass-through sequences.
//!
//! Not responsible for: acting on classified events (see `daemon` dispatch) or
//! rendering (see `tui` render modules).

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
/// A second click on the active tab cell within this window is a
/// TUI double-click and opens the rename-tab dialog.
pub(crate) const TAB_DOUBLE_CLICK_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(500);

/// `JACKIN_ESCAPE_TIME` env var — operator-tunable in milliseconds.
pub(crate) const ENV_ESCAPE_TIME: &str = "JACKIN_ESCAPE_TIME";

/// 50 ms matches tmux's default. Below human perception while
/// surviving slow ssh / paste chunks.
pub(crate) const DEFAULT_ESCAPE_TIME: std::time::Duration = std::time::Duration::from_millis(50);

/// `XTerm` SGR any-event mouse tracking reports passive motion as
/// button code 35 (`32` motion bit + `3` no-button code).
pub(crate) const SGR_NO_BUTTON_MOTION: u8 = 35;

pub(crate) fn pane_wheel_cursor_fallback_reason(
    mouse_enabled: bool,
    alternate_screen: bool,
) -> Option<&'static str> {
    if mouse_enabled {
        return None;
    }
    if alternate_screen {
        return Some("alternate-screen");
    }
    None
}

/// SGR mouse wheel events set bit 6 of the button byte. Every value in
/// `64..=95` is a wheel event with some combination of modifier flags
/// (shift = +4, alt = +8, ctrl = +16). Panes that did not request
/// mouse mode must not receive these bytes because they dump raw SGR at
/// prompts or disappear into TUIs that never subscribed to mouse input.
pub(crate) fn is_wheel_button(button: u8) -> bool {
    (64..96).contains(&button)
}

pub(crate) fn mouse_event_allowed_for_mode(
    mode: jackin_term::MouseProtocolMode,
    button: u8,
    press: bool,
) -> bool {
    if mode == jackin_term::MouseProtocolMode::None {
        return false;
    }
    if is_wheel_button(button) {
        return true;
    }

    let motion = button & 0b100000 != 0;
    let passive_motion = motion && button & 0b11 == 3;
    match mode {
        jackin_term::MouseProtocolMode::None => false,
        jackin_term::MouseProtocolMode::Press => press && !motion,
        // PressRelease = mode 1001: press + release events, no motion.
        jackin_term::MouseProtocolMode::PressRelease => !motion,
        // ButtonMotion = mode 1002: press + release + button-held motion, no passive motion.
        jackin_term::MouseProtocolMode::ButtonMotion => !passive_motion,
        // AnyEvent and AnyMotion are aliases for mode 1003: all events.
        jackin_term::MouseProtocolMode::AnyEvent | jackin_term::MouseProtocolMode::AnyMotion => {
            true
        }
    }
}

pub(crate) fn mouse_event_encoding_for_mode(
    mode: jackin_term::MouseProtocolMode,
    encoding: jackin_term::MouseProtocolEncoding,
    button: u8,
    press: bool,
) -> Option<jackin_term::MouseProtocolEncoding> {
    if mouse_event_allowed_for_mode(mode, button, press) {
        return Some(encoding);
    }
    None
}

pub(crate) fn encode_mouse_for_protocol(
    button: u8,
    col: u16,
    row: u16,
    press: bool,
    encoding: jackin_term::MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    match encoding {
        jackin_term::MouseProtocolEncoding::Sgr => {
            let final_byte = if press { 'M' } else { 'm' };
            Some(format!("\x1b[<{button};{col};{row}{final_byte}").into_bytes())
        }
        jackin_term::MouseProtocolEncoding::Default
        | jackin_term::MouseProtocolEncoding::Utf8
        // Urxvt uses decimal coordinates but the same CSI M prefix — treat as Default.
        | jackin_term::MouseProtocolEncoding::Urxvt => {
            let release_button = (button & !0b11) | 3;
            let button_code = if press { button } else { release_button };
            let mut out = b"\x1b[M".to_vec();
            push_xterm_mouse_number(&mut out, u32::from(button_code) + 32, encoding)?;
            push_xterm_mouse_number(&mut out, u32::from(col) + 32, encoding)?;
            push_xterm_mouse_number(&mut out, u32::from(row) + 32, encoding)?;
            Some(out)
        }
    }
}

pub(crate) fn encode_wheel_cursor_fallback(
    mouse_enabled: bool,
    application_cursor: bool,
    button: u8,
) -> Option<Vec<u8>> {
    if !is_wheel_button(button) || mouse_enabled {
        return None;
    }
    let seq = if application_cursor {
        if (button & 1) == 0 {
            b"\x1bOA".as_slice()
        } else {
            b"\x1bOB".as_slice()
        }
    } else if (button & 1) == 0 {
        b"\x1b[A".as_slice()
    } else {
        b"\x1b[B".as_slice()
    };
    let mut out = Vec::with_capacity(seq.len() * 3);
    for _ in 0..3 {
        out.extend_from_slice(seq);
    }
    Some(out)
}

pub(crate) fn push_xterm_mouse_number(
    out: &mut Vec<u8>,
    value: u32,
    encoding: jackin_term::MouseProtocolEncoding,
) -> Option<()> {
    match encoding {
        jackin_term::MouseProtocolEncoding::Default | jackin_term::MouseProtocolEncoding::Urxvt => {
            out.push(u8::try_from(value).ok()?);
        }
        jackin_term::MouseProtocolEncoding::Utf8 => {
            let ch = char::from_u32(value)?;
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
        jackin_term::MouseProtocolEncoding::Sgr => unreachable!("SGR does not use xterm fields"),
    }
    Some(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputBindings {
    pub prefix: Option<u8>,
    pub palette_key: Option<u8>,
}

impl Default for InputBindings {
    fn default() -> Self {
        Self {
            prefix: None,
            palette_key: Some(0x1C),
        }
    }
}

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
    /// `Ctrl+Q` (byte `0x11`) → open the "Exit jackin❯?" confirmation. The
    /// quit chord is consistent with every other jackin❯ surface; the dialog
    /// warns that exiting force-stops the container before it does so.
    RequestExit,
    /// Resize the focused pane in `dir` by one step. Emitted by
    /// `Alt+Shift+Arrow` so the operator can drag a split without
    /// reaching for the mouse. Steps are ratio-based (~5%) so the
    /// gesture is independent of terminal size.
    ResizePane(ArrowDir),
    FocusIn,
    FocusOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    ClearPane,
    Detach,
    Usage,
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

/// Cap on the in-progress CSI/OSC/SS3/OtherEsc sequence buffer. The
/// parser is stateful across `parse()` calls — an attacker (or operator
/// pasting malformed terminal output) could otherwise stream
/// `\x1b[` followed by megabytes of parameter bytes across many input
/// frames without ever sending the terminator byte, growing `self.seq`
/// unboundedly. 16 KiB is well above the largest legitimate terminal
/// escape (kitty graphics OSC payloads top out around 4 KiB chunks).
/// When the cap is hit we drop the in-flight sequence and reset to
/// Idle so a subsequent well-formed sequence resyncs cleanly.
const MAX_ESC_SEQ_LEN: usize = 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum State {
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
    pub fn is_awaiting_prefix(&self) -> bool {
        matches!(self.state, State::PrefixAwait)
    }

    /// Whether the prefix-mode (`Ctrl+B …`) is active. Affects the
    /// status-bar hint format.
    pub fn prefix_enabled(&self) -> bool {
        self.prefix.is_some()
    }

    /// The resolved palette-key byte, or `None` when palette mode is disabled.
    /// Used by the hint builder to render the correct key glyph when the
    /// operator has overridden `JACKIN_PALETTE_KEY`.
    pub fn palette_key(&self) -> Option<u8> {
        self.palette_key
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
                    } else if let Some(chord) = jackin_tui::keymap::raw_bytes_to_chord(&[b])
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
                        Some(Some(ev)) => events.push(ev),
                        Some(None) => {}
                        None => events.push(InputEvent::Data(seq)),
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
                            // sequence" outcome via Some(None) so kitty
                            // key-release events (and any future
                            // suppress-class CSI) never reach the agent
                            // or the dialog as garbage Data bytes.
                            match classify_csi(&seq, self.palette_key) {
                                Some(Some(ev)) => events.push(ev),
                                Some(None) => {}
                                None => events.push(InputEvent::Data(seq)),
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

pub fn parse_prefix(s: &str) -> Option<u8> {
    parse_key_binding(s)
}

/// Accept:
/// - `C-a` ... `C-z` (case-insensitive) - `Ctrl+letter`, maps to `0x01..=0x1A`
/// - `C-\` / `C-]` / `C-^` / `C-_` - `Ctrl+symbol`, maps to `0x1C..=0x1F`
/// - `C-Space` or `C-@` - `Ctrl+Space` / `Ctrl+@`, maps to `0x00`
/// - A single ASCII control byte in hex form `0xNN`
/// - A single literal byte
pub fn parse_key_binding(s: &str) -> Option<u8> {
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
    use jackin_tui::keymap::raw_bytes_to_chord;
    let chord = raw_bytes_to_chord(&[b])?;
    crate::tui::keymap::PREFIX_COMMAND_KEYMAP.dispatch(chord)
}

fn parse_csi_u_key(rest: &[u8]) -> Option<(u32, Option<u32>, Option<u32>)> {
    let mut parts = rest.splitn(2, |&b| b == b';');
    let codepoint = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    let Some(modifier_and_event) = parts.next() else {
        return Some((codepoint, None, None));
    };
    let mut modifier_parts = modifier_and_event.splitn(2, |&b| b == b':');
    let modifier = std::str::from_utf8(modifier_parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    let event = modifier_parts
        .next()
        .and_then(|raw| std::str::from_utf8(raw).ok())
        .and_then(|raw| raw.parse::<u32>().ok());
    Some((codepoint, Some(modifier), event))
}

fn parse_xterm_modify_other_keys(seq: &[u8]) -> Option<(u32, u32)> {
    let body = seq.strip_prefix(b"\x1b[")?.strip_suffix(b"~")?;
    let mut parts = body.split(|&b| b == b';');
    let prefix = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    if prefix != 27 {
        return None;
    }
    let modifier = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    let codepoint = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((codepoint, modifier))
}

/// Decode a complete CSI sequence into a higher-level event when we
/// recognise it. Returns `None` to forward the bytes verbatim.
/// Outer return shape:
///   `None`            → not classified, caller emits the raw `Data`.
///   `Some(None)`      → classified as "suppress" — emit nothing
///                       (kitty key-release and terminal-report replies).
///   `Some(Some(ev))`  → classified, caller emits `ev`.
fn classify_csi(seq: &[u8], palette_key: Option<u8>) -> Option<Option<InputEvent>> {
    // Focus in / out.
    if seq == b"\x1b[I" {
        return Some(Some(InputEvent::FocusIn));
    }
    if seq == b"\x1b[O" {
        return Some(Some(InputEvent::FocusOut));
    }
    // Kitty / CSI-u Escape and control keys. Once a focused agent enables the
    // kitty keyboard protocol, many terminals encode Esc as `CSI 27 ... u`
    // instead of a bare `ESC`; dialogs must still receive the same byte their
    // dismiss logic matches. Release events are suppressed like kitty arrow
    // releases because dialog and agent paths only care about key press / repeat.
    // Control-byte press/repeat events (palette key, Ctrl+Q, etc.) are
    // dispatched through the global keymap before being forwarded to the agent.
    if let Some(rest) = seq
        .strip_prefix(b"\x1b[")
        .and_then(|body| body.strip_suffix(b"u"))
    {
        let (codepoint, modifier, event) = parse_csi_u_key(rest)?;
        if event == Some(3) {
            return Some(None);
        }
        if codepoint == 27 && modifier.unwrap_or(1) == 1 {
            return Some(Some(InputEvent::Data(b"\x1b".to_vec())));
        }
        if matches!(event, None | Some(1 | 2))
            && let Some(control) = csi_u_control_byte(codepoint, modifier)
        {
            return dispatch_control_byte(control, palette_key).map(Some);
        }
    }
    // Xterm window-report replies (`CSI ... t`) are generated by the
    // outer terminal, not typed by the operator. Ghostty emits them
    // while answering size queries such as `CSI 18t`; during a
    // resize burst those replies arrive on the attach client's stdin
    // and cannot be safely routed by "whatever pane is focused now".
    // Forwarding them to a shell leaks fragments like `8;40;135t` as
    // command text. Keep this paired with the output-side `CSI ... t`
    // passthrough suppression in `session::apply_passthrough_policy` for `UnhandledCsi`.
    if matches!(seq.last(), Some(b't')) {
        return Some(None);
    }
    // Ghostty may emit xterm modifyOtherKeys for Shift+Enter as
    // `CSI 27 ; 2 ; 13 ~` before the focused agent has negotiated
    // CSI-u/kitty mode on the outer terminal. Codex treats CSI-u
    // `CSI 13 ; 2 u` as the multiline-entry key but ignores the
    // xterm form, so normalize this one editor-critical key while
    // leaving the rest of modifyOtherKeys byte-for-byte.
    if let Some((13, 2)) = parse_xterm_modify_other_keys(seq) {
        return Some(Some(InputEvent::Data(b"\x1b[13;2u".to_vec())));
    }
    // Other Ctrl+key combos (palette key, Ctrl+Q, …) encoded as xterm
    // modifyOtherKeys by terminals that haven't negotiated CSI-u mode — dispatch
    // through the same control-byte path as the CSI-u block above.
    if let Some((codepoint, modifier)) = parse_xterm_modify_other_keys(seq)
        && let Some(control) = csi_u_control_byte(codepoint, Some(modifier))
    {
        return dispatch_control_byte(control, palette_key).map(Some);
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

        if modifier == 4 {
            let action = match final_byte {
                b'A' => crate::tui::keymap::ResizePaneAction::Up,
                b'B' => crate::tui::keymap::ResizePaneAction::Down,
                b'C' => crate::tui::keymap::ResizePaneAction::Right,
                b'D' => crate::tui::keymap::ResizePaneAction::Left,
                _ => unreachable!("kitty arrow parser only calls resize mapping for arrow bytes"),
            };
            return Some(Some(action.to_input_event()));
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

/// Dispatch a bare control byte through the capsule-level keymap. Returns the
/// mapped `InputEvent` (palette open, request-exit, pane-resize, …) or `None`
/// when the byte has no capsule binding and should pass through to the agent.
fn dispatch_control_byte(control: u8, palette_key: Option<u8>) -> Option<InputEvent> {
    if Some(control) == palette_key {
        return Some(InputEvent::OpenPalette);
    }
    if let Some(chord) = jackin_tui::keymap::raw_bytes_to_chord(&[control])
        && let Some(action) = crate::tui::keymap::CAPSULE_GLOBAL_KEYMAP.dispatch(chord)
    {
        return Some(action.to_input_event());
    }
    None
}

/// Map a CSI-u codepoint+modifier pair to a bare control byte (0x00–0x1F), or
/// `None` when the key is not a control byte. C0 codepoints pass through
/// directly. For printable codepoints, Ctrl must be held — CSI-u encodes
/// modifiers as a 1-based bitmask, so bit 2 of `modifier − 1` is the Ctrl bit.
fn csi_u_control_byte(codepoint: u32, modifier: Option<u32>) -> Option<u8> {
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

fn classify_x10_mouse(seq: &[u8]) -> Option<InputEvent> {
    if seq.len() != 6 || !seq.starts_with(b"\x1b[M") {
        return None;
    }
    let button = seq[3].checked_sub(32)?;
    let col = u16::from(seq[4]).checked_sub(33)?;
    let row = u16::from(seq[5]).checked_sub(33)?;
    if button & 0b11 == 3 && button & 0b100000 == 0 {
        return Some(InputEvent::MouseRelease {
            col,
            row,
            button: 0,
        });
    }
    Some(InputEvent::MousePress { col, row, button })
}

#[cfg(test)]
mod tests;
