/// The multiplexer daemon — runs as PID 1, manages sessions and clients.
///
/// Architecture:
///   - One active attach client at a time. A new `Hello` from a second
///     client sends `Shutdown` to the old one and aborts the old
///     client's reader task (see `attached_task`).
///   - Attach traffic uses the binary tag+length protocol in
///     `protocol::attach`. The hot path forwards raw PTY bytes without
///     base64 or JSON nesting.
///   - The control channel still speaks length-prefixed JSON for one-shot
///     `status` queries from the host CLI. Channel dispatch is by first
///     byte: `0x00` → control (length prefix), anything else → attach.
///   - Lifecycle: the daemon exits when the last session ends so the
///     container reaps cleanly. SIGTERM also triggers shutdown.
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use unicode_width::UnicodeWidthChar;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use portable_pty::CommandBuilder;

use crate::dialog::{
    ConfirmKind, Dialog, DialogAction, PaletteCloseLabel, PaletteCommand, PickerIntent,
    SplitDirection,
};
use crate::input::{ArrowDir, InputEvent, InputParser, PrefixCommand};
use crate::layout::{Direction, Rect, SplitOrient, SplitPosition, Tab};
use crate::protocol::attach::{
    ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_server, read_client_frame,
};
use crate::protocol::control::{AgentState, SessionInfo};
use crate::render::{PaneBodyCache, PaneBodyRenderMode, draw_scrollbar};
use crate::session::{
    PullRequestChecks, PullRequestInfo, PullRequestLookupOutcome, SESSION_ENV_PASSTHROUGH, Session,
    SessionEvent, build_agent_command, build_shell_command,
};
use crate::socket;
use crate::statusbar::{STATUS_BAR_ROWS, StatusBar, draw_pane_box};
use crate::terminal_geometry::{DEFAULT_COLS, DEFAULT_ROWS, normalize_size};

struct SessionLaunch {
    label: String,
    cmd: CommandBuilder,
}

pub struct Multiplexer {
    sessions: HashMap<u64, Session>,
    tabs: Vec<Tab>,
    active_tab: usize,
    term_rows: u16,
    term_cols: u16,
    status_bar: StatusBar,
    /// LIFO stack of open dialogs. The top of stack is the live one
    /// the renderer paints and the input dispatcher routes keys to;
    /// older dialogs sit underneath waiting for an Esc-pop to surface
    /// them again. Sub-dialogs (Menu → New tab → AgentPicker,
    /// Menu → Split pane → SplitDirectionPicker → AgentPicker,
    /// Menu → Close → CloseTargetPicker / ConfirmClose, …) push onto
    /// this stack so Esc walks the operator back one step at a time
    /// instead of nuking the whole flow. The empty stack means "no
    /// dialog open" — every consumer treats `dialog_top()` as the
    /// canonical "is a dialog visible" check.
    dialog_stack: Vec<Dialog>,
    content_rows: u16,
    available_agents: Vec<String>,
    launch_config: CapsuleConfig,
    env_passthrough: Vec<(String, String)>,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    zoomed: Option<u64>,
    input_parser: InputParser,
    detach_requested: bool,
    attached_out: Option<mpsc::UnboundedSender<Vec<u8>>>,
    /// Latched true on the first `send_to_client` after `attached_out`
    /// was set: once the receiver drops mid-attach, every subsequent
    /// frame send into the same channel will also fail. Without this
    /// latch the per-tick redraw + per-PTY output + per-OSC repaint
    /// would write one `clog!` line each, swamping `multiplexer.log`.
    /// Cleared whenever `attached_out` is reassigned (next attach).
    attached_out_dead_logged: bool,
    /// JoinHandle of the spawned `handle_attach_client` task for the
    /// currently-attached client. Tracked so a takeover (second `Hello`)
    /// can abort the old task's reader loop — without the abort, the
    /// old client's stale Input / Resize / Detach frames keep flowing
    /// into the shared `cmd_tx` until its socket finally closes.
    attached_task: Option<tokio::task::JoinHandle<()>>,
    /// Records the previous tab-cell click so a second click on the
    /// same tab within `DOUBLE_CLICK_WINDOW` is treated as a
    /// double-click (open the rename modal).
    last_tab_click: Option<(usize, std::time::Instant)>,
    /// Active mouse-drag resize, if any. Populated when the operator
    /// presses the left button on a shared pane border; updated on
    /// every motion event; cleared on release.
    drag: Option<DragState>,
    /// Active mouse text selection on a pane whose program ignored
    /// the mouse. Updated on every motion event; copied to the
    /// outer clipboard via OSC 52 on release.
    selection: Option<SelectionState>,
    /// Last visible pane-body snapshot per session. PTY output can
    /// then repaint only rows whose vt100 cells changed.
    pane_body_caches: HashMap<u64, PaneBodyCache>,
    /// Pane bodies dirtied by PTY output. The render ticker drains
    /// this at most once per frame, preserving the existing coalescing
    /// behavior while avoiding broad body redraws.
    dirty_panes: HashSet<u64>,
    /// Named full-frame invalidation, used whenever partial pane-body
    /// repainting would be unsafe or when chrome/status/dialog/layout
    /// changed outside the pane body.
    pending_full_redraw: Option<FullRedrawReason>,
    /// Last pointer shape emitted through OSC 22. Stored so passive
    /// mouse motion does not spam the outer terminal with duplicate
    /// pointer-shape updates.
    pointer_shape: PointerShape,
    /// True only for outer terminals known to support OSC 22 with CSS
    /// pointer names. Unsupported terminals keep normal cursor behavior.
    pointer_shapes_supported: bool,
    /// Terminal identity reported by the active attach client. Refreshed
    /// on every attach/takeover so daemon-owned output enhancements can
    /// follow the terminal the operator is using now rather than the
    /// terminal that launched the container.
    attached_terminal: ClientTerminal,
    /// Hash of the last multiplexer-owned OSC 2 title sent to the
    /// outer terminal. Gates re-emission on inequality: without the
    /// diff, every full frame would reassert the workspace/PR title
    /// and override per-pane agent-set titles in the outer terminal's
    /// tab list on every redraw. Reset to `None` when a child pane
    /// updates its own title so the next full frame re-asserts.
    last_outer_terminal_title: Option<String>,
    hover_target: Option<HoverTarget>,
    /// Deadline for hiding the transient "Copied!" badge in whichever
    /// dialog most recently performed a jackin-owned OSC 52 copy.
    dialog_copy_feedback_deadline: Option<Instant>,
    /// Current branch / PR context rendered in the bottom status line.
    pull_request_context_branch: Option<String>,
    pull_request_context: Option<PullRequestInfo>,
    /// State of the fast local git branch lookup (`git_current_branch`):
    /// monotonic request id, in-flight gate, last-run instant for the
    /// cooldown check. The result lands on `pull_request_context_branch`.
    git_branch_lookup: LookupState,
    /// State of the 60 s `gh` PR-info lookup. Uses `request_id` +
    /// `in_flight`; `last_run` is unused (per-branch freshness lives
    /// in `pull_request_context_cache` instead because the operator
    /// can flip between branches with cached results in flight).
    pull_request_lookup: LookupState,
    pull_request_context_cache: HashMap<String, PullRequestContextCacheEntry>,
    /// Workspace workdir read from `/jackin/run/agent.toml` at daemon startup.
    /// Every spawned PTY (agent or shell) receives this as its `cwd`
    /// so the operator's panes open in the workspace they configured
    /// instead of `$HOME` (portable_pty's CommandBuilder default).
    workdir: PathBuf,
    /// Static facts about the workdir resolved once at daemon construction:
    /// `git` / `gh` binary availability, whether the workdir is a git
    /// checkout, and the repo's default branch resolved from
    /// `origin/HEAD`. Cached so the 1Hz state ticker does not fork
    /// `git`/`gh` subprocesses for facts that cannot change while the
    /// daemon is alive — a non-git workdir would otherwise pay 86,400
    /// subprocess forks per day for the same negative verdict.
    workdir_context: WorkdirContext,
}

/// One-shot resolution of workdir + tool facts. The daemon never
/// re-probes; if `git` or `gh` is installed mid-session the operator
/// must restart the container to pick it up (a non-event for an
/// orchestrator that already discards containers per launch).
struct WorkdirContext {
    is_git_repo: bool,
    git_available: bool,
    gh_available: bool,
    /// `origin/HEAD` resolved to a short branch name (`main`, `master`,
    /// `trunk`, `develop`, …). `None` when the workdir is not a git
    /// checkout, when `origin/HEAD` is not set, or when `git
    /// symbolic-ref` fails. Falls back to a `main`/`master` literal
    /// match for branches that look like defaults when this is `None`.
    default_branch: Option<String>,
}

impl WorkdirContext {
    fn resolve(workdir: &Path) -> Self {
        let git_available = command_in_path("git");
        let gh_available = command_in_path("gh");
        // `.git` may be a regular directory (normal checkout) or a
        // file containing `gitdir: …` (worktree / submodule).
        // `try_exists` covers both. False when `workdir` is not a git
        // checkout at all — no point probing branch lookups in that
        // case.
        let is_git_repo = git_available && workdir.join(".git").try_exists().ok().unwrap_or(false);
        let default_branch = if is_git_repo {
            resolve_default_branch(workdir)
        } else {
            None
        };
        Self {
            is_git_repo,
            git_available,
            gh_available,
            default_branch,
        }
    }

    /// True when `branch` is the repo's default branch (the chrome bar
    /// stays hidden in that case). Falls back to a literal
    /// `main`/`master`/empty match when `origin/HEAD` is not set so
    /// freshly-cloned-but-not-`gh-repo-set-default`ed repos still
    /// suppress the bar.
    fn is_default_branch(&self, branch: &str) -> bool {
        if branch.is_empty() {
            return true;
        }
        if let Some(default) = self.default_branch.as_deref() {
            return branch == default;
        }
        matches!(branch, "main" | "master")
    }
}

/// Probe `name --version` once at construction. Stdin/stdout/stderr
/// are nulled so a misbehaving subprocess cannot leak output into the
/// daemon's logs and cannot block on stdin. A `false` verdict
/// short-circuits all later lookup spawns for the duration of the
/// daemon process.
fn command_in_path(name: &str) -> bool {
    let mut cmd = Command::new(name);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    matches!(cmd.status(), Ok(status) if status.success())
}

/// `git symbolic-ref --short refs/remotes/origin/HEAD` returns
/// `origin/main` (or whatever the default branch is). Strip the
/// `origin/` so it can compare directly against `git branch
/// --show-current` output.
fn resolve_default_branch(workdir: &Path) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(workdir)
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]);
    let raw = command_stdout_trimmed_with_timeout(&mut cmd, GIT_CONTEXT_COMMAND_TIMEOUT)?;
    raw.strip_prefix("origin/").map(|s| s.to_string())
}

/// Shared shape for the two background context lookups (git branch +
/// gh PR). Wraps the three book-keeping fields that always travel
/// together — bumping `request_id` on every spawn, flipping
/// `in_flight` for the dedup gate, and stamping `last_run` for the
/// optional cooldown check. Collapsing them into one struct prevents
/// the symmetric-variant drift the project DRY rule warns about: a
/// future change that bumps `request_id` without clearing
/// `in_flight` on one side and not the other was the exact shape of
/// the branch-flip race bug just fixed.
#[derive(Default)]
struct LookupState {
    request_id: u64,
    in_flight: bool,
    last_run: Option<Instant>,
}

impl LookupState {
    fn next_request_id(&mut self) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.request_id
    }

    fn cooldown_active(&self, now: Instant, interval: Duration) -> bool {
        self.last_run
            .is_some_and(|last| now.duration_since(last) < interval)
    }
}

#[derive(Clone)]
struct PullRequestContextCacheEntry {
    checked_at: Instant,
    pull_request: Option<PullRequestInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FullRedrawReason {
    FirstAttach,
    Resize,
    TabSwitch,
    LayoutChange,
    SplitClose,
    ZoomChange,
    ScrollbackMovement,
    DialogChange,
    SelectionRepaint,
    PaletteOverlay,
    FocusChange,
    SessionExit,
    PaneClear,
    ExplicitRedraw,
    StatusChange,
    PaneCacheMiss,
    UnsafePartial,
}

impl FullRedrawReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::FirstAttach => "first-attach",
            Self::Resize => "resize",
            Self::TabSwitch => "tab-switch",
            Self::LayoutChange => "layout-change",
            Self::SplitClose => "split-close",
            Self::ZoomChange => "zoom-change",
            Self::ScrollbackMovement => "scrollback-movement",
            Self::DialogChange => "dialog-change",
            Self::SelectionRepaint => "selection-repaint",
            Self::PaletteOverlay => "palette-overlay",
            Self::FocusChange => "focus-change",
            Self::SessionExit => "session-exit",
            Self::PaneClear => "pane-clear",
            Self::ExplicitRedraw => "explicit-redraw",
            Self::StatusChange => "status-change",
            Self::PaneCacheMiss => "pane-cache-miss",
            Self::UnsafePartial => "unsafe-partial",
        }
    }
}

/// Stages of the takeover/first-attach burst. Each variant maps to a
/// human-readable label used in the clog line when the send fails so a
/// dropped initial frame is observable in the multiplexer log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitialFrameKind {
    Welcome,
    ClientOwnedModes,
    FocusedPaneModes,
    FirstAttach,
    SpawnFailureBanner,
}

impl InitialFrameKind {
    fn label(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::ClientOwnedModes => "client-owned mode state",
            Self::FocusedPaneModes => "focused-pane mode state",
            Self::FirstAttach => "first-attach frame",
            Self::SpawnFailureBanner => "spawn-failure banner",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointerShape {
    Default,
    Pointer,
    Text,
    EwResize,
    NsResize,
    Grabbing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HoverTarget {
    Tab(usize),
    Menu,
    BranchContext,
    Container,
    DialogCopyTarget,
}

impl PointerShape {
    fn as_osc22_name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Pointer => "pointer",
            Self::Text => "text",
            Self::EwResize => "ew-resize",
            Self::NsResize => "ns-resize",
            Self::Grabbing => "grabbing",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct VisiblePane {
    id: u64,
    outer: Rect,
    inner: Rect,
    focused: bool,
    dim: bool,
}

#[derive(Debug, Clone)]
struct DragState {
    tab_idx: usize,
    /// Tree path from the tab's root to the split node being resized
    /// (`0` = left/top child, `1` = right/bottom). Empty path = root
    /// split.
    path: Vec<u8>,
    orient: SplitOrient,
    /// Outer rectangle of the split — stable for the duration of the
    /// drag because spawns / closes block on dialog input and the
    /// daemon does not reflow during a drag.
    rect: Rect,
}

/// Mouse-driven text selection on a pane whose program never asked
/// for a mouse protocol (shells, post-exit agents). Modelled on
/// zellij's behaviour: drag inside the pane body paints an inverse
/// highlight; release base64-encodes the selected text and writes it
/// to the operator's clipboard via OSC 52. Cleared on any focus
/// change, tab swap, or dialog open.
#[derive(Debug, Clone)]
struct SelectionState {
    session_id: u64,
    /// Pane's inner content rectangle at selection-start time. Stays
    /// stable through the drag (a resize / reflow cancels the
    /// selection in the same places `DragState` is cancelled).
    inner: Rect,
    /// 0-based grid coordinates relative to the pane's inner area,
    /// captured at press time. Stays put during the drag.
    anchor_row: u16,
    anchor_col: u16,
    /// Latest grid coordinate the operator's cursor reached. Updated
    /// on every motion event.
    end_row: u16,
    end_col: u16,
}

const DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Hard cap on simultaneous tabs. 32 is well past any operator
/// workflow but small enough that an accidental loop of new-tab
/// requests cannot drive the container OOM.
const MAX_TABS: usize = 32;

/// Hard cap on simultaneous sessions (panes). Splits within tabs
/// can grow the session count past the tab count; cap separately
/// for the same memory-bounding reason.
const MAX_SESSIONS: usize = 64;

/// `JACKIN_ESCAPE_TIME` env var — operator-tunable in milliseconds.
const ENV_ESCAPE_TIME: &str = "JACKIN_ESCAPE_TIME";

/// 50 ms matches tmux's default. Below human perception while
/// surviving slow ssh / paste chunks.
const DEFAULT_ESCAPE_TIME: std::time::Duration = std::time::Duration::from_millis(50);

/// XTerm SGR any-event mouse tracking reports passive motion as
/// button code 35 (`32` motion bit + `3` no-button code).
const SGR_NO_BUTTON_MOTION: u8 = 35;

const DIALOG_COPY_FEEDBACK_DURATION: std::time::Duration = std::time::Duration::from_secs(2);
/// Bottom branch/PR context bar is a single chrome row. Centralising
/// the constant keeps the content-row math and renderer in sync; if
/// the bar ever grows to two rows the change happens in one place.
const BRANCH_CONTEXT_BAR_ROWS: u16 = 1;
/// 100 ms keeps the branch bar inside the "immediate" threshold after
/// `git checkout`. The hot path is a direct `.git/HEAD` read (including
/// worktree-style `.git` files), so this does not put `git` subprocesses
/// on the normal render path.
const GIT_BRANCH_CONTEXT_POLL_INTERVAL: Duration = Duration::from_millis(100);
/// 60 s keeps the CI-status freshness within one PR turn while
/// staying well under `gh`'s default secondary-rate-limit budget.
/// The bar is operator-facing chrome, not a live feed.
const PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL: Duration = Duration::from_secs(60);

impl Multiplexer {
    pub fn new(rows: u16, cols: u16, launch_config: CapsuleConfig) -> Self {
        let (rows, cols) = normalize_size(rows, cols);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let content_rows = rows
            .saturating_sub(STATUS_BAR_ROWS)
            .saturating_sub(BRANCH_CONTEXT_BAR_ROWS);
        let agents = launch_config.supported_agents();

        let env_passthrough: Vec<(String, String)> = SESSION_ENV_PASSTHROUGH
            .iter()
            .filter_map(|&k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
            .collect();

        let input_parser = InputParser::default();
        let workdir = PathBuf::from(&launch_config.workdir);
        let workdir_context = WorkdirContext::resolve(&workdir);
        crate::clog!(
            "workdir-context: git_available={} gh_available={} is_git_repo={} default_branch={:?}",
            workdir_context.git_available,
            workdir_context.gh_available,
            workdir_context.is_git_repo,
            workdir_context.default_branch
        );
        let mut status_bar = StatusBar::new_with_role(launch_config.role.clone());
        status_bar.set_prefix_enabled(input_parser.prefix_enabled());

        Self {
            sessions: HashMap::new(),
            tabs: Vec::new(),
            active_tab: 0,
            term_rows: rows,
            term_cols: cols,
            status_bar,
            dialog_stack: Vec::new(),
            content_rows,
            available_agents: agents,
            launch_config,
            env_passthrough,
            event_tx,
            event_rx,
            zoomed: None,
            input_parser,
            detach_requested: false,
            attached_out: None,
            attached_out_dead_logged: false,
            attached_task: None,
            last_tab_click: None,
            drag: None,
            selection: None,
            pane_body_caches: HashMap::new(),
            dirty_panes: HashSet::new(),
            pending_full_redraw: None,
            pointer_shape: PointerShape::Default,
            pointer_shapes_supported: false,
            attached_terminal: ClientTerminal::default(),
            last_outer_terminal_title: None,
            hover_target: None,
            dialog_copy_feedback_deadline: None,
            pull_request_context_branch: None,
            pull_request_context: None,
            git_branch_lookup: LookupState::default(),
            pull_request_lookup: LookupState::default(),
            pull_request_context_cache: HashMap::new(),
            workdir,
            workdir_context,
        }
    }

    fn send_to_client(&mut self, frame: ServerFrame) {
        if let Some(tx) = &self.attached_out
            && tx.send(encode_server(frame)).is_err()
            && !self.attached_out_dead_logged
        {
            self.attached_out_dead_logged = true;
            crate::clog!(
                "send_to_client: client receiver dropped; frame discarded (this attach is dead)"
            );
        }
    }

    fn send_output(&mut self, bytes: Vec<u8>) {
        self.send_to_client(ServerFrame::Output(bytes));
    }

    fn set_pointer_shape(&mut self, shape: PointerShape) {
        if !self.pointer_shapes_supported || self.pointer_shape == shape {
            return;
        }
        self.pointer_shape = shape;
        self.send_output(osc22_pointer_shape(shape));
    }

    fn update_pointer_shape_for_mouse(&mut self, row: u16, col: u16, button: u8) {
        if !self.pointer_shapes_supported {
            return;
        }
        let shape = self.pointer_shape_at(row, col, button);
        self.set_pointer_shape(shape);
    }

    fn update_hover_for_mouse(&mut self, row: u16, col: u16) -> Option<Vec<u8>> {
        let next = self.hover_target_at(row, col);
        if self.hover_target == next {
            return None;
        }
        self.hover_target = next;
        if self.dialog_open() {
            Some(self.compose_full_frame(FullRedrawReason::DialogChange))
        } else {
            Some(self.compose_chrome_hover_frame())
        }
    }

    fn hover_target_at(&self, row: u16, col: u16) -> Option<HoverTarget> {
        if self.drag.is_some() || self.selection.is_some() {
            return None;
        }
        if let Some(dialog) = self.dialog_top() {
            return dialog
                .clickable_at(row + 1, col + 1, self.term_rows, self.term_cols)
                .then_some(HoverTarget::DialogCopyTarget);
        }
        let row_1based = row + 1;
        let col_1based = col + 1;
        if row_1based == 1
            && let Some(tab_idx) = self.status_bar.tab_at_col(col_1based)
        {
            return Some(HoverTarget::Tab(tab_idx));
        }
        if self.status_bar.hint_at(row_1based, col_1based) {
            return Some(HoverTarget::Menu);
        }
        match branch_context_bar_hit(
            row_1based,
            col_1based,
            self.term_rows,
            self.term_cols,
            self.context_bar_branch(),
            self.pull_request_context.as_ref(),
            self.pull_request_context_loading(),
            self.status_bar.instance_id_label(),
        ) {
            Some(BranchContextBarHit::Context) => Some(HoverTarget::BranchContext),
            Some(BranchContextBarHit::Container) => Some(HoverTarget::Container),
            None => None,
        }
    }

    fn pointer_shape_at(&self, row: u16, col: u16, button: u8) -> PointerShape {
        if self.drag.is_some() {
            return PointerShape::Grabbing;
        }
        if self.selection.is_some() {
            return PointerShape::Text;
        }
        if let Some(dialog) = self.dialog_top() {
            return if dialog.clickable_at(row + 1, col + 1, self.term_rows, self.term_cols) {
                PointerShape::Pointer
            } else {
                PointerShape::Default
            };
        }
        let row_1based = row + 1;
        let col_1based = col + 1;
        if row_1based == 1 && self.status_bar.tab_at_col(col_1based).is_some() {
            return PointerShape::Pointer;
        }
        if self.status_bar.hint_at(row_1based, col_1based) {
            return PointerShape::Pointer;
        }
        if branch_context_bar_hit(
            row_1based,
            col_1based,
            self.term_rows,
            self.term_cols,
            self.context_bar_branch(),
            self.pull_request_context.as_ref(),
            self.pull_request_context_loading(),
            self.status_bar.instance_id_label(),
        )
        .is_some()
        {
            return PointerShape::Pointer;
        }
        if let Some(drag) = self.detect_drag_start(row, col) {
            return match drag.orient {
                SplitOrient::Horizontal => PointerShape::EwResize,
                SplitOrient::Vertical => PointerShape::NsResize,
            };
        }
        if button == SGR_NO_BUTTON_MOTION && self.detect_selection_start(row, col).is_some() {
            return PointerShape::Text;
        }
        PointerShape::Default
    }

    fn env_for_spawn(&self, overrides: &[(String, String)]) -> Vec<(String, String)> {
        let mut env = self.env_passthrough.clone();
        for (key, value) in overrides {
            if !SESSION_ENV_PASSTHROUGH.iter().any(|allowed| allowed == key) {
                crate::clog!("spawn env: rejected non-allowlisted key {key:?}");
                continue;
            }
            if let Some((_, existing)) =
                env.iter_mut().find(|(existing_key, _)| existing_key == key)
            {
                *existing = value.clone();
            } else {
                env.push((key.clone(), value.clone()));
            }
        }
        env
    }

    /// Top of the dialog stack — `Some` when a dialog is visible.
    /// Use this instead of inspecting `dialog_stack` directly so the
    /// "is a dialog open" check stays in one place.
    fn dialog_top(&self) -> Option<&Dialog> {
        self.dialog_stack.last()
    }

    fn dialog_top_mut(&mut self) -> Option<&mut Dialog> {
        self.dialog_stack.last_mut()
    }

    /// `true` when at least one dialog is on the stack.
    fn dialog_open(&self) -> bool {
        !self.dialog_stack.is_empty()
    }

    /// Push a new dialog on top of the current one. The previous
    /// dialog stays underneath waiting for an Esc-pop to surface it
    /// again — the standard sub-dialog opening path (Menu → New tab
    /// pushes AgentPicker on top of Menu, not a replacement).
    fn dialog_push(&mut self, d: Dialog) {
        self.dialog_copy_feedback_deadline = None;
        self.dialog_stack.push(d);
    }

    fn open_container_info_dialog(&mut self) {
        let focused_agent = self
            .active_focused_id()
            .and_then(|id| self.sessions.get(&id))
            .and_then(|s| s.agent.clone());
        let container_name = self.status_bar.container_name().to_string();
        self.dialog_push(Dialog::ContainerInfo {
            container_name,
            role: self.status_bar.role().to_string(),
            focused_agent,
            workdir: self.workdir.to_string_lossy().into_owned(),
            copied: false,
        });
    }

    fn open_github_context_dialog(&mut self) {
        self.dialog_push(Dialog::GitHubContext {
            branch: self.pull_request_context_branch.clone(),
            pull_request: self.pull_request_context.clone(),
            pull_request_loading: self.pull_request_context_loading(),
            copied: false,
        });
    }

    fn pull_request_context_loading(&self) -> bool {
        let Some(branch) = self.pull_request_context_branch.as_deref() else {
            return false;
        };
        self.pull_request_lookup.in_flight
            && !self.workdir_context.is_default_branch(branch)
            && self.pull_request_context.is_none()
    }

    fn maybe_spawn_git_branch_context_lookup(&mut self, now: Instant) {
        if !self.workdir_context.git_available {
            return;
        }
        if self.git_branch_lookup.in_flight {
            return;
        }
        if self
            .git_branch_lookup
            .cooldown_active(now, GIT_BRANCH_CONTEXT_POLL_INTERVAL)
        {
            return;
        }
        self.git_branch_lookup.last_run = Some(now);
        let request_id = self.git_branch_lookup.next_request_id();
        self.git_branch_lookup.in_flight = true;
        let workdir = self.workdir.clone();
        self.spawn_context_lookup(
            "git-branch-context",
            move || git_current_branch(&workdir),
            move |branch| SessionEvent::GitBranchContextLoaded { request_id, branch },
        );
    }

    fn maybe_spawn_pull_request_context_lookup(&mut self, now: Instant) {
        if !self.workdir_context.gh_available {
            return;
        }
        if self.pull_request_lookup.in_flight {
            return;
        }
        let Some(branch) = self.pull_request_context_branch.clone() else {
            return;
        };
        if self.workdir_context.is_default_branch(&branch)
            || self.pull_request_cache_is_fresh(&branch, now)
        {
            return;
        }
        let request_id = self.pull_request_lookup.next_request_id();
        self.pull_request_lookup.in_flight = true;
        let workdir = self.workdir.clone();
        let branch_for_event = branch.clone();
        self.spawn_context_lookup(
            "pull-request-context",
            move || match gh_pull_request_info(&workdir, &branch) {
                Ok(pr) => PullRequestLookupOutcome::Resolved(pr),
                Err(err) => {
                    crate::clog!(
                        "pull-request-context: gh lookup failed for branch {:?}: {err}",
                        branch
                    );
                    PullRequestLookupOutcome::TransientFailure
                }
            },
            move |outcome| SessionEvent::PullRequestContextLoaded {
                request_id,
                branch: Some(branch_for_event),
                outcome,
            },
        );
    }

    /// Generic worker spawn for the two background context lookups.
    /// `work` runs the actual `git`/`gh` subprocess (off the daemon's
    /// main thread); `to_event` maps the worker's return value into
    /// the `SessionEvent` variant the main loop dispatches. The
    /// channel-closed `clog!` is uniform across callers so a future
    /// triage of "why didn't the bar refresh?" has the same shape
    /// regardless of which lookup misbehaved.
    fn spawn_context_lookup<F, T, E>(&self, label: &'static str, work: F, to_event: E)
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
        E: FnOnce(T) -> SessionEvent + Send + 'static,
    {
        let event_tx = self.event_tx.clone();
        std::thread::spawn(move || {
            let value = work();
            if event_tx.send(to_event(value)).is_err() {
                crate::clog!("{label}: event channel closed before result reached main loop");
            }
        });
    }

    fn apply_git_branch_context_loaded(
        &mut self,
        request_id: u64,
        branch: Option<String>,
        now: Instant,
    ) -> bool {
        crate::cdebug!(
            "git-branch-context: lookup loaded request_id={} current_request_id={} branch={:?}",
            request_id,
            self.git_branch_lookup.request_id,
            branch
        );
        if request_id != self.git_branch_lookup.request_id {
            return false;
        }
        self.git_branch_lookup.in_flight = false;
        self.apply_git_branch_context(branch, now)
    }

    fn apply_git_branch_context(&mut self, branch: Option<String>, now: Instant) -> bool {
        let old_branch = self.pull_request_context_branch.clone();
        if old_branch == branch {
            self.maybe_spawn_pull_request_context_lookup(now);
            return false;
        }
        let old_pull_request = self.pull_request_context.clone();
        self.pull_request_context_branch = branch.clone();
        if branch.is_some() && !self.workdir_context.is_git_repo {
            self.workdir_context.is_git_repo = true;
            self.workdir_context.default_branch = resolve_default_branch(&self.workdir);
        }
        self.pull_request_context = branch
            .as_deref()
            .and_then(|branch| self.cached_pull_request_for_branch(branch, now));

        // Invalidate any in-flight PR lookup against the previous
        // branch by bumping the request_id token. A worker that fired
        // off `gh pr list --head <old_branch>` could otherwise return
        // ~16s later, pass the stale check, and overwrite the new
        // branch's cache slot with the wrong branch's PR. Bumping the
        // id makes the late-arrival response fail the equality guard
        // in `apply_pull_request_context_loaded`.
        self.pull_request_lookup.request_id = self.pull_request_lookup.request_id.wrapping_add(1);
        self.pull_request_lookup.in_flight = false;
        crate::cdebug!(
            "git-branch-context: branch changed old={:?} new={:?}",
            old_branch,
            self.pull_request_context_branch
        );
        let changed = old_branch != self.pull_request_context_branch
            || old_pull_request != self.pull_request_context;
        let resized = self.reconcile_content_rows();
        self.maybe_spawn_pull_request_context_lookup(now);
        resized || changed
    }

    fn apply_pull_request_context_loaded(
        &mut self,
        request_id: u64,
        branch: Option<String>,
        outcome: PullRequestLookupOutcome,
        now: Instant,
    ) -> bool {
        if request_id != self.pull_request_lookup.request_id {
            return false;
        }
        self.pull_request_lookup.in_flight = false;
        let Some(branch) = branch else {
            return false;
        };
        // Transient gh failures (binary missing, auth not configured,
        // JSON parse, timeout) MUST NOT poison the 60s cache with a
        // synthetic "no PR" answer — operators would lose a real PR
        // for a full minute after every blip. Preserve the previous
        // cached value; the next state-ticker tick retries.
        let pull_request = match outcome {
            PullRequestLookupOutcome::Resolved(pr) => pr,
            PullRequestLookupOutcome::TransientFailure => return false,
        };
        self.purge_expired_pull_request_cache_entries(now);
        self.pull_request_context_cache.insert(
            branch.clone(),
            PullRequestContextCacheEntry {
                checked_at: now,
                pull_request: pull_request.clone(),
            },
        );
        if self.pull_request_context_branch.as_deref() != Some(branch.as_str()) {
            return false;
        }
        let changed = self.pull_request_context != pull_request;
        self.pull_request_context = pull_request;
        if self.reconcile_content_rows() {
            return true;
        }
        changed
    }

    /// Drop cache entries older than `2 * PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL`
    /// so a session that visits many feature branches does not grow the
    /// cache without bound. Two intervals = enough that an "I'm flipping
    /// between two PRs" workflow keeps both warm, while monotonic growth
    /// across hundreds of branches gets pruned.
    fn purge_expired_pull_request_cache_entries(&mut self, now: Instant) {
        let ttl = PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL * 2;
        self.pull_request_context_cache
            .retain(|_, entry| now.duration_since(entry.checked_at) < ttl);
    }

    fn cached_pull_request_for_branch(
        &self,
        branch: &str,
        now: Instant,
    ) -> Option<PullRequestInfo> {
        self.pull_request_context_cache
            .get(branch)
            .filter(|entry| {
                now.duration_since(entry.checked_at) < PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL
            })
            .and_then(|entry| entry.pull_request.clone())
    }

    fn pull_request_cache_is_fresh(&self, branch: &str, now: Instant) -> bool {
        self.pull_request_context_cache
            .get(branch)
            .is_some_and(|entry| {
                now.duration_since(entry.checked_at) < PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL
            })
    }

    /// Current branch for the chrome bar, filtered to `None` when the
    /// operator is on the repo's default branch (resolved at startup
    /// from `origin/HEAD`). Centralising the filter at the render
    /// callsite means the renderer / layout / hit-test helpers can
    /// stay default-branch-agnostic and remain straightforward to
    /// unit-test with literal branch names.
    fn context_bar_branch(&self) -> Option<&str> {
        let branch = self.pull_request_context_branch.as_deref()?;
        if self.workdir_context.is_default_branch(branch) {
            None
        } else {
            Some(branch)
        }
    }

    /// Pop the top dialog. Returns `Some(prev)` when something was on
    /// the stack. The Esc handler uses this for back-navigation:
    /// popping a sub-dialog exposes its parent again rather than
    /// dismissing the whole flow.
    fn dialog_pop_one(&mut self) -> Option<Dialog> {
        let popped = self.dialog_stack.pop();
        if !self
            .dialog_stack
            .last()
            .is_some_and(Dialog::has_copy_feedback)
        {
            self.dialog_copy_feedback_deadline = None;
        }
        popped
    }

    /// Clear every dialog on the stack — used by action paths that
    /// finish the flow (`SpawnAgent` after picking an agent,
    /// destructive confirmations after they fire, etc.) so the
    /// operator returns straight to the focused pane.
    fn dialog_clear(&mut self) {
        self.dialog_stack.clear();
        self.dialog_copy_feedback_deadline = None;
    }

    fn active_tab_pane_count(&self) -> usize {
        self.tabs
            .get(self.active_tab)
            .map(|tab| tab.tree.all_ids().len())
            .unwrap_or_default()
    }

    fn palette_close_label(&self) -> PaletteCloseLabel {
        if self.active_tab_pane_count() == 1 {
            PaletteCloseLabel::CloseTab
        } else {
            PaletteCloseLabel::ChooseTarget
        }
    }

    fn open_command_palette(&mut self) {
        let close_label = self.palette_close_label();
        self.dialog_push(Dialog::new_command_palette(close_label));
    }

    fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.cancel_drag();
        let prev = self.active_focused_id();
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.synthesise_focus_swap(prev, self.active_focused_id());
    }

    fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.cancel_drag();
        let prev = self.active_focused_id();
        self.active_tab = if self.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab - 1
        };
        self.synthesise_focus_swap(prev, self.active_focused_id());
    }

    fn jump_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() && idx != self.active_tab {
            self.cancel_drag();
            let prev = self.active_focused_id();
            self.active_tab = idx;
            self.synthesise_focus_swap(prev, self.active_focused_id());
        }
    }

    /// Drop saved gesture state when the pane geometry it referenced
    /// is about to change. Cheaper than per-motion re-validation.
    fn cancel_drag(&mut self) {
        self.drag = None;
        self.selection = None;
    }

    fn close_focused_tab(&mut self) {
        if self.active_tab >= self.tabs.len() {
            return;
        }
        // Drop any in-flight selection / drag-resize anchored to a
        // pane in this tab — resize_panes below invalidates every
        // remaining pane's rect and removing the active tab swaps the
        // visible content entirely. Mirrors close_focused_pane and
        // remove_exited_session, which both call cancel_drag for the
        // same reason.
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let tab_ids = self.tabs[self.active_tab].tree.all_ids();
        crate::clog!(
            "action: close_focused_tab tab_idx={} pane_count={}",
            self.active_tab,
            tab_ids.len()
        );
        for id in tab_ids {
            if let Some(session) = self.sessions.remove(&id) {
                session.terminate();
            }
            self.pane_body_caches.remove(&id);
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        self.zoomed = None;
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }

    fn exit_all_sessions(&mut self) {
        self.cancel_drag();
        crate::clog!(
            "action: exit_all_sessions session_count={} tab_count={}",
            self.sessions.len(),
            self.tabs.len()
        );
        for (_, session) in self.sessions.drain() {
            session.terminate();
        }
        self.tabs.clear();
        self.active_tab = 0;
        self.zoomed = None;
        self.pane_body_caches.clear();
        self.dirty_panes.clear();
        self.dialog_copy_feedback_deadline = None;
        self.hover_target = None;
    }

    /// Drop the session whose PTY just exited. Removes the pane from
    /// the owning tab's tree, focuses a sibling if any remain, and
    /// removes the tab itself when its last pane is gone. Same
    /// semantic as `close_focused_pane` but driven by the agent
    /// process exiting instead of an explicit operator action — keeps
    /// `○ Done` tabs from piling up after every agent quits.
    ///
    /// When the closed tab was the active one, focus moves to the
    /// tab on the **left**. Operator's mental model: exiting an
    /// agent should return them to whatever they were looking at
    /// before they opened that tab, not to the next-tab-to-the-right
    /// (which feels like a stack push).
    fn remove_exited_session(&mut self, session_id: u64) {
        crate::clog!("action: remove_exited_session id={session_id}");
        // Any in-flight selection / drag-resize was anchored to a
        // pane that may be about to disappear (or whose siblings
        // are about to reflow). Drop both gestures so the next motion
        // event does not paint stale geometry. `cancel_drag` clears
        // selection + drag together; calling it unconditionally is
        // cheaper than per-field re-validation.
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let owning_tab = self
            .tabs
            .iter()
            .position(|t| t.tree.all_ids().contains(&session_id));
        if let Some(tab_idx) = owning_tab {
            let leaves = self.tabs[tab_idx].tree.all_ids();
            let tab_is_empty = leaves.len() == 1 && leaves[0] == session_id;
            if tab_is_empty {
                // `PaneTree::remove` is a no-op on a top-level
                // `Leaf` (no parent split to collapse), so we drop
                // the tab here instead of calling it. Without this
                // branch the tab persists with a dangling session
                // id and the operator sees a `Done` tab they
                // cannot interact with.
                let was_active = tab_idx == self.active_tab;
                self.tabs.remove(tab_idx);
                if was_active {
                    // Move to the tab on the left when it exists;
                    // otherwise stay at index 0 (the leftmost tab
                    // remaining, which was the next-right neighbour
                    // before the removal). `saturating_sub(1)`
                    // collapses both "go left" and "no-left, stay
                    // at 0" into the same expression. Clamp again
                    // so `active_tab` stays in bounds if the last
                    // tab in the strip just vanished.
                    self.active_tab = tab_idx.saturating_sub(1);
                    if self.active_tab >= self.tabs.len() {
                        self.active_tab = self.tabs.len().saturating_sub(1);
                    }
                } else if tab_idx < self.active_tab {
                    // A non-active tab to the left of the active one
                    // vanished; shift `active_tab` down so it keeps
                    // pointing at the same tab.
                    self.active_tab -= 1;
                }
            } else {
                self.tabs[tab_idx].tree.remove(session_id);
                if self.tabs[tab_idx].focused_id == session_id {
                    let remaining = self.tabs[tab_idx].tree.all_ids();
                    if let Some(&next_focus) = remaining.first() {
                        self.tabs[tab_idx].focused_id = next_focus;
                    }
                }
            }
        }
        self.sessions.remove(&session_id);
        self.pane_body_caches.remove(&session_id);
        self.zoomed = self.zoomed.filter(|&id| id != session_id);
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }

    fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.launch_config.model_for_agent(agent)
    }

    fn spawn_request(
        &mut self,
        request: SpawnRequest,
        env_overrides: &[(String, String)],
    ) -> Result<u64> {
        match request {
            SpawnRequest::Agent(agent_slug) => {
                if let Err(reason) =
                    crate::session::validate_agent_slug(&agent_slug, &self.available_agents)
                {
                    anyhow::bail!("rejected agent {agent_slug:?}: {reason}");
                }
                self.spawn_session(Some(agent_slug), env_overrides)
            }
            SpawnRequest::Shell => self.spawn_session(None, env_overrides),
        }
    }

    fn session_launch(
        &self,
        agent: Option<&str>,
        env_passthrough: &[(String, String)],
    ) -> SessionLaunch {
        let cwd = self.workdir.as_path();
        match agent {
            Some(slug) => SessionLaunch {
                label: capitalize(slug),
                cmd: build_agent_command(slug, self.model_for_agent(slug), env_passthrough, cwd),
            },
            None => SessionLaunch {
                label: "Shell".to_string(),
                cmd: build_shell_command(env_passthrough, cwd),
            },
        }
    }

    /// Single dispatch point for a `DialogAction`. Both the
    /// mouse-click and key-event paths call `Dialog::handle_*`
    /// and route the result here, so adding a new variant means
    /// updating one match arm instead of two.
    fn apply_dialog_action(&mut self, action: DialogAction) -> Vec<u8> {
        // Compact breadcrumb (always logged) for the load-bearing
        // dispatch arms — Dismiss, Command, SpawnAgent, RenameTab. The
        // Redraw / Consume arms fire on every arrow key inside a dialog
        // and would swamp the production log; they go through the
        // debug-only `cdebug!` surface so a `--debug` trace shows
        // dialog dispatch landing for arrow keys while quiet runs stay
        // tidy.
        match &action {
            DialogAction::Redraw | DialogAction::Consume => {
                crate::cdebug!("action: dialog={action:?}");
            }
            _ => crate::clog!("action: dialog={action:?}"),
        }
        match action {
            DialogAction::Dismiss => {
                // Back-navigation: pop one dialog so a sub-dialog
                // reveals its parent rather than closing the whole
                // flow. Operator at the top of stack (Menu) pops to
                // an empty stack — same effective "close" the
                // pre-stack code achieved with `self.dialog = None`.
                self.dialog_pop_one();
            }
            DialogAction::Redraw | DialogAction::Consume => {}
            DialogAction::Command(cmd) => {
                // `handle_palette_command` decides per-arm whether
                // the command opens a sub-dialog (push) or finishes
                // the flow (clear stack).
                if let Some(frame) = self.handle_palette_command(cmd) {
                    return frame;
                }
            }
            DialogAction::SpawnAgent { agent, intent } => {
                // Terminal action — agent picked, spawn the session,
                // close every dialog underneath (Menu / Split picker /
                // …) so the operator drops straight onto the new pane.
                self.dialog_clear();
                self.dispatch_spawn_intent(agent, intent);
            }
            DialogAction::RenameTab { tab_idx, label } => {
                self.dialog_clear();
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
                    tab.set_custom_label(label);
                }
            }
            DialogAction::CopyToClipboard(payload) => {
                // OSC 52 selection write — `\x1b]52;c;<base64>\x07`.
                // `c` is the system clipboard target; modern terminals
                // (Ghostty, iTerm2, Kitty, Alacritty, wezterm, recent
                // gnome-terminal) all honour it. Older / locked-down
                // terminals silently drop the sequence — the copy
                // appears to do nothing but no error fires; the
                // multiplexer can't tell from this side. Emitted to
                // the client via `send_output`; the alt-screen path
                // forwards it byte-for-byte to the operator's outer
                // terminal.
                //
                // Copy-capable dialogs stay on the stack — the
                // operator's "did it actually copy?" question is
                // answered by the green "✓ Copied!" badge the renderer
                // paints now that `copied = true` (flipped by the
                // dialog's handle_key or row-click handler before this
                // action returned).
                // The badge expires from the daemon's tick loop.
                self.send_output(encode_osc52_clipboard_write(&payload));
                self.dialog_copy_feedback_deadline =
                    Some(Instant::now() + DIALOG_COPY_FEEDBACK_DURATION);
                return self.compose_dialog_overlay_frame(FullRedrawReason::DialogChange);
            }
            DialogAction::SplitDirection(direction) => {
                // Chain to the agent picker carrying the direction —
                // push it on top of the SplitDirectionPicker so Esc
                // walks the operator one step back instead of
                // closing the whole flow.
                let agents = self.available_agents.clone();
                self.dialog_push(Dialog::new_agent_picker(
                    agents,
                    PickerIntent::Split(direction),
                ));
            }
            DialogAction::PickedCloseTarget(kind) => {
                // Push the ConfirmAction dialog on top of the
                // CloseTargetPicker. Esc walks back to the picker,
                // then back to the Menu — operator can change their
                // mind without destroying anything.
                self.dialog_push(Dialog::ConfirmAction {
                    kind,
                    selected_yes: false,
                });
            }
            DialogAction::ConfirmedAction(kind) => {
                // Terminal action — clear every dialog under us and
                // fire the matching destructive call.
                self.dialog_clear();
                match kind {
                    ConfirmKind::ClosePane => self.close_focused_pane(),
                    ConfirmKind::CloseTab => self.close_focused_tab(),
                    ConfirmKind::Exit => self.exit_all_sessions(),
                }
            }
        }
        self.compose_full_frame(FullRedrawReason::DialogChange)
    }

    /// Single dispatch point for `DialogAction::SpawnAgent`. Spawn
    /// failures (PTY allocation, missing agent binary, cap hit) are
    /// clog'd with their intent and agent label so a `jackin load
    /// --debug` shows the cause; the dialog dismisses regardless so
    /// the operator can retry.
    fn dispatch_spawn_intent(&mut self, agent: Option<String>, intent: PickerIntent) {
        let result: anyhow::Result<()> = match intent {
            PickerIntent::NewTab => self.spawn_session(agent.clone(), &[]).map(|_| ()),
            PickerIntent::Split(direction) => {
                self.split_focused_into(direction, agent.clone(), &[])
            }
        };
        if let Err(err) = result {
            let agent_label = agent.as_deref().unwrap_or("shell");
            crate::clog!("spawn ({intent:?}, agent={agent_label}) failed: {err:?}");
            // Surface to the attach client too — otherwise the dialog
            // closes successfully and the operator sees no new pane and
            // no explanation.
            let banner = spawn_failure_banner(&format!("{agent_label}: {err:#}"));
            self.send_output(banner);
        }
    }

    fn spawn_session(
        &mut self,
        agent: Option<String>,
        env_overrides: &[(String, String)],
    ) -> Result<u64> {
        // Bound the per-container surface so a runaway client (or an
        // operator mis-click loop) cannot allocate unbounded PTYs.
        // Each session retains ~SCROLLBACK_LEN lines of scrollback,
        // a master+slave PTY pair, and a child process — at MAX_TABS
        // sessions the container memory footprint is still well
        // under typical limits, but well past the size any operator
        // can usefully navigate.
        self.ensure_capacity_for_new_session(true)?;
        // Mirror split_focused_into: resize_panes below reflows every
        // pane's interior rect, and the new tab swaps the visible
        // content. Drop any in-flight gesture anchored to a now-stale
        // pane rect so the next mouse-motion does not paint selection
        // or splitter feedback against geometry that has moved.
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let env_passthrough = self.env_for_spawn(env_overrides);
        let launch = self.session_launch(agent.as_deref(), &env_passthrough);
        let (session, id) = Session::spawn(
            &launch.label,
            agent.clone(),
            launch.cmd,
            self.content_rows.saturating_sub(2),
            self.term_cols.saturating_sub(2),
            self.event_tx.clone(),
        )?;
        let tab_label = launch.label.clone();
        self.sessions.insert(id, session);
        if self.tabs.is_empty() {
            self.tabs.push(Tab::new_single(tab_label, id));
            self.active_tab = 0;
        } else {
            self.tabs.push(Tab::new_single(tab_label, id));
            self.active_tab = self.tabs.len() - 1;
        }
        // Reflow so the new pane's PTY gets the correct interior
        // dimensions (outer rect minus border rows/cols). Without
        // this, the session keeps its initial `content_rows ×
        // term_cols` guess and the agent draws its bottom rows
        // past the pane's bottom border.
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, Some(id));
        crate::clog!(
            "action: spawn_session id={id} agent={:?} label={label} tab_idx={tab_idx}",
            agent,
            label = launch.label,
            tab_idx = self.active_tab
        );
        Ok(id)
    }

    /// Bound the per-container surface for any path that allocates a
    /// new PTY (top-level spawn, split, etc.). All such paths must
    /// route through here so `MAX_TABS` / `MAX_SESSIONS` are enforced
    /// uniformly — runaway-mis-click defence. `add_tab=true` enforces
    /// both caps; `add_tab=false` enforces only `MAX_SESSIONS` because
    /// the caller is reusing an existing tab.
    fn ensure_capacity_for_new_session(&self, add_tab: bool) -> Result<()> {
        if add_tab && self.tabs.len() >= MAX_TABS {
            anyhow::bail!("tab limit reached ({MAX_TABS}); close one before spawning another");
        }
        if self.sessions.len() >= MAX_SESSIONS {
            anyhow::bail!(
                "pane limit reached ({MAX_SESSIONS}); close some panes before opening more"
            );
        }
        Ok(())
    }

    /// Split the focused pane and spawn a session of the operator's
    /// choice inside it. `agent_slug = None` opens a shell. Used by
    /// the AgentPicker → Split flow so the operator picks the new
    /// pane's identity instead of cloning the source pane's agent.
    fn split_focused_into(
        &mut self,
        direction: SplitDirection,
        agent_slug: Option<String>,
        env_overrides: &[(String, String)],
    ) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        // Any selection / drag-resize is anchored to a specific pane
        // rect that this reflow is about to invalidate.
        self.cancel_drag();
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Ok(());
        };
        let from_id = tab.focused_id;
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let from_rect = tab
            .tree
            .leaves(content_rect)
            .into_iter()
            .find(|(id, _)| *id == from_id)
            .map(|(_, r)| r)
            .unwrap_or(content_rect);
        let (spawn_rows, spawn_cols) = match direction {
            SplitDirection::Left | SplitDirection::Right => (
                from_rect.rows.saturating_sub(2),
                (from_rect.cols / 2).saturating_sub(2),
            ),
            SplitDirection::Above | SplitDirection::Below => (
                (from_rect.rows / 2).saturating_sub(2),
                from_rect.cols.saturating_sub(2),
            ),
        };
        let env_passthrough = self.env_for_spawn(env_overrides);
        let launch = self.session_launch(agent_slug.as_deref(), &env_passthrough);
        let agent_for_log = agent_slug.clone();
        let (session, new_id) = Session::spawn(
            &launch.label,
            agent_slug,
            launch.cmd,
            spawn_rows,
            spawn_cols,
            self.event_tx.clone(),
        )?;
        self.sessions.insert(new_id, session);
        let tab = &mut self.tabs[self.active_tab];
        let placed = match direction {
            SplitDirection::Left => tab.tree.split_h(from_id, new_id, SplitPosition::Before),
            SplitDirection::Right => tab.tree.split_h(from_id, new_id, SplitPosition::After),
            SplitDirection::Above => tab.tree.split_v(from_id, new_id, SplitPosition::Before),
            SplitDirection::Below => tab.tree.split_v(from_id, new_id, SplitPosition::After),
        };
        if !placed {
            // from_id vanished between split intent and dispatch
            // (e.g. the source pane exited mid-action). Undo the
            // session insert so the spawned PTY + child + tasks do
            // not leak as an orphan that no tab tree references.
            if let Some(orphan) = self.sessions.remove(&new_id) {
                orphan.terminate();
            }
            crate::clog!(
                "action: split aborted — from_id={from_id} no longer in tab tree; reaped orphan id={new_id}",
            );
            return Ok(());
        }
        tab.focused_id = new_id;
        self.resize_panes();
        self.synthesise_focus_swap(Some(from_id), Some(new_id));
        crate::clog!(
            "action: split id={new_id} from={from_id} dir={direction:?} agent={agent_for_log:?} label={label}",
            label = launch.label,
        );
        Ok(())
    }

    /// Split the focused pane and clone the source pane's agent into
    /// the new pane. Used by the `Ctrl+B %` / `Ctrl+B "` prefix
    /// bindings so split-and-spawn skips the agent picker and inherits
    /// the source pane's runtime.
    fn split_focused(&mut self, direction: SplitDirection) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Ok(());
        };
        let from_id = tab.focused_id;
        let agent_slug = self.sessions.get(&from_id).and_then(|s| s.agent.clone());
        self.split_focused_into(direction, agent_slug, &[])
    }

    fn close_focused_pane(&mut self) {
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let id = tab.focused_id;
        let all = tab.tree.all_ids();
        let next_focus = all.iter().find(|&&sid| sid != id).copied();
        crate::clog!(
            "action: close_focused_pane id={id} tab_idx={} siblings_remaining={}",
            self.active_tab,
            next_focus.is_some()
        );
        tab.tree.remove(id);
        if let Some(session) = self.sessions.remove(&id) {
            session.terminate();
        }
        self.pane_body_caches.remove(&id);
        // Drop the zoomed reference when the killed pane was the zoom
        // target so the next `compose_frame` does not paint a stale
        // zoom area until the operator manually unzooms.
        self.zoomed = self.zoomed.filter(|&zid| zid != id);
        if let Some(nf) = next_focus {
            tab.focused_id = nf;
        } else {
            self.tabs.remove(self.active_tab);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            }
        }
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }

    fn toggle_zoom(&mut self) {
        // Zoom is a single global field but scoped per-tab via
        // `active_zoomed_id`. Toggling has to consult the *active*
        // tab's zoom state — checking the raw `self.zoomed.is_some()`
        // would let a toggle on Tab B unzoom whatever Tab A had
        // pinned, surprising the operator on their next switch back
        // to Tab A. Use `active_zoomed_id` so unzoom only fires when
        // the active tab actually owns the zoom; otherwise zoom the
        // active tab's focused pane.
        let focused = self.tabs.get(self.active_tab).map(|t| t.focused_id);
        let was_zoomed = self.active_zoomed_id().is_some();
        self.zoomed = if was_zoomed { None } else { focused };
        self.resize_panes();
        crate::clog!(
            "action: toggle_zoom from={was_zoomed} to={} focused={focused:?}",
            self.zoomed.is_some()
        );
    }

    fn resize_panes(&mut self) {
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        if let Some(zoom_id) = self.active_zoomed_id() {
            let inner = content_rect.shrink(1);
            if let Some(session) = self.sessions.get_mut(&zoom_id) {
                session.resize(inner.rows, inner.cols);
            }
            return;
        }
        for tab in &self.tabs {
            let leaves = tab.tree.leaves(content_rect);
            for (id, rect) in leaves {
                let inner = rect.shrink(1);
                if let Some(session) = self.sessions.get_mut(&id) {
                    session.resize(inner.rows, inner.cols);
                }
            }
        }
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = normalize_size(rows, cols);
        // Outer-terminal resize invalidates the drag's saved rect.
        self.cancel_drag();
        self.term_rows = rows;
        self.term_cols = cols;
        self.content_rows = self.available_content_rows();
        self.resize_panes();
    }

    fn bottom_chrome_rows(&self) -> u16 {
        BRANCH_CONTEXT_BAR_ROWS
    }

    fn available_content_rows(&self) -> u16 {
        self.term_rows
            .saturating_sub(STATUS_BAR_ROWS)
            .saturating_sub(self.bottom_chrome_rows())
    }

    fn reconcile_content_rows(&mut self) -> bool {
        let next = self.available_content_rows();
        if next == self.content_rows {
            return false;
        }
        self.content_rows = next;
        self.pane_body_caches.clear();
        self.resize_panes();
        true
    }

    fn active_focused_id(&self) -> Option<u64> {
        self.tabs.get(self.active_tab).map(|t| t.focused_id)
    }

    /// `self.zoomed` narrowed to "only when the zoomed session belongs
    /// to the active tab." The zoom field is global (one value across
    /// all tabs), but render / input / scroll / mouse paths must
    /// behave as if zoom is per-tab — switching tabs has to surface
    /// the new tab's panes normally even when a different tab still
    /// has a zoomed session pinned, otherwise opening a new tab paints
    /// the previously-zoomed pane full-screen. Returning `None` from
    /// the active-tab check routes every consumer of zoom state
    /// through the normal multi-pane path for tabs that don't hold
    /// the zoom.
    fn active_zoomed_id(&self) -> Option<u64> {
        let zoom_id = self.zoomed?;
        let tab = self.tabs.get(self.active_tab)?;
        if tab.tree.all_ids().contains(&zoom_id) {
            Some(zoom_id)
        } else {
            None
        }
    }

    fn active_focused_outer_rect(&self) -> Option<Rect> {
        let focused = self.active_focused_id()?;
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        if let Some(zoom_id) = self.active_zoomed_id() {
            return (zoom_id == focused).then_some(content_rect);
        }
        self.tabs
            .get(self.active_tab)?
            .tree
            .leaves(content_rect)
            .into_iter()
            .find(|(id, _)| *id == focused)
            .map(|(_, rect)| rect)
    }

    fn active_focused_inner_rect(&self) -> Option<Rect> {
        self.active_focused_outer_rect().map(|rect| rect.shrink(1))
    }

    /// Derive the label that should appear in the tab strip for `tab`
    /// given the current pane contents. Operator's mental model is
    /// "what kinds of things am I running in here?", so the label
    /// tracks the kind makeup instead of pinning to the first
    /// session spawned: a single-agent tab carries that agent's
    /// name; shells-only is `Shell`; two distinct agents is
    /// `Agents`; any agent + any shell is `Mix`.
    fn tab_display_label(&self, tab: &Tab) -> String {
        let ids = tab.tree.all_ids();
        let pane_count = ids.len();
        let mut agent_slugs: Vec<String> = Vec::new();
        let mut has_shell = false;
        for id in ids {
            if let Some(s) = self.sessions.get(&id) {
                match &s.agent {
                    Some(slug) => {
                        if !agent_slugs.iter().any(|s| s == slug) {
                            agent_slugs.push(slug.clone());
                        }
                    }
                    None => has_shell = true,
                }
            }
        }
        let base = match (agent_slugs.len(), has_shell) {
            (0, _) => "Shell".to_string(),
            (1, false) => capitalize(&agent_slugs[0]),
            (_, false) => "Agents".to_string(),
            (_, true) => "Mix".to_string(),
        };
        if pane_count > 1 {
            format!("{base} ({pane_count})")
        } else {
            base
        }
    }

    /// Rewrite each tab's auto-label after a spawn / split / remove.
    /// `Tab::label()` reads `custom_label` first, so operator-typed
    /// names survive this refresh automatically. Cheap (clones a few
    /// short strings) and easier to reason about than dispatching
    /// incremental updates from every mutation site.
    fn refresh_tab_labels(&mut self) {
        let mut new_labels = Vec::with_capacity(self.tabs.len());
        for tab in &self.tabs {
            new_labels.push(self.tab_display_label(tab));
        }
        for (tab, label) in self.tabs.iter_mut().zip(new_labels) {
            tab.set_auto_label(label);
        }
    }

    /// True when there are no sessions left.
    /// `sessions.is_empty()` covers the operator-explicitly-killed-all
    /// case; `all !alive` covers the natural-exit case (every agent /
    /// shell process closed its PTY).
    fn no_live_sessions(&self) -> bool {
        self.sessions.is_empty()
    }

    fn request_pane_body_redraw(&mut self, session_id: u64) {
        if self.pending_full_redraw.is_none() {
            self.dirty_panes.insert(session_id);
        }
    }

    fn request_full_redraw(&mut self, reason: FullRedrawReason) {
        self.pending_full_redraw = Some(reason);
        self.dirty_panes.clear();
    }

    fn has_pending_render(&self) -> bool {
        self.pending_full_redraw.is_some() || !self.dirty_panes.is_empty()
    }

    fn expire_dialog_copy_feedback(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.dialog_copy_feedback_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.dialog_copy_feedback_deadline = None;
        self.dialog_top_mut()
            .is_some_and(Dialog::clear_copy_feedback)
    }

    fn visible_panes(&self) -> Vec<VisiblePane> {
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let focused_id = self.active_focused_id();
        let dim_panes = self.dialog_open();
        if let Some(zoom_id) = self.active_zoomed_id() {
            let outer = content_rect;
            return vec![VisiblePane {
                id: zoom_id,
                outer,
                inner: outer.shrink(1),
                focused: Some(zoom_id) == focused_id,
                dim: dim_panes,
            }];
        }
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Vec::new();
        };
        let leaves = tab.tree.leaves(content_rect);
        let multi_pane = leaves.len() > 1;
        leaves
            .into_iter()
            .map(|(id, outer)| {
                let focused = Some(id) == focused_id;
                VisiblePane {
                    id,
                    outer,
                    inner: outer.shrink(1),
                    focused,
                    dim: dim_panes || (multi_pane && !focused),
                }
            })
            .collect()
    }

    /// Adjust the split that contains the focused pane along `dir` by
    /// 5% of the parent rectangle. Triggered by `Alt+Shift+Arrow`.
    fn resize_focused(&mut self, dir: ArrowDir) {
        let Some(tab_idx) = self.tabs.get(self.active_tab).map(|_| self.active_tab) else {
            return;
        };
        let focused = self.tabs[tab_idx].focused_id;
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        if self.tabs[tab_idx].tree.resize(focused, d, 0.05) {
            self.resize_panes();
        }
    }

    fn move_focus(&mut self, dir: ArrowDir) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        let prev = tab.focused_id;
        if let Some(next_id) = tab.tree.adjacent(content_rect, tab.focused_id, d) {
            self.tabs[self.active_tab].focused_id = next_id;
            self.synthesise_focus_swap(Some(prev), Some(next_id));
        }
    }

    /// Synthesise `\x1b[O` / `\x1b[I` to track which pane the operator
    /// is actually looking at. Agents that watch focus events use them
    /// to pause polling / animations; without synthesis, a backgrounded
    /// pane thinks it is still focused.
    ///
    /// Also re-emits the newly focused session's mode state
    /// (bracketed paste, etc.) so the outer terminal matches what
    /// the now-visible agent wants. Each agent owns its own mode
    /// state and switching tabs must not leak the previous agent's
    /// setup to the new one.
    fn synthesise_focus_swap(&mut self, old: Option<u64>, new: Option<u64>) {
        if old == new {
            return;
        }
        // Synthetic `\x1b[I` / `\x1b[O` to the agent's PTY only
        // when the agent enabled focus-event reporting (DEC ?1004).
        // Shells and pre-mount agents leave it off; writing the
        // bytes into their PTY would surface as literal `[I` /
        // `[O` text at the prompt.
        if let Some(o) = old
            && let Some(s) = self.sessions.get(&o)
            && s.focus_events_enabled()
        {
            s.send_input(b"\x1b[O");
        }
        if let Some(n) = new
            && let Some(s) = self.sessions.get(&n)
        {
            if s.focus_events_enabled() {
                s.send_input(b"\x1b[I");
            }
            // Reset the outer terminal to a known baseline, then
            // re-emit every mode the new pane wants live.
            // Mouse/focus are reasserted as client-owned modes so a
            // pane cannot downgrade the multiplexer's input channel
            // to legacy X10 after a focus-changing close/split.
            if self.attached_out.is_some() {
                let mut frames: Vec<Vec<u8>> = Vec::new();
                frames.push(crate::session::Session::focus_swap_reset().to_vec());
                frames.push(crate::session::Session::client_owned_mode_state().to_vec());
                for bytes in s.current_mode_state() {
                    frames.push(bytes);
                }
                for bytes in frames {
                    self.send_output(bytes);
                }
            }
        }
    }

    /// Handle a parsed input event from the client terminal.
    /// Returns bytes to send to the client (e.g. redraws), if any.
    fn handle_input(&mut self, event: InputEvent) -> Option<Vec<u8>> {
        if let InputEvent::MousePress { col, row, button }
        | InputEvent::MouseRelease { col, row, button } = &event
        {
            if let Some(frame) = self.update_hover_for_mouse(*row, *col) {
                self.send_output(frame);
            }
            self.update_pointer_shape_for_mouse(*row, *col, *button);
        }
        match event {
            InputEvent::OpenPalette => {
                // Toggle: opening the palette key while any dialog is
                // already on the stack closes the whole flow (faster
                // than walking back with Esc). Operator opens fresh
                // when the stack was empty.
                self.cancel_drag();
                if self.dialog_open() {
                    self.dialog_clear();
                } else {
                    self.open_command_palette();
                }
                Some(self.compose_full_frame(FullRedrawReason::PaletteOverlay))
            }
            InputEvent::PrefixCommand(cmd) => {
                // While a dialog is open the prefix gesture's payload
                // must not reach the focused pane — operator's intent
                // is to act on the dialog, not the agent underneath.
                if self.dialog_open() {
                    return None;
                }
                self.handle_prefix_command(cmd)
            }
            InputEvent::ResizePane(dir) => {
                if self.dialog_open() {
                    return None;
                }
                self.resize_focused(dir);
                Some(self.compose_full_frame(FullRedrawReason::LayoutChange))
            }
            InputEvent::FocusIn | InputEvent::FocusOut => {
                // Forward only when the focused agent actually
                // requested focus events (`?1004h`) — shells and
                // pre-mount agents leave the mode off and would
                // surface `[I` / `[O` as literal text at the prompt.
                if self.dialog_open() {
                    return None;
                }
                let bytes = if matches!(event, InputEvent::FocusIn) {
                    b"\x1b[I".as_ref()
                } else {
                    b"\x1b[O".as_ref()
                };
                if let Some(focused) = self.active_focused_id()
                    && let Some(session) = self.sessions.get(&focused)
                    && session.focus_events_enabled()
                {
                    session.send_input(bytes);
                }
                None
            }
            InputEvent::MousePress { col, row, button }
                if self.dialog_open() && button == 0 && !is_wheel_button(button) =>
            {
                // Mouse handling while a dialog overlay is up:
                //   click on a row  → select + confirm
                //   click on border / padding → swallowed
                //   click anywhere outside the box → dismiss
                //
                // SGR mouse coords are 0-based; `box_rect` returns
                // render-side coords that are 1-based (the values
                // passed to `move_to`, which emits `\x1b[r;cH`).
                // Pass row+1 / col+1 here so `handle_click` compares
                // apples to apples — otherwise a click on the
                // dialog's top border or leftmost column reads as
                // outside-the-box and dismisses the dialog.
                let term_rows = self.term_rows;
                let term_cols = self.term_cols;
                let action = self
                    .dialog_top_mut()
                    .expect("dialog presence checked")
                    .handle_click(row + 1, col + 1, term_rows, term_cols);
                Some(self.apply_dialog_action(action))
            }
            InputEvent::MousePress { .. } if self.dialog_open() => {
                // Any non-wheel mouse event with the dialog up that
                // did not land on a row is swallowed so it never
                // reaches the agent underneath.
                None
            }
            InputEvent::MouseRelease { .. } if self.dialog_open() => {
                // Drop the release that pairs with a press the dialog
                // already absorbed. Letting it through would surface
                // the raw `\x1b[<...m` bytes at the focused pane's
                // prompt as garbage text the moment the dialog
                // dismisses (e.g. click-outside-to-close).
                None
            }
            InputEvent::MouseRelease { col, row, button } => {
                // End an in-flight pane resize on left-button release.
                // Drop the PTY forward so the source agent does not
                // see a half-paired release in the middle of a drag.
                if self.drag.is_some() && (button & 0b11) == 0 {
                    self.drag = None;
                    return Some(self.compose_full_frame(FullRedrawReason::LayoutChange));
                }
                // Commit any active text selection: copy to clipboard
                // and clear the highlight.
                if self.selection.is_some() && (button & 0b11) == 0 {
                    return self.finalize_selection();
                }
                self.forward_mouse_to_focused_pane_with_kind(col, row, button, false);
                None
            }
            InputEvent::MousePress { col, row, button } if is_wheel_button(button) => {
                // Panes that requested mouse reporting own their wheel
                // events. Alternate-screen panes without mouse reporting
                // get cursor-key fallback regardless of retained primary-
                // screen scrollback — the pane controls the surface, not
                // jackin'. Normal-screen panes without mouse reporting use
                // jackin's scrollback when available; otherwise the wheel
                // is silenced so it does not become prompt cursor keys.
                // Routing is based on screen state, not spawn origin.
                if self.dialog_open() {
                    return None;
                }
                if self.forward_mouse_to_focused_pane_with_kind(col, row, button, true) {
                    crate::cdebug!(
                        "wheel dispatch: forwarded-to-pty row={} col={} button={}",
                        row,
                        col,
                        button
                    );
                    return None;
                }
                let delta = if (button & 1) == 0 { 3 } else { -3 };
                let focused = self.active_focused_id()?;
                let session = self.sessions.get_mut(&focused)?;
                let debug_enabled = crate::logging::debug_enabled();
                let (filled, vt_filled, inline_filled) = if debug_enabled {
                    let (vt_filled, inline_filled) = session.scrollback_counts();
                    (
                        vt_filled.saturating_add(inline_filled),
                        vt_filled,
                        inline_filled,
                    )
                } else {
                    (session.scrollback_filled(), 0, 0)
                };
                if let Some(fallback_reason) = pane_wheel_cursor_fallback_reason(session)
                    && let Some(buf) = encode_wheel_cursor_fallback(session, button)
                {
                    crate::cdebug!(
                        "wheel dispatch: cursor-fallback session={} agent={:?} row={} col={} button={} scrollback_filled={} reason={} bytes={:02x?}",
                        focused,
                        session.agent,
                        row,
                        col,
                        button,
                        filled,
                        fallback_reason,
                        buf
                    );
                    session.send_input(&buf);
                    return None;
                }
                if filled == 0 {
                    crate::cdebug!(
                        "wheel dispatch: no-scrollback session={} agent={:?} row={} col={} button={} alt_screen={} mouse_enabled={} vt_scrollback={} inline_scrollback={}",
                        focused,
                        session.agent,
                        row,
                        col,
                        button,
                        session.screen().alternate_screen(),
                        session.mouse_enabled(),
                        vt_filled,
                        inline_filled
                    );
                    return None;
                }
                crate::cdebug!(
                    "wheel dispatch: jackin-scrollback session={} row={} col={} button={} delta={} before={} filled={}",
                    focused,
                    row,
                    col,
                    button,
                    delta,
                    session.scrollback_offset,
                    filled
                );
                session.scroll_by(delta);
                crate::cdebug!(
                    "wheel dispatch: jackin-scrollback session={} after={}",
                    focused,
                    session.scrollback_offset
                );
                Some(self.compose_full_frame(FullRedrawReason::ScrollbackMovement))
            }
            InputEvent::MousePress {
                row,
                col,
                button: 0,
            } if branch_context_bar_hit(
                row + 1,
                col + 1,
                self.term_rows,
                self.term_cols,
                self.context_bar_branch(),
                self.pull_request_context.as_ref(),
                self.pull_request_context_loading(),
                self.status_bar.instance_id_label(),
            )
            .is_some() =>
            {
                match branch_context_bar_hit(
                    row + 1,
                    col + 1,
                    self.term_rows,
                    self.term_cols,
                    self.context_bar_branch(),
                    self.pull_request_context.as_ref(),
                    self.pull_request_context_loading(),
                    self.status_bar.instance_id_label(),
                ) {
                    Some(BranchContextBarHit::Context) => self.open_github_context_dialog(),
                    Some(BranchContextBarHit::Container) => self.open_container_info_dialog(),
                    None => {}
                }
                Some(self.compose_dialog_overlay_frame(FullRedrawReason::DialogChange))
            }
            InputEvent::MousePress {
                row: 0,
                col,
                button: 0,
            } => {
                // 1) Click on a tab cell switches active tab. A
                //    second click on the same cell within the
                //    double-click window opens the rename modal.
                if let Some(idx) = self.status_bar.tab_at_col(col + 1)
                    && idx < self.tabs.len()
                {
                    let now = std::time::Instant::now();
                    let is_double = self
                        .last_tab_click
                        .filter(|(prev_idx, prev_t)| {
                            *prev_idx == idx && now.duration_since(*prev_t) <= DOUBLE_CLICK_WINDOW
                        })
                        .is_some();
                    if is_double {
                        self.cancel_drag();
                        let initial = self.tabs[idx]
                            .custom_label()
                            .map(str::to_owned)
                            .unwrap_or_default();
                        let input = jackin_tui::TextField::new(initial)
                            .with_max_chars(crate::dialog::MAX_CUSTOM_LABEL_LEN);
                        self.dialog_push(Dialog::RenameTab {
                            tab_idx: idx,
                            input,
                        });
                        self.last_tab_click = None;
                        return Some(self.compose_full_frame(FullRedrawReason::DialogChange));
                    }
                    self.last_tab_click = Some((idx, now));
                    if idx != self.active_tab {
                        self.cancel_drag();
                        let prev = self.active_focused_id();
                        self.active_tab = idx;
                        self.synthesise_focus_swap(prev, self.active_focused_id());
                        return Some(self.compose_full_frame(FullRedrawReason::TabSwitch));
                    }
                    return None;
                }
                if self.status_bar.hint_at(1, col + 1) {
                    self.open_command_palette();
                    return Some(self.compose_full_frame(FullRedrawReason::PaletteOverlay));
                }
                None
            }
            InputEvent::MousePress { col, row, button } => {
                // SGR motion event with the left button still held
                // (`button == 32`) drives an in-flight resize drag or
                // selection drag if one is active. Treat it as the
                // drag/selection update path; do not focus-switch or
                // forward to PTY.
                if button == 32 {
                    if self.drag.is_some() {
                        return self.drag_motion(row, col);
                    }
                    if self.selection.is_some() {
                        return self.selection_motion(row, col);
                    }
                    // No drag / selection in flight: motion events
                    // belong to the focused pane only if it asked
                    // for any-event tracking (`?1003h`) or
                    // button-motion tracking (`?1002h`). Forwarding
                    // them blindly would dump SGR bytes into shells
                    // that ignored mouse mode.
                    self.forward_mouse_to_focused_pane(col, row, button);
                    return None;
                }
                if button == 0 {
                    // Press on a shared pane border starts a drag —
                    // skip focus switch and PTY forward in that case.
                    if let Some(state) = self.detect_drag_start(row, col) {
                        self.drag = Some(state);
                        return None;
                    }
                    // Click on a pane other than the currently-focused
                    // one switches focus first so the operator never
                    // has to click twice (once to focus, once to act).
                    // Selection or PTY-mouse forwarding then runs
                    // against the freshly-focused pane.
                    let switched_focus = self.focus_pane_at(row, col);
                    // Press inside a pane whose program never asked
                    // for a mouse protocol starts a text selection.
                    if let Some(state) = self.detect_selection_start(row, col) {
                        self.selection = Some(state);
                        return Some(self.compose_full_frame(FullRedrawReason::SelectionRepaint));
                    }
                    self.forward_mouse_to_focused_pane(col, row, button);
                    return if switched_focus {
                        Some(self.compose_full_frame(FullRedrawReason::FocusChange))
                    } else {
                        None
                    };
                }
                self.forward_mouse_to_focused_pane(col, row, button);
                None
            }
            InputEvent::Data(bytes) => {
                if let Some(dialog) = self.dialog_top_mut() {
                    let action = dialog.handle_key(&bytes);
                    Some(self.apply_dialog_action(action))
                } else {
                    // Any keyboard input from the operator returns the
                    // focused pane to the live tail. Matches the
                    // common multiplexer convention that "I'm typing
                    // again" implies "show me what's happening now."
                    let mut snapped = false;
                    let mut unblocked = false;
                    if let Some(focused) = self.active_focused_id()
                        && let Some(session) = self.sessions.get_mut(&focused)
                    {
                        if session.scrollback_offset != 0 {
                            session.scroll_to_live();
                            snapped = true;
                        }
                        unblocked = session.mark_operator_input();
                        session.send_input(&bytes);
                    }
                    if snapped || unblocked {
                        let reason = if snapped {
                            FullRedrawReason::ScrollbackMovement
                        } else {
                            FullRedrawReason::ExplicitRedraw
                        };
                        Some(self.compose_full_frame(reason))
                    } else {
                        None
                    }
                }
            }
        }
    }

    fn handle_prefix_command(&mut self, cmd: PrefixCommand) -> Option<Vec<u8>> {
        // Action breadcrumb: every prefix-key chord lands here, so one
        // line per dispatch is enough to reconstruct what the operator
        // pressed when triaging a bug report. The Debug formatter
        // includes any payload (`JumpTab(i)`, `MoveFocus(dir)`).
        crate::clog!("action: prefix={cmd:?}");
        let full_redraw_reason = prefix_full_redraw_reason(&cmd);
        match cmd {
            PrefixCommand::NewTab => {
                let agents = self.available_agents.clone();
                self.dialog_push(Dialog::new_agent_picker(agents, PickerIntent::NewTab));
            }
            PrefixCommand::NextTab => self.next_tab(),
            PrefixCommand::PrevTab => self.prev_tab(),
            PrefixCommand::JumpTab(i) => self.jump_tab(i),
            PrefixCommand::SplitTopBottom => {
                if let Err(err) = self.split_focused(SplitDirection::Below) {
                    crate::clog!("split (top/bottom) failed: {err:?}");
                }
            }
            PrefixCommand::SplitSideBySide => {
                if let Err(err) = self.split_focused(SplitDirection::Right) {
                    crate::clog!("split (side by side) failed: {err:?}");
                }
            }
            PrefixCommand::MoveFocus(dir) => self.move_focus(dir),
            PrefixCommand::ZoomToggle => self.toggle_zoom(),
            PrefixCommand::KillPane => self.close_focused_pane(),
            PrefixCommand::KillTab => self.close_focused_tab(),
            PrefixCommand::ClearPane => self.clear_focused_pane(),
            PrefixCommand::Detach => {
                self.detach_requested = true;
            }
            PrefixCommand::Palette => {
                self.open_command_palette();
            }
            PrefixCommand::Redraw => {}
        }
        Some(self.compose_full_frame(full_redraw_reason))
    }

    fn forward_mouse_to_focused_pane(&mut self, col: u16, row: u16, button: u8) -> bool {
        self.forward_mouse_to_focused_pane_with_kind(col, row, button, true)
    }

    /// Re-encode an SGR mouse event in the focused pane's local
    /// coordinate space and forward to its PTY. `press = true` emits
    /// the `M` final, `false` emits `m` (release). Forwarding is
    /// gated by the focused pane's requested mouse mode so shells and
    /// pre-mount agents never see raw mouse bytes leak out as
    /// command-line garbage, and press-only panes do not receive
    /// motion events from the multiplexer's always-on outer tracking.
    fn forward_mouse_to_focused_pane_with_kind(
        &mut self,
        col: u16,
        row: u16,
        button: u8,
        press: bool,
    ) -> bool {
        let Some(focused) = self.active_focused_id() else {
            return false;
        };
        let Some(session) = self.sessions.get(&focused) else {
            return false;
        };
        let Some(encoding) = mouse_event_encoding_for_session(session, button, press) else {
            return false;
        };
        let Some(inner) = self.active_focused_inner_rect() else {
            return false;
        };
        if row < inner.row || row >= inner.row + inner.rows {
            return false;
        }
        if col < inner.col || col >= inner.col + inner.cols {
            return false;
        }
        let local_row = row - inner.row;
        let local_col = col - inner.col;
        let Some(buf) =
            encode_mouse_for_protocol(button, local_col + 1, local_row + 1, press, encoding)
        else {
            return false;
        };
        session.send_input(&buf);
        true
    }

    /// Zoomed tabs never produce a drag — there are no shared
    /// borders to grab on a single visible pane.
    fn detect_drag_start(&self, row: u16, col: u16) -> Option<DragState> {
        if row < STATUS_BAR_ROWS || self.active_zoomed_id().is_some() {
            return None;
        }
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let tab = self.tabs.get(self.active_tab)?;
        let (path, orient, rect) = tab.tree.border_at(content_rect, row, col)?;
        Some(DragState {
            tab_idx: self.active_tab,
            path,
            orient,
            rect,
        })
    }

    /// Test whether the click at `(row, col)` lands inside the inner
    /// content area of a pane whose program never opted into a
    /// mouse protocol. If so, this is the start of a text selection
    /// (zellij-style "drag in shell pane → copy to clipboard").
    fn detect_selection_start(&self, row: u16, col: u16) -> Option<SelectionState> {
        if row < STATUS_BAR_ROWS {
            return None;
        }
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let (id, outer) = if let Some(zoom_id) = self.active_zoomed_id() {
            (zoom_id, content_rect)
        } else {
            let tab = self.tabs.get(self.active_tab)?;
            tab.tree.leaves(content_rect).into_iter().find(|(_, r)| {
                row >= r.row && row < r.row + r.rows && col >= r.col && col < r.col + r.cols
            })?
        };
        let inner = outer.shrink(1);
        if row < inner.row
            || row >= inner.row + inner.rows
            || col < inner.col
            || col >= inner.col + inner.cols
        {
            return None;
        }
        let session = self.sessions.get(&id)?;
        if session.mouse_enabled() {
            // Pane's program wants the mouse — defer to PTY forward.
            return None;
        }
        let anchor_row = row - inner.row;
        let anchor_col = col - inner.col;
        Some(SelectionState {
            session_id: id,
            inner,
            anchor_row,
            anchor_col,
            end_row: anchor_row,
            end_col: anchor_col,
        })
    }

    /// Update the active selection's end-cell to the new motion
    /// position. Clamps to the inner pane rect so a drag that leaves
    /// the pane still produces a reasonable highlight.
    fn selection_motion(&mut self, row: u16, col: u16) -> Option<Vec<u8>> {
        let sel = self.selection.as_mut()?;
        let inner = sel.inner;
        let clamped_row = row.clamp(inner.row, inner.row + inner.rows.saturating_sub(1));
        let clamped_col = col.clamp(inner.col, inner.col + inner.cols.saturating_sub(1));
        sel.end_row = clamped_row - inner.row;
        sel.end_col = clamped_col - inner.col;
        Some(self.compose_full_frame(FullRedrawReason::SelectionRepaint))
    }

    /// Commit the active selection: extract the selected text from
    /// the source session's `vt100` grid, emit OSC 52 to the
    /// attached client (which the outer terminal turns into a
    /// real clipboard write), and clear the highlight.
    fn finalize_selection(&mut self) -> Option<Vec<u8>> {
        let sel = self.selection.take()?;
        // Suppress single-cell selections: a click-to-focus with no
        // drag motion lands anchor==end and would otherwise OSC 52
        // whatever character sat under the cursor — a silent host-
        // clipboard overwrite on every focus click.
        let dragged = sel.anchor_row != sel.end_row || sel.anchor_col != sel.end_col;
        if dragged && let Some(session) = self.sessions.get(&sel.session_id) {
            let text = selection_text(session.screen(), &sel);
            if !text.is_empty() && self.attached_out.is_some() {
                let bytes = encode_osc52_clipboard_write(&text);
                self.send_output(bytes);
            }
        }
        Some(self.compose_full_frame(FullRedrawReason::SelectionRepaint))
    }

    /// Apply a drag motion at `(row, col)` against the active drag's
    /// split. Recomputes the ratio from the mouse position relative
    /// to the saved split rectangle, clamps to `[0.05, 0.95]`, then
    /// reflows the panes so the agent PTYs resize in step.
    fn drag_motion(&mut self, row: u16, col: u16) -> Option<Vec<u8>> {
        let drag = self.drag.clone()?;
        let new_ratio = match drag.orient {
            SplitOrient::Horizontal => {
                let off = col.saturating_sub(drag.rect.col);
                (off as f32 / drag.rect.cols as f32).clamp(0.05, 0.95)
            }
            SplitOrient::Vertical => {
                let off = row.saturating_sub(drag.rect.row);
                (off as f32 / drag.rect.rows as f32).clamp(0.05, 0.95)
            }
        };
        let tab = self.tabs.get_mut(drag.tab_idx)?;
        if !tab.tree.set_ratio_at(&drag.path, new_ratio) {
            return None;
        }
        self.resize_panes();
        Some(self.compose_full_frame(FullRedrawReason::LayoutChange))
    }

    /// Switch focus to the pane the operator clicked on, if it differs
    /// from the current focus. Returns `true` when the focus actually
    /// changed so the caller can trigger a redraw.
    ///
    /// Honours the zoomed-pane state: when a pane is zoomed it fills
    /// the entire content rect, so clicks inside that rect must
    /// resolve to the zoomed pane even if a sibling pane's unzoomed
    /// rect happens to cover the click point. Walking `tab.tree.leaves`
    /// without this guard sends focus (and subsequent keystrokes) to
    /// a hidden pane while the zoomed pane stays painted as focused.
    fn focus_pane_at(&mut self, row: u16, col: u16) -> bool {
        if row < STATUS_BAR_ROWS {
            return false;
        }
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return false;
        };
        let prev = tab.focused_id;
        if let Some(zoom_id) = self.active_zoomed_id() {
            // Click outside the content rect (header chrome, status
            // bar) cannot affect zoom focus; otherwise the only
            // candidate is the zoomed pane itself.
            if row < content_rect.row + content_rect.rows
                && col < content_rect.col + content_rect.cols
                && zoom_id != prev
            {
                self.tabs[self.active_tab].focused_id = zoom_id;
                self.synthesise_focus_swap(Some(prev), Some(zoom_id));
                return true;
            }
            return false;
        }
        let leaves = tab.tree.leaves(content_rect);
        for (id, rect) in leaves {
            if row >= rect.row
                && row < rect.row + rect.rows
                && col >= rect.col
                && col < rect.col + rect.cols
                && id != prev
            {
                self.tabs[self.active_tab].focused_id = id;
                self.synthesise_focus_swap(Some(prev), Some(id));
                return true;
            }
        }
        false
    }

    fn handle_palette_command(&mut self, cmd: PaletteCommand) -> Option<Vec<u8>> {
        // Per-arm decision: sub-dialog openings push onto the dialog
        // stack (Menu stays underneath for Esc → back); terminal
        // actions clear the stack and run the action. No blanket
        // clear at the top because that would prevent the sub-dialog
        // back-navigation chain from working.
        match cmd {
            PaletteCommand::Split => {
                // Open the SplitDirectionPicker sub-dialog. The
                // operator picks the direction; that resolves to a
                // `DialogAction::SplitDirection(...)` which
                // `apply_dialog_action` chains into an `AgentPicker`
                // carrying `PickerIntent::Split(direction)`. Final
                // confirm spawns the new pane.
                self.dialog_push(Dialog::SplitDirectionPicker {
                    selected: 0,
                    filter: String::new(),
                });
            }
            PaletteCommand::NewTab => {
                // Always show the agent picker — even when the role
                // declares a single agent. The operator must
                // explicitly choose between that agent and a Shell;
                // jumping straight into the agent would surprise an
                // operator who picked "New tab" to open a shell.
                let agents = self.available_agents.clone();
                self.dialog_push(Dialog::new_agent_picker(agents, PickerIntent::NewTab));
            }
            PaletteCommand::NextTab => {
                self.dialog_clear();
                self.next_tab();
            }
            PaletteCommand::PrevTab => {
                self.dialog_clear();
                self.prev_tab();
            }
            PaletteCommand::Close => {
                if self.active_tab_pane_count() == 1 {
                    self.dialog_push(Dialog::ConfirmAction {
                        kind: ConfirmKind::CloseTab,
                        selected_yes: false,
                    });
                } else {
                    // Drill-down: push the CloseTargetPicker on top
                    // of the Menu so split tabs still ask whether
                    // the operator wants the focused pane or every
                    // pane in the tab. Esc walks back to Menu.
                    self.dialog_push(Dialog::CloseTargetPicker {
                        selected: 0,
                        filter: String::new(),
                    });
                }
            }
            PaletteCommand::ZoomPane => {
                self.dialog_clear();
                self.toggle_zoom();
            }
            PaletteCommand::ClearPane => {
                self.dialog_clear();
                self.clear_focused_pane();
                return Some(self.compose_full_frame(FullRedrawReason::PaneClear));
            }
            PaletteCommand::Exit => {
                // Push ConfirmAction for Exit — the operator
                // confirms before every agent session is stopped. Esc
                // walks back to Menu.
                self.dialog_push(Dialog::ConfirmAction {
                    kind: ConfirmKind::Exit,
                    selected_yes: false,
                });
            }
        }
        None
    }

    fn clear_focused_pane(&mut self) {
        self.cancel_drag();
        if let Some(id) = self.active_focused_id()
            && let Some(session) = self.sessions.get_mut(&id)
        {
            session.clear_scrollback_and_request_screen_clear();
            self.pane_body_caches.remove(&id);
            self.dirty_panes.remove(&id);
        }
    }

    fn compose_pending_frame(&mut self) -> Vec<u8> {
        if let Some(reason) = self.pending_full_redraw.take() {
            self.dirty_panes.clear();
            return self.compose_full_frame(reason);
        }
        let dirty_panes = std::mem::take(&mut self.dirty_panes);
        self.compose_partial_frame(dirty_panes)
    }

    fn append_outer_terminal_title(&mut self, buf: &mut Vec<u8>) {
        let title = compose_outer_terminal_title(
            &self.workdir,
            self.pull_request_context_branch.as_deref(),
            self.pull_request_context.as_ref(),
        );
        if self.last_outer_terminal_title.as_deref() == Some(title.as_str()) {
            return;
        }
        append_osc_window_title(buf, &title);
        self.last_outer_terminal_title = Some(title);
    }

    fn compose_full_frame(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        let started = Instant::now();
        let mut buf = Vec::with_capacity(65536);
        self.append_outer_terminal_title(&mut buf);
        buf.extend_from_slice(b"\x1b[?25l");
        let dialog_dim = self.dialog_open();

        // Tab labels track the pane makeup. Done here (not on every
        // spawn / split / remove) so the rule lives in one place.
        self.refresh_tab_labels();

        let states: Vec<(u64, AgentState)> =
            self.sessions.iter().map(|(&id, s)| (id, s.state)).collect();
        self.status_bar.render(
            &mut buf,
            self.term_cols,
            &self.tabs,
            self.active_tab,
            &states,
            hovered_tab(self.hover_target),
            hovered_menu(self.hover_target),
            dialog_dim,
        );

        let focused_id = self.active_focused_id();
        let mut focused_pane_rect: Option<Rect> = None;
        let panes = self.visible_panes();
        let multi_pane = panes.len() > 1;
        let zoomed = self.active_zoomed_id().is_some();
        let mut pane_rows_emitted = 0usize;
        let mut pane_body_bytes = 0usize;

        for pane in &panes {
            let mut scrollbar = PaneScrollbar::default();
            let mut title = None;
            if let Some(session) = self.sessions.get_mut(&pane.id) {
                scrollbar = pane_scrollbar(session, pane.inner.rows, pane.inner.cols);
                title = Some(display_title(session));
                let scrollback_prefix = session.scrollback_render_prefix(pane.inner.rows);
                let before = buf.len();
                let stats = self
                    .pane_body_caches
                    .entry(pane.id)
                    .or_default()
                    .render_full_with_scrollback_prefix(
                        session.screen(),
                        &scrollback_prefix,
                        pane.inner.row,
                        pane.inner.col,
                        pane.inner.rows,
                        pane.inner.cols,
                        pane.dim,
                        &mut buf,
                    );
                pane_rows_emitted += stats.rows_emitted;
                pane_body_bytes += buf.len() - before;
                if pane.focused {
                    focused_pane_rect = Some(pane.inner);
                }
            }
            if let Some(title) = title {
                // Always draw a pane box, even for the single-pane
                // case — matches zellij's "every pane is framed"
                // convention and gives the operator a reliable place
                // to read the live `OSC 2` title.
                let highlight_focus = if zoomed {
                    scrollbar.visible()
                } else {
                    multi_pane || scrollbar.visible()
                };
                draw_pane_box(
                    &mut buf,
                    pane.outer.row,
                    pane.outer.col,
                    pane.outer.rows,
                    pane.outer.cols,
                    &title,
                    pane.focused && highlight_focus && !dialog_dim,
                    dialog_dim,
                );
                draw_scrollbar(
                    &mut buf,
                    pane.outer.row,
                    pane.outer.col,
                    pane.outer.rows,
                    pane.outer.cols,
                    scrollbar.offset,
                    scrollbar.filled,
                    pane.focused && highlight_focus && !dialog_dim,
                );
            }
        }

        if !zoomed {
            // Paint the selection highlight on top of pane content
            // (but underneath the pane box so the inverse stops at
            // the inner edge). The selection lives on a specific
            // pane, so resolve the screen + inner rect once.
            if let Some(sel) = &self.selection
                && let Some(session) = self.sessions.get(&sel.session_id)
            {
                paint_selection_highlight(&mut buf, session.screen(), sel);
            }
        }

        render_branch_context_bar(
            &mut buf,
            self.term_rows,
            self.term_cols,
            self.context_bar_branch(),
            self.pull_request_context.as_ref(),
            self.pull_request_context_loading(),
            self.status_bar.instance_id_label(),
            self.hover_target,
            dialog_dim,
        );

        if let Some(dialog) = self.dialog_top() {
            dialog.render_with_hover(
                &mut buf,
                self.term_rows,
                self.term_cols,
                self.hover_target == Some(HoverTarget::DialogCopyTarget),
            );
            dialog.render_footer_hint(&mut buf, self.term_rows, self.term_cols);
        }

        self.append_cursor_state(&mut buf, focused_id, focused_pane_rect);

        crate::cdebug!(
            "render: kind=full reason={} panes={} rows={} pane_bytes={} bytes={} duration_us={}",
            reason.as_str(),
            panes.len(),
            pane_rows_emitted,
            pane_body_bytes,
            buf.len(),
            started.elapsed().as_micros()
        );

        buf
    }

    fn compose_dialog_overlay_frame(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        // Dialog overlays always go through the full compositor so the
        // background dim cue, status chrome, branch bar, and bottom
        // footer hint stay consistent for every dialog type.
        self.compose_full_frame(reason)
    }

    fn compose_chrome_hover_frame(&mut self) -> Vec<u8> {
        let mut buf = b"\x1b7".to_vec();
        let states: Vec<(u64, AgentState)> =
            self.sessions.iter().map(|(&id, s)| (id, s.state)).collect();
        self.status_bar.render(
            &mut buf,
            self.term_cols,
            &self.tabs,
            self.active_tab,
            &states,
            hovered_tab(self.hover_target),
            hovered_menu(self.hover_target),
            self.dialog_open(),
        );
        render_branch_context_bar(
            &mut buf,
            self.term_rows,
            self.term_cols,
            self.context_bar_branch(),
            self.pull_request_context.as_ref(),
            self.pull_request_context_loading(),
            self.status_bar.instance_id_label(),
            self.hover_target,
            self.dialog_open(),
        );
        buf.extend_from_slice(b"\x1b8");
        buf
    }

    fn compose_partial_frame(&mut self, dirty_panes: HashSet<u64>) -> Vec<u8> {
        if dirty_panes.is_empty() {
            return Vec::new();
        }
        if self.dialog_open() || self.selection.is_some() {
            return self.compose_full_frame(FullRedrawReason::UnsafePartial);
        }

        let started = Instant::now();
        let panes = self.visible_panes();
        let focused_id = self.active_focused_id();
        let focused_pane_rect = panes
            .iter()
            .find(|pane| pane.focused)
            .map(|pane| pane.inner);

        if !panes.iter().any(|pane| dirty_panes.contains(&pane.id)) {
            crate::cdebug!(
                "render: kind=partial reason=pty-output dirty_panes={} panes=0 rows=0 pane_bytes=0 bytes=0 duration_us={}",
                dirty_panes.len(),
                started.elapsed().as_micros()
            );
            return Vec::new();
        }

        for pane in panes.iter().filter(|pane| dirty_panes.contains(&pane.id)) {
            let Some(session) = self.sessions.get(&pane.id) else {
                continue;
            };
            if session.scrollback_offset != 0 {
                return self.compose_full_frame(FullRedrawReason::ScrollbackMovement);
            }
            if !self
                .pane_body_caches
                .get(&pane.id)
                .is_some_and(|cache| cache.is_valid_for(pane.inner.rows, pane.inner.cols, pane.dim))
            {
                return self.compose_full_frame(FullRedrawReason::PaneCacheMiss);
            }
        }

        let mut buf = Vec::with_capacity(16384);
        self.append_outer_terminal_title(&mut buf);
        buf.extend_from_slice(b"\x1b[?25l");
        let mut rows_emitted = 0usize;
        let mut panes_rendered = 0usize;
        let mut pane_body_bytes = 0usize;
        let multi_pane = panes.len() > 1;
        let zoomed = self.active_zoomed_id().is_some();
        for pane in panes.iter().filter(|pane| dirty_panes.contains(&pane.id)) {
            let mut scrollbar = PaneScrollbar::default();
            let mut title = None;
            if let Some(session) = self.sessions.get_mut(&pane.id) {
                scrollbar = pane_scrollbar(session, pane.inner.rows, pane.inner.cols);
                title = Some(display_title(session));
                let before = buf.len();
                let stats = self
                    .pane_body_caches
                    .entry(pane.id)
                    .or_default()
                    .render_partial(
                        session.screen(),
                        pane.inner.row,
                        pane.inner.col,
                        pane.inner.rows,
                        pane.inner.cols,
                        pane.dim,
                        &mut buf,
                    );
                if stats.mode == PaneBodyRenderMode::Full {
                    return self.compose_full_frame(FullRedrawReason::PaneCacheMiss);
                }
                if stats.rows_emitted > 0 {
                    panes_rendered += 1;
                }
                rows_emitted += stats.rows_emitted;
                pane_body_bytes += buf.len() - before;
            }
            if let Some(title) = title {
                let highlight_focus = if zoomed {
                    scrollbar.visible()
                } else {
                    multi_pane || scrollbar.visible()
                };
                draw_pane_box(
                    &mut buf,
                    pane.outer.row,
                    pane.outer.col,
                    pane.outer.rows,
                    pane.outer.cols,
                    &title,
                    pane.focused && highlight_focus,
                    false,
                );
                draw_scrollbar(
                    &mut buf,
                    pane.outer.row,
                    pane.outer.col,
                    pane.outer.rows,
                    pane.outer.cols,
                    scrollbar.offset,
                    scrollbar.filled,
                    pane.focused && highlight_focus,
                );
            }
        }

        self.append_cursor_state(&mut buf, focused_id, focused_pane_rect);

        crate::cdebug!(
            "render: kind=partial reason=pty-output dirty_panes={} panes={} rows={} pane_bytes={} bytes={} duration_us={}",
            dirty_panes.len(),
            panes_rendered,
            rows_emitted,
            pane_body_bytes,
            buf.len(),
            started.elapsed().as_micros()
        );

        buf
    }

    fn append_cursor_state(
        &self,
        buf: &mut Vec<u8>,
        focused_id: Option<u64>,
        focused_pane_rect: Option<Rect>,
    ) {
        // Position cursor at the focused pane's screen cursor only when
        // the pane has something the operator can actually type into.
        // Show conditions, all must hold:
        //   1. No dialog is open (already gated above).
        //   2. Focused session has produced PTY output. A pane that
        //      just spawned (or split-into-shell that hasn't drawn its
        //      first prompt yet) paints a stray blinking cursor at
        //      `(0, 0)` of an empty rectangle otherwise.
        //   3. The agent did not request cursor hidden (`\x1b[?25l`).
        //   4. The operator is not browsing scrollback — the live VT
        //      cursor position is meaningless against history rows.
        // When any rule fails we emit `\x1b[?25l` so no second cursor
        // remains visible anywhere else in the multiplexer chrome.
        if !self.dialog_open() {
            let mut showed = false;
            if let (Some(fid), Some(rect)) = (focused_id, focused_pane_rect)
                && let Some(session) = self.sessions.get(&fid)
            {
                let screen = session.screen();
                let live_input = session.received_output
                    && session.scrollback_offset == 0
                    && !screen.hide_cursor();
                if live_input {
                    let (vt_row, vt_col) = screen.cursor_position();
                    use std::io::Write as _;
                    let _ = write!(
                        buf,
                        "\x1b[{};{}H",
                        rect.row + vt_row + 1,
                        rect.col + vt_col + 1
                    );
                    buf.extend_from_slice(b"\x1b[?25h");
                    showed = true;
                }
            }
            if !showed {
                buf.extend_from_slice(b"\x1b[?25l");
            }
        }
    }

    fn session_infos(&self) -> Vec<SessionInfo> {
        let focused = self.active_focused_id();
        self.sessions
            .iter()
            .map(|(&id, s)| SessionInfo {
                id,
                label: s.label.clone(),
                agent: s.agent.clone(),
                state: s.state,
                active: Some(id) == focused,
            })
            .collect()
    }

    /// Switch the active tab to whichever tab contains the leaf
    /// carrying `session_id`, and set that tab's `focused_id` to
    /// `session_id`. Returns `true` when the search succeeded;
    /// `false` when no tab references the id, leaving state
    /// untouched.
    fn focus_session_globally(&mut self, session_id: u64) -> bool {
        use crate::layout::Rect;
        let probe_rect = Rect::new(0, 0, self.term_rows, self.term_cols);
        let prev_focused = self.active_focused_id();
        for (tab_idx, tab) in self.tabs.iter().enumerate() {
            let leaf_ids: Vec<u64> = tab
                .tree
                .leaves(probe_rect)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            if leaf_ids.contains(&session_id) {
                self.active_tab = tab_idx;
                self.tabs[tab_idx].focused_id = session_id;
                self.synthesise_focus_swap(prev_focused, Some(session_id));
                return true;
            }
        }
        false
    }

    /// Build a tab/pane tree snapshot for the host console's preview
    /// pane. The leaf order matches `PaneTree::leaves` so the operator
    /// sees panes in the same left-to-right / top-to-bottom order the
    /// multiplexer renders. Missing sessions (race against a kill)
    /// fall back to a placeholder so the snapshot still covers every
    /// leaf the tree references — the host UI can dim those rows.
    fn tab_snapshots(&self) -> Vec<crate::protocol::control::TabSnapshot> {
        use crate::layout::Rect;
        use crate::protocol::control::{PaneSnapshot, TabSnapshot};
        let placeholder_rect = Rect::new(0, 0, self.term_rows, self.term_cols);
        self.tabs
            .iter()
            .map(|tab| {
                let panes = tab
                    .tree
                    .leaves(placeholder_rect)
                    .into_iter()
                    .map(|(id, _)| match self.sessions.get(&id) {
                        Some(session) => PaneSnapshot {
                            session_id: id,
                            label: session.label.clone(),
                            agent: session.agent.clone(),
                            state: session.state,
                        },
                        None => PaneSnapshot {
                            session_id: id,
                            label: "(missing)".to_string(),
                            agent: None,
                            state: crate::protocol::control::AgentState::Idle,
                        },
                    })
                    .collect();
                TabSnapshot {
                    label: tab.label_owned(),
                    focused_pane: tab.focused_id,
                    panes,
                }
            })
            .collect()
    }
}

/// Run the multiplexer daemon. Called from `main` when PID == 1.
pub async fn run_daemon(initial_agent: String, launch_config: CapsuleConfig) -> Result<()> {
    crate::pid1::install_sigchld_reaper();

    let rows = std::env::var("JACKIN_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ROWS);
    let cols = std::env::var("JACKIN_COLS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_COLS);
    let (rows, cols) = normalize_size(rows, cols);

    // Initialise the file logger before anything else can emit a
    // diagnostic. Failures fall back to stderr-only, so this is safe
    // to call unconditionally.
    crate::logging::init();
    crate::clog!(
        "daemon start: rows={rows} cols={cols} initial_agent={initial_agent:?} workdir={}",
        launch_config.workdir.as_str()
    );

    let mut mux = Multiplexer::new(rows, cols, launch_config);
    // Defer the first pane until the first attach Hello has supplied
    // real outer-terminal dimensions. Later panes already spawn after
    // attach-time resize; routing the first pane through the same
    // path removes first-tab-only scrollback/chrome differences.
    let mut pending_initial_spawn = Some(initial_spawn_request(&initial_agent));

    let mut new_clients = socket::start_listener()?;
    let mut branch_context_ticker = interval(GIT_BRANCH_CONTEXT_POLL_INTERVAL);
    let mut state_ticker = interval(Duration::from_secs(1));
    // Render ticker: ~30 fps. Coalesces PTY-output bursts into one
    // frame per tick. With 4+ panes producing output continuously,
    // composing immediately on every event spent more time emitting
    // SGR bytes than the client could draw, which read as visible
    // multiplexer lag. zellij uses the same coalescing pattern.
    let mut render_ticker = interval(Duration::from_millis(33));
    render_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    // Inbound: attach handler tasks → main loop.
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ClientFrame>();
    // Inbound: spawned handshake tasks → main loop. The spawned task
    // owns the slow `read_exact` for the first byte + Hello frame so
    // a silent client cannot stall the main `select!`. Validated
    // handshakes ride this channel back to the main loop, which then
    // applies the take-over + spawns the persistent attach task.
    let (handshake_tx, mut handshake_rx) = mpsc::unbounded_channel::<AttachHandshake>();

    // Resolve the operator's escape-time once at startup; the value
    // cannot change after daemon launch, so per-iteration env reads
    // would be wasted syscalls. A present-but-unparseable env var
    // emits a debug line so the operator sees their config rejected
    // rather than silently falling back to the default.
    let escape_time = match std::env::var(ENV_ESCAPE_TIME) {
        Ok(raw) => match raw.parse::<u64>() {
            Ok(ms) => Duration::from_millis(ms),
            Err(_) => {
                crate::clog!(
                    "{ENV_ESCAPE_TIME}={raw:?} ignored (not a positive integer); using default {} ms",
                    DEFAULT_ESCAPE_TIME.as_millis()
                );
                DEFAULT_ESCAPE_TIME
            }
        },
        Err(_) => DEFAULT_ESCAPE_TIME,
    };

    // Persistent escape-time deadline. Set when the parser first
    // enters `EscStart` (one Esc with no follow-up yet). Cleared once
    // the parser leaves `EscStart` (because either the rest of a CSI
    // sequence arrived or `flush_pending_esc` ran).
    //
    // Recomputing this each iteration as `now() + escape_time` is
    // wrong: a chatty PTY (a TUI agent with a spinner) wakes the
    // select loop dozens of times per second, and a fresh deadline
    // each wake-up never lapses before the next PTY output resets it.
    let mut esc_deadline: Option<tokio::time::Instant> = None;
    loop {
        if mux.input_parser.esc_pending() {
            if esc_deadline.is_none() {
                esc_deadline = Some(tokio::time::Instant::now() + escape_time);
            }
        } else {
            esc_deadline = None;
        }
        tokio::select! {
            biased;

            _ = sigterm.recv() => {
                detach_client(&mut mux).await;
                return Ok(());
            }
            _ = sigint.recv() => {
                detach_client(&mut mux).await;
                return Ok(());
            }

            // New socket connection — spawn the handshake off the
            // main loop so a client that connects but never sends the
            // first byte does not stall PTY processing, ticks, or
            // signal handling. The spawned task either handles the
            // control channel inline (one-shot reply, closes the
            // socket) or forwards a validated attach Hello back via
            // `handshake_tx`.
            Some((stream, client_permit)) = new_clients.recv() => {
                let handshake_tx = handshake_tx.clone();
                let sessions_snapshot = mux.session_infos();
                let tabs_snapshot = mux.tab_snapshots();
                let active_tab = u32::try_from(mux.active_tab).unwrap_or(0);
                tokio::spawn(perform_handshake(
                    stream,
                    client_permit,
                    handshake_tx,
                    sessions_snapshot,
                    tabs_snapshot,
                    active_tab,
                ));
            }

            // Validated attach handshake from the spawned handshake task.
            Some(ready) = handshake_rx.recv() => {
                let AttachHandshake {
                    stream,
                    rows,
                    cols,
                    spawn,
                    env,
                    terminal,
                    focus_session,
                    client_permit,
                } = ready;
                mux.resize(rows, cols);
                mux.pointer_shapes_supported = terminal.pointer_shapes_supported();
                mux.attached_terminal = terminal;
                mux.pointer_shape = PointerShape::Default;
                if mux.sessions.is_empty()
                    && let Some(request) = pending_initial_spawn.take()
                    && let Err(err) = mux.spawn_request(request.clone(), &[])
                {
                    crate::clog!(
                        "initial spawn failed (request={}): {err:#}",
                        spawn_request_label(&request)
                    );
                    return Err(err);
                }
                if let Some(target) = focus_session
                    && !mux.focus_session_globally(target)
                {
                    crate::clog!(
                        "attach: ignoring unknown focus_session={target} (no matching pane)"
                    );
                }
                // Honor a spawn intent from `jackin-capsule new
                // <agent>` / `jackin-capsule new` (shell). Spawn
                // failures get clog'd and surfaced to the new client
                // as an Output frame after Welcome so the operator
                // sees the reason in their terminal — silently
                // landing on an empty multiplexer would otherwise be
                // indistinguishable from "no spawn requested".
                let mut spawn_failure: Option<String> = None;
                if let Some(request) = spawn {
                    let label = spawn_request_label(&request);
                    if let Err(err) = mux.spawn_request(request, &env) {
                        crate::clog!("attach: spawn {label} failed: {err:#}");
                        spawn_failure = Some(format!("spawn {label} failed: {err:#}"));
                    }
                }
                // Take over from any existing attach client. The shared
                // helper sends Shutdown, gives the writer side a brief
                // drain window, then aborts the old reader task.
                detach_attached_task(&mut mux, "takeover").await;
                // Drain any stale frames the old client task pushed
                // into cmd_tx before its abort actually took effect —
                // without this drain, the next `cmd_rx.recv()` after
                // the new attach is wired processes Input / Resize /
                // Detach against the NEW mux state. The abort + drain
                // pair must stay single-threaded in this order: by the
                // time `try_recv` runs the old task can no longer be
                // scheduled, so the loop bound is exactly "everything
                // the old task already enqueued." On a first-attach
                // (no prior task) cmd_rx is already empty.
                let mut drained = 0u32;
                while cmd_rx.try_recv().is_ok() {
                    drained = drained.saturating_add(1);
                }
                if drained > 0 {
                    crate::clog!("takeover: drained {drained} stale frame(s) from prior client");
                }
                let (new_out_tx, new_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
                mux.attached_out = Some(new_out_tx.clone());
                mux.attached_out_dead_logged = false;
                // Build the initial-attach burst as a typed list so a
                // typo at one call site cannot disagree with the clog
                // label. A send failure here means the receiver was
                // closed by a takeover/cancellation race in the same
                // tick; log the first failure so a wedged first-frame
                // queue is observable in the multiplexer log instead
                // of silently leaving the operator's terminal blank.
                let mut initial_frames: Vec<(InitialFrameKind, Vec<u8>)> = Vec::with_capacity(5);
                initial_frames.push((
                    InitialFrameKind::Welcome,
                    encode_server(ServerFrame::Welcome {
                        session_count: mux.sessions.len() as u32,
                    }),
                ));
                // Re-assert the attach-client-owned mouse/focus modes,
                // then restore the focused session's modes (bracketed
                // paste, etc.). Without this, a re-attach loses
                // bracketed-paste and the operator's clipboard arrives
                // unwrapped.
                initial_frames.push((
                    InitialFrameKind::ClientOwnedModes,
                    encode_server(ServerFrame::Output(
                        Session::client_owned_mode_state().to_vec(),
                    )),
                ));
                if let Some(focused) = mux.active_focused_id()
                    && let Some(session) = mux.sessions.get(&focused)
                {
                    for bytes in session.current_mode_state() {
                        initial_frames.push((
                            InitialFrameKind::FocusedPaneModes,
                            encode_server(ServerFrame::Output(bytes)),
                        ));
                    }
                }
                let mut initial = b"\x1b[2J".to_vec();
                initial.extend(mux.compose_full_frame(FullRedrawReason::FirstAttach));
                initial_frames.push((
                    InitialFrameKind::FirstAttach,
                    encode_server(ServerFrame::Output(initial)),
                ));
                if let Some(reason) = spawn_failure {
                    initial_frames.push((
                        InitialFrameKind::SpawnFailureBanner,
                        encode_server(ServerFrame::Output(spawn_failure_banner(&reason))),
                    ));
                }
                let first_failure = initial_frames
                    .into_iter()
                    .find_map(|(kind, bytes)| new_out_tx.send(bytes).err().map(|_| kind));
                if let Some(kind) = first_failure {
                    crate::clog!(
                        "attach: receiver closed before initial frame ({}); operator's terminal will not paint",
                        kind.label()
                    );
                    mux.attached_out_dead_logged = true;
                }
                let cmd_tx_for_task = cmd_tx.clone();
                mux.attached_task = Some(tokio::spawn(async move {
                    handle_attach_client(stream, new_out_rx, cmd_tx_for_task).await;
                    // Hold the concurrency permit alive for the
                    // lifetime of the attach task. Dropping at the
                    // end of the spawned future returns a slot to
                    // the listener's Semaphore.
                    drop(client_permit);
                }));
            }

            // Inbound attach frame from the active client task.
            Some(frame) = cmd_rx.recv() => {
                handle_client_frame(&mut mux, frame).await;
                if mux.detach_requested {
                    mux.detach_requested = false;
                    detach_client(&mut mux).await;
                }
                if mux.no_live_sessions() {
                    drain_and_exit(&mut mux).await;
                    return Ok(());
                }
            }

            // PTY output or exit event from a session.
            Some(event) = mux.event_rx.recv() => {
                match event {
                    SessionEvent::Output { session_id, data } => {
                        let focused_id = mux.active_focused_id();
                        let is_focused = Some(session_id) == focused_id;
                        // Collect any focused-pane output into local
                        // vecs so the `&mut Session` borrow ends before
                        // `mux.send_output` (which takes `&mut Multiplexer`).
                        let mut to_emit: Vec<Vec<u8>> = Vec::new();
                        let mut reassert_outer_terminal_title = false;
                        if let Some(session) = mux.sessions.get_mut(&session_id) {
                            session.feed_pty(&data);
                            // Always drain the OSC + unhandled-CSI
                            // passthrough buffer so a backgrounded
                            // agent emitting OSC 7 / OSC 9 / OSC 8 on
                            // every prompt does not grow `pending`
                            // unboundedly until it becomes focused.
                            // Forward the drained bytes ONLY when this
                            // session is the focused pane —
                            // backgrounded panes' notifications,
                            // clipboard writes, and titles must not
                            // reach the operator's outer terminal.
                            let drained = session.drain_passthrough();
                            // Mode-state transitions (bracketed paste,
                            // etc.) round-trip through the outer
                            // terminal. Drain regardless of focus for
                            // the same reason; on focus swap,
                            // `current_mode_state()` restores the
                            // destination pane's full mode set in one
                            // shot, so intermediate transitions of
                            // backgrounded panes do not need to leak
                            // out (and would be silently dropped here
                            // anyway).
                            let mode_transitions = session.drain_mode_transitions();
                            if is_focused {
                                reassert_outer_terminal_title = !drained.is_empty();
                                to_emit.extend(drained);
                                to_emit.extend(mode_transitions);
                            }
                        }
                        for bytes in to_emit {
                            mux.send_output(bytes);
                        }
                        if reassert_outer_terminal_title {
                            mux.last_outer_terminal_title = None;
                        }
                        // Mark the pane body dirty; the render ticker coalesces
                        // bursts of PTY output into one frame per
                        // tick. Dialog-open still invalidates — the
                        // render ticker now paints the dialog overlay
                        // against the latest pane state, so dismiss
                        // doesn't produce a sudden burst of
                        // accumulated frames.
                        mux.request_pane_body_redraw(session_id);
                    }
                    SessionEvent::Exited { session_id } => {
                        // Remove the pane / tab immediately rather than
                        // leaving a stale `○ Done` placeholder behind.
                        // Matches the operator's mental model: "agent
                        // exited → its tab is gone."
                        mux.remove_exited_session(session_id);
                        mux.request_full_redraw(FullRedrawReason::SessionExit);
                        // When the last live session exits — whether
                        // the operator typed `/exit` in the agent or
                        // the agent crashed — there is nothing left to
                        // attach to. Tear down the container so the
                        // host cleanup path fires.
                        if mux.no_live_sessions() {
                            drain_and_exit(&mut mux).await;
                            return Ok(());
                        }
                    }
                    SessionEvent::GitBranchContextLoaded { request_id, branch } => {
                        if mux.apply_git_branch_context_loaded(request_id, branch, Instant::now()) {
                            mux.request_full_redraw(FullRedrawReason::StatusChange);
                        }
                    }
                    SessionEvent::PullRequestContextLoaded {
                        request_id,
                        branch,
                        outcome,
                    } => {
                        if mux.apply_pull_request_context_loaded(
                            request_id,
                            branch,
                            outcome,
                            Instant::now(),
                        ) {
                            mux.request_full_redraw(FullRedrawReason::StatusChange);
                        }
                    }
                }
            }

            // Escape-time fired: the operator's `\x1b` did not get a
            // follow-up byte in time, so emit it as a bare Data event.
            // Dialogs treat it as dismiss; agents see the lone Esc.
            _ = async {
                match esc_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            }, if esc_deadline.is_some() => {
                esc_deadline = None;
                let events = mux.input_parser.flush_pending_esc();
                for event in events {
                    if let Some(redraw) = mux.handle_input(event) {
                        mux.send_output(redraw);
                    }
                }
            }

            // Render ticker: drain dirty pane bodies or a named full-frame
            // invalidation at ~30 fps. One
            // frame per tick at most, regardless of how many PTY
            // events arrived since the last tick. Full-frame fallback
            // includes the dialog overlay when one is open, so the
            // open-dialog case still composes (and the operator sees
            // dialog content over the latest pane state) instead of
            // accumulating dirty until dismiss — without this the
            // dismiss frame was a sudden jump of N frames' worth of
            // accumulated PTY output that the operator had no way to
            // see coming.
            _ = render_ticker.tick(), if mux.has_pending_render() => {
                let frame_data = mux.compose_pending_frame();
                if !frame_data.is_empty() {
                    mux.send_output(frame_data);
                }
            }

            // Branch changes are directly operator-triggered (`git checkout`)
            // and should surface in chrome immediately. Keep this separate
            // from the heavier 1s state ticker so session state refreshes and
            // GitHub lookups do not need the same fast cadence.
            _ = branch_context_ticker.tick() => {
                mux.maybe_spawn_git_branch_context_lookup(Instant::now());
            }

            // Periodic state refresh: re-render the status bar so the tab
            // strip's state glyph follows the four-state model. The full
            // pane bodies stay where they are.
            //
            // The buffer is wrapped in `DECSC` (`\x1b7`) / `DECRC`
            // (`\x1b8`) so the terminal saves the active pane's
            // cursor position before painting the status bar and
            // restores it afterwards. Without this guard the cursor
            // visibly jumps to the tab strip every tick and parks
            // there as a phantom block until the next pane redraw.
            _ = state_ticker.tick() => {
                mux.maybe_spawn_pull_request_context_lookup(Instant::now());
                for session in mux.sessions.values_mut() {
                    session.refresh_state();
                }
                if mux.expire_dialog_copy_feedback(Instant::now()) {
                    let frame_data =
                        mux.compose_dialog_overlay_frame(FullRedrawReason::DialogChange);
                    mux.send_output(frame_data);
                    continue;
                }
                mux.refresh_tab_labels();
                let mut sbuf = b"\x1b7".to_vec();
                let states: Vec<(u64, AgentState)> = mux.sessions.iter()
                    .map(|(&id, s)| (id, s.state))
                    .collect();
                mux.status_bar.render(
                    &mut sbuf,
                    mux.term_cols,
                    &mux.tabs,
                    mux.active_tab,
                    &states,
                    hovered_tab(mux.hover_target),
                    hovered_menu(mux.hover_target),
                    mux.dialog_open(),
                );
                render_branch_context_bar(
                    &mut sbuf,
                    mux.term_rows,
                    mux.term_cols,
                    mux.context_bar_branch(),
                    mux.pull_request_context.as_ref(),
                    mux.pull_request_context_loading(),
                    mux.status_bar.instance_id_label(),
                    mux.hover_target,
                    mux.dialog_open(),
                );
                sbuf.extend_from_slice(b"\x1b8");
                mux.send_output(sbuf);
            }
        }
    }
}

async fn handle_client_frame(mux: &mut Multiplexer, frame: ClientFrame) {
    match frame {
        ClientFrame::Hello { .. } => {
            // The initial Hello is consumed by the accept handler; any
            // further Hello on the same connection is ignored.
        }
        ClientFrame::Resize { rows, cols } => {
            mux.resize(rows, cols);
            let frame_data = mux.compose_full_frame(FullRedrawReason::Resize);
            mux.send_output(frame_data);
        }
        ClientFrame::Input(bytes) => {
            // Debug-only input-path telemetry: every chunk from the
            // client and every parser event lands in the log when
            // `JACKIN_DEBUG=1`. Production runs stay quiet — the macro
            // skips the format + write entirely. The pair is the
            // canonical trace for "key X did nothing" triage: chunk
            // line proves the byte reached the daemon, event line
            // proves the parser classified it.
            crate::cdebug!(
                "rx ClientFrame::Input len={} bytes={:02x?}",
                bytes.len(),
                bytes
            );
            let events = mux.input_parser.parse(&bytes);
            for event in events {
                crate::cdebug!(
                    "  → InputEvent::{:?} dialog_open={}",
                    event,
                    mux.dialog_open()
                );
                if let Some(redraw) = mux.handle_input(event) {
                    mux.send_output(redraw);
                }
            }
            let mode = if mux.input_parser.is_awaiting_prefix() {
                crate::statusbar::PrefixMode::Awaiting
            } else {
                crate::statusbar::PrefixMode::Idle
            };
            if mux.status_bar.prefix_mode != mode {
                mux.status_bar.set_prefix_mode(mode);
                let frame_data = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
                mux.send_output(frame_data);
            }
        }
        ClientFrame::Command(_payload) => {
            // Reserved for future structured commands from the host CLI.
        }
        ClientFrame::Detach => {
            mux.detach_requested = true;
        }
        ClientFrame::FocusIn => {
            // Forward only when no dialog is intercepting input AND
            // the focused session actually asked for focus reports
            // (`?1004h`). Without the gate, normal-screen shells
            // surface `[I` as literal text at the prompt.
            if !mux.dialog_open()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.sessions.get(&focused)
                && s.focus_events_enabled()
            {
                s.send_input(b"\x1b[I");
            }
        }
        ClientFrame::FocusOut => {
            if !mux.dialog_open()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.sessions.get(&focused)
                && s.focus_events_enabled()
            {
                s.send_input(b"\x1b[O");
            }
        }
    }
}

/// A validated attach handshake produced by `perform_handshake`. The
/// main loop applies these — `client_permit` is kept alive until the
/// spawned persistent attach task drops it.
struct AttachHandshake {
    stream: UnixStream,
    rows: u16,
    cols: u16,
    spawn: Option<SpawnRequest>,
    env: Vec<(String, String)>,
    terminal: ClientTerminal,
    /// `Some(session_id)` when the client (typically the host
    /// console picking out of the snapshot preview) wants the daemon
    /// to focus a specific pane before forwarding content. The main
    /// loop calls `Multiplexer::focus_session_globally` on receipt.
    /// Unknown ids are silently ignored — see the daemon arm.
    focus_session: Option<u64>,
    client_permit: tokio::sync::OwnedSemaphorePermit,
}

/// Per-connection handshake task. Reads the first byte, routes
/// control-channel requests inline (one-shot reply, closes the
/// socket), and forwards validated attach Hellos back to the main
/// loop via `handshake_tx`. Owning the slow `read_exact` here keeps a
/// silent or slow client from stalling the daemon's main `select!`.
async fn perform_handshake(
    mut stream: UnixStream,
    client_permit: tokio::sync::OwnedSemaphorePermit,
    handshake_tx: mpsc::UnboundedSender<AttachHandshake>,
    sessions_snapshot: Vec<crate::protocol::control::SessionInfo>,
    tabs_snapshot: Vec<crate::protocol::control::TabSnapshot>,
    active_tab: u32,
) {
    // Bound the handshake reads. A client that opens the socket and
    // never sends a byte otherwise holds the `OwnedSemaphorePermit`
    // forever — sixteen silent peers would starve the
    // `MAX_CONCURRENT_CLIENTS` cap and lock out legitimate attaches.
    const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

    let mut first = [0u8; 1];
    match tokio::time::timeout(HANDSHAKE_TIMEOUT, stream.read_exact(&mut first)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            crate::clog!("attach: handshake read_exact(first byte) failed: {e}");
            drop(client_permit);
            return;
        }
        Err(_) => {
            crate::clog!(
                "attach: handshake first byte not received within {HANDSHAKE_TIMEOUT:?}; dropping connection"
            );
            drop(client_permit);
            return;
        }
    }
    if first[0] == 0x00 {
        // Control channel — one-shot length-prefixed JSON. The
        // sessions snapshot is captured at accept time in the main
        // loop; mildly stale (microseconds) for the host CLI's
        // informational `status` query.
        socket::handle_control_request(
            stream,
            first[0],
            sessions_snapshot,
            tabs_snapshot,
            active_tab,
        )
        .await;
        drop(client_permit);
        return;
    }
    let initial_frame = match tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        read_client_frame(&mut stream, first[0]),
    )
    .await
    {
        Ok(Ok(Some(frame))) => frame,
        Ok(Ok(None)) => {
            crate::clog!("attach: handshake EOF before initial frame");
            drop(client_permit);
            return;
        }
        Ok(Err(e)) => {
            crate::clog!("attach: handshake frame decode failed: {e}");
            drop(client_permit);
            return;
        }
        Err(_) => {
            crate::clog!(
                "attach: handshake Hello frame not received within {HANDSHAKE_TIMEOUT:?}; dropping connection"
            );
            drop(client_permit);
            return;
        }
    };
    let ClientFrame::Hello {
        rows,
        cols,
        spawn,
        env,
        terminal,
        focus_session,
    } = initial_frame
    else {
        crate::clog!("attach: rejected client whose first frame was not Hello: {initial_frame:?}");
        drop(client_permit);
        return;
    };
    let handshake = AttachHandshake {
        stream,
        rows,
        cols,
        spawn,
        env,
        terminal,
        focus_session,
        client_permit,
    };
    if handshake_tx.send(handshake).is_err() {
        crate::clog!("attach: handshake channel closed; daemon shutting down");
    }
}

async fn drain_and_exit(mux: &mut Multiplexer) {
    detach_client(mux).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
}

const ATTACH_SHUTDOWN_FLUSH_GRACE_MS: u64 = 50;

fn send_attached_shutdown(mux: &mut Multiplexer, context: &str) -> bool {
    let Some(tx) = mux.attached_out.take() else {
        return false;
    };
    if tx.send(encode_server(ServerFrame::Shutdown)).is_err() {
        crate::clog!("{context}: client receiver already dropped; Shutdown frame not delivered");
    }
    true
}

/// Centralised detach for the currently-attached client. Take-then-
/// send-then-wait-then-abort, in that order, so a takeover/cancel race never
/// leaves `attached_task = Some` with a dead `attached_out`: take the
/// out-channel sender first (so the next frame queue allocation does
/// not race with the old receiver), send Shutdown best-effort, give
/// the attach task a brief writer-side drain window, then
/// abort the attach task so its reader stops pushing into the shared
/// `cmd_tx`. Used by SIGTERM / SIGINT shutdown, explicit detach, and
/// `drain_and_exit`.
async fn detach_attached_task(mux: &mut Multiplexer, context: &str) {
    let had_sender = send_attached_shutdown(mux, context);
    // The latch is paired with the sender's lifetime: clearing
    // `attached_out` invalidates the previous attach, so the next
    // assignment (in the takeover branch of `run_daemon`) starts from
    // a clean state regardless of which code path reassigns it.
    mux.attached_out_dead_logged = false;
    if had_sender {
        tokio::time::sleep(Duration::from_millis(ATTACH_SHUTDOWN_FLUSH_GRACE_MS)).await;
    }
    if let Some(handle) = mux.attached_task.take() {
        handle.abort();
    }
}

fn initial_spawn_request(initial_agent: &str) -> SpawnRequest {
    if initial_agent.is_empty() {
        SpawnRequest::Shell
    } else {
        SpawnRequest::Agent(initial_agent.to_string())
    }
}

fn spawn_request_label(request: &SpawnRequest) -> String {
    match request {
        SpawnRequest::Agent(agent) => format!("agent {agent:?}"),
        SpawnRequest::Shell => "shell".to_string(),
    }
}

async fn detach_client(mux: &mut Multiplexer) {
    detach_attached_task(mux, "detach_client").await;
}

/// Per-client connection handler: bidirectional bridge between the
/// socket and the main daemon loop. Reads `ClientFrame`s off the
/// socket and pushes them through `cmd_tx`; writes any bytes
/// received on `out_rx` back to the socket. Exits on any I/O error
/// or when either channel closes (which happens during takeover —
/// `attached_task.abort()` ends this task before its socket sees EOF).
async fn handle_attach_client(
    mut stream: UnixStream,
    mut out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    cmd_tx: mpsc::UnboundedSender<ClientFrame>,
) {
    let mut tag = [0u8; 1];
    loop {
        tokio::select! {
            result = stream.read_exact(&mut tag) => {
                if let Err(e) = result {
                    crate::clog!("attach client: socket read failed: {e}");
                    break;
                }
                let frame = match read_client_frame(&mut stream, tag[0]).await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        crate::clog!("attach client: EOF mid-frame (tag={:#04x})", tag[0]);
                        break;
                    }
                    Err(e) => {
                        crate::clog!(
                            "attach client: frame decode failed (tag={:#04x}): {e}",
                            tag[0]
                        );
                        break;
                    }
                };
                if cmd_tx.send(frame).is_err() {
                    crate::clog!("attach client: cmd_tx closed; daemon shutting down");
                    return;
                }
            }
            Some(bytes) = out_rx.recv() => {
                if let Err(e) = stream.write_all(&bytes).await {
                    crate::clog!("attach client: socket write failed: {e}");
                    break;
                }
            }
        }
    }
    // Signal the main loop that this client is gone so it can clear
    // `attached_out` / `attached_task` — without this, subsequent
    // `send_to_client` calls silently drop into the closed channel
    // and the daemon keeps treating the dead socket as live. If the
    // main loop is already shutting down the send fails; log so the
    // exact symptom this comment warns against does not happen
    // silently if the cmd_tx side is the one that died first.
    if cmd_tx.send(ClientFrame::Detach).is_err() {
        crate::clog!(
            "attach client: cmd_tx closed before synthetic Detach could fire; main loop is already tearing down"
        );
    }
}

/// Extract the text inside `sel` from the source pane's `vt100`
/// screen. Walks every cell between anchor and end in row-major
/// order (with newlines between rows) and concatenates the cell
/// contents. Whitespace at the trailing edge of each row is trimmed
/// so the operator's clipboard doesn't fill with padding spaces.
fn selection_text(screen: &vt100::Screen, sel: &SelectionState) -> String {
    let (start_row, start_col, end_row, end_col) = canonical_selection(sel);
    let (screen_rows, _) = screen.size();
    // Must match `paint_selection_highlight`'s bound — without this
    // the painted highlight and the copied text disagree mid-resize.
    let cols_for_full_row = sel.inner.cols.saturating_sub(1);
    let max_row = screen_rows.saturating_sub(1).min(end_row);
    if start_row > max_row {
        return String::new();
    }
    let mut out = String::new();
    for r in start_row..=max_row {
        let from_col = if r == start_row { start_col } else { 0 };
        let to_col = if r == end_row {
            end_col
        } else {
            cols_for_full_row
        };
        let mut row_text = String::new();
        let mut c = from_col;
        while c <= to_col {
            if let Some(cell) = screen.cell(r, c)
                && cell.has_contents()
            {
                row_text.push_str(cell.contents());
                c += if cell.is_wide() { 2 } else { 1 };
            } else {
                row_text.push(' ');
                c += 1;
            }
        }
        out.push_str(row_text.trim_end());
        if r != max_row {
            out.push('\n');
        }
    }
    out
}

/// Normalise a selection into `(start_row, start_col, end_row, end_col)`
/// in top-left → bottom-right order, regardless of which direction the
/// operator dragged.
fn canonical_selection(sel: &SelectionState) -> (u16, u16, u16, u16) {
    if (sel.anchor_row, sel.anchor_col) <= (sel.end_row, sel.end_col) {
        (sel.anchor_row, sel.anchor_col, sel.end_row, sel.end_col)
    } else {
        (sel.end_row, sel.end_col, sel.anchor_row, sel.anchor_col)
    }
}

/// Paint an inverse-video highlight over every cell inside the
/// selection rectangle. Emitted after pane-body rendering so the
/// agent's content is preserved underneath — the operator sees the same
/// glyphs but on a reversed colour pair, which is the universal
/// "this is selected" cue.
fn paint_selection_highlight(buf: &mut Vec<u8>, screen: &vt100::Screen, sel: &SelectionState) {
    let (start_row, start_col, end_row, end_col) = canonical_selection(sel);
    let inner = sel.inner;
    for r in start_row..=end_row {
        let from_col = if r == start_row { start_col } else { 0 };
        let to_col = if r == end_row {
            end_col
        } else {
            inner.cols.saturating_sub(1)
        };
        if to_col < from_col {
            continue;
        }
        let abs_row = inner.row + r;
        let abs_col = inner.col + from_col;
        let _ =
            std::io::Write::write_fmt(buf, format_args!("\x1b[{};{}H", abs_row + 1, abs_col + 1));
        // Inverse SGR — preserves whatever fg/bg the underlying cell
        // carried so the operator still reads the selected text.
        buf.extend_from_slice(b"\x1b[7m");
        let mut c = from_col;
        while c <= to_col {
            if let Some(cell) = screen.cell(r, c)
                && cell.has_contents()
            {
                buf.extend_from_slice(cell.contents().as_bytes());
                c += if cell.is_wide() { 2 } else { 1 };
            } else {
                buf.push(b' ');
                c += 1;
            }
        }
        buf.extend_from_slice(b"\x1b[0m");
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

fn display_title(session: &Session) -> String {
    if let Some(title) = session.title() {
        title.to_string()
    } else if let Some(cwd) = session.cwd() {
        jackin_tui::shorten_home(cwd)
    } else {
        session.label.clone()
    }
}

const OUTER_TERMINAL_TITLE_MAX_CHARS: usize = 180;

fn compose_outer_terminal_title(
    workdir: &Path,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
) -> String {
    let workspace = workspace_title(workdir);
    let context = pull_request
        .map(|pr| format!("PR #{} · {}", pr.number, pr.title))
        .or_else(|| branch.map(ToOwned::to_owned))
        .filter(|value| !value.trim().is_empty());

    let raw_title = match context {
        Some(context) => format!("{workspace} · {context}"),
        None => workspace,
    };
    trim_title_chars(
        &sanitize_terminal_title(&raw_title),
        OUTER_TERMINAL_TITLE_MAX_CHARS,
    )
}

fn workspace_title(workdir: &Path) -> String {
    workdir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| workdir.display().to_string())
}

fn sanitize_terminal_title(title: &str) -> String {
    title
        .chars()
        .map(|ch| {
            if ch.is_control() || ch == '\u{7f}' {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn trim_title_chars(title: &str, max_chars: usize) -> String {
    if title.chars().count() <= max_chars {
        return title.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut trimmed = title.chars().take(keep).collect::<String>();
    trimmed.push('…');
    trimmed
}

fn append_osc_window_title(buf: &mut Vec<u8>, title: &str) {
    buf.extend_from_slice(b"\x1b]2;");
    buf.extend_from_slice(title.as_bytes());
    buf.extend_from_slice(b"\x1b\\");
}

const BRANCH_CONTEXT_BAR_BG: &str = "\x1b[48;2;255;255;255m";
const BRANCH_CONTEXT_BAR_HOVER_BG: &str = "\x1b[48;2;225;245;255m";
const BRANCH_CONTEXT_BAR_FG: &str = "\x1b[38;2;0;0;0m";
const BRANCH_CONTEXT_BAR_LINK_FG: &str = "\x1b[38;2;0;80;180m";
const BRANCH_CONTEXT_BAR_HOVER_FG: &str = "\x1b[38;2;0;55;140m";
const BRANCH_CONTEXT_BAR_BOLD: &str = "\x1b[1m";
const BRANCH_CONTEXT_BAR_BG_DIM: &str = "\x1b[48;2;51;51;51m";
const BRANCH_CONTEXT_BAR_FG_DIM: &str = "\x1b[38;2;0;28;6m";
const BRANCH_CONTEXT_BAR_LINK_FG_DIM: &str = "\x1b[38;2;0;16;36m";
const RESET: &str = "\x1b[0m";

fn render_branch_context_bar(
    buf: &mut Vec<u8>,
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    container_name: &str,
    hover_target: Option<HoverTarget>,
    dim: bool,
) {
    let Some(layout) = branch_context_bar_layout(
        term_rows,
        term_cols,
        branch,
        pull_request,
        pull_request_loading,
        container_name,
    ) else {
        return;
    };

    buf.extend_from_slice(format!("\x1b[{};1H", term_rows).as_bytes());
    buf.extend_from_slice(
        if dim {
            BRANCH_CONTEXT_BAR_BG_DIM
        } else {
            BRANCH_CONTEXT_BAR_BG
        }
        .as_bytes(),
    );
    buf.extend_from_slice(
        if dim {
            BRANCH_CONTEXT_BAR_FG_DIM
        } else {
            BRANCH_CONTEXT_BAR_FG
        }
        .as_bytes(),
    );
    for _ in 0..term_cols {
        buf.push(b' ');
    }

    buf.extend_from_slice(format!("\x1b[{};1H", term_rows).as_bytes());
    let left_hovered = !dim && hover_target == Some(HoverTarget::BranchContext);
    let left_bg = if dim {
        BRANCH_CONTEXT_BAR_BG_DIM
    } else if left_hovered {
        BRANCH_CONTEXT_BAR_HOVER_BG
    } else {
        BRANCH_CONTEXT_BAR_BG
    };
    let left_fg = if dim {
        BRANCH_CONTEXT_BAR_FG_DIM
    } else if left_hovered {
        BRANCH_CONTEXT_BAR_HOVER_FG
    } else {
        BRANCH_CONTEXT_BAR_FG
    };
    buf.extend_from_slice(left_bg.as_bytes());
    buf.extend_from_slice(left_fg.as_bytes());
    if !dim {
        buf.extend_from_slice(BRANCH_CONTEXT_BAR_BOLD.as_bytes());
    }
    buf.extend_from_slice(layout.left.as_bytes());

    if let Some(container_start) = layout.container_start {
        buf.extend_from_slice(format!("\x1b[{};{}H", term_rows, container_start).as_bytes());
        let container_hovered = !dim && hover_target == Some(HoverTarget::Container);
        let bg = if dim {
            BRANCH_CONTEXT_BAR_BG_DIM
        } else if container_hovered {
            BRANCH_CONTEXT_BAR_HOVER_BG
        } else {
            BRANCH_CONTEXT_BAR_BG
        };
        let fg = if dim {
            BRANCH_CONTEXT_BAR_LINK_FG_DIM
        } else if container_hovered {
            BRANCH_CONTEXT_BAR_HOVER_FG
        } else {
            BRANCH_CONTEXT_BAR_LINK_FG
        };
        buf.extend_from_slice(bg.as_bytes());
        buf.extend_from_slice(fg.as_bytes());
        if container_hovered {
            buf.extend_from_slice(BRANCH_CONTEXT_BAR_BOLD.as_bytes());
        }
        buf.extend_from_slice(layout.container.as_bytes());
    }
    buf.extend_from_slice(RESET.as_bytes());
}

struct BranchContextBarLayout {
    left: String,
    left_region: Option<(u16, u16)>,
    container: String,
    container_region: Option<(u16, u16)>,
    container_start: Option<u16>,
}

fn branch_context_bar_layout(
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    container_name: &str,
) -> Option<BranchContextBarLayout> {
    if term_rows == 0 || term_cols == 0 {
        return None;
    }
    let branch_detail_visible = branch.is_some_and(|branch| !is_default_branch_name(branch));
    let (left, left_clickable) = match pull_request {
        Some(pr) => (format!(" PR {} · {} ", pr.number_label(), pr.title), true),
        None if pull_request_loading && branch_detail_visible => (
            format!(" Resolving PR · {} ", branch.unwrap_or_default()),
            true,
        ),
        None if branch_detail_visible => {
            (format!(" Branch · {} ", branch.unwrap_or_default()), true)
        }
        None => (String::new(), false),
    };
    let container = if container_name.is_empty() {
        String::new()
    } else {
        format!(" {} ", container_name)
    };
    let term_cols_usize = usize::from(term_cols);
    let container_cols = display_cols(&container);
    let container_fits = container_cols > 0 && container_cols + 2 < term_cols_usize;
    let left_max_cols = if container_fits {
        term_cols_usize.saturating_sub(container_cols + 1)
    } else {
        term_cols_usize
    };
    let left = take_display_cols(&left, left_max_cols);
    let left_cols = display_cols(&left);
    let left_region = if left_clickable && left_cols > 0 {
        let end = u16::try_from(left_cols.saturating_add(1)).unwrap_or(u16::MAX);
        Some((1, end))
    } else {
        None
    };
    let (container_start, container_region) = if container_fits {
        let start = term_cols_usize
            .saturating_sub(container_cols)
            .saturating_add(1);
        let end = start.saturating_add(container_cols);
        (
            u16::try_from(start).ok(),
            Some((
                u16::try_from(start).unwrap_or(u16::MAX),
                u16::try_from(end).unwrap_or(u16::MAX),
            )),
        )
    } else {
        (None, None)
    };
    Some(BranchContextBarLayout {
        left,
        left_region,
        container,
        container_region,
        container_start,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchContextBarHit {
    Context,
    Container,
}

const fn hovered_tab(target: Option<HoverTarget>) -> Option<usize> {
    match target {
        Some(HoverTarget::Tab(idx)) => Some(idx),
        _ => None,
    }
}

const fn hovered_menu(target: Option<HoverTarget>) -> bool {
    matches!(target, Some(HoverTarget::Menu))
}

fn branch_context_bar_hit(
    row: u16,
    col: u16,
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    container_name: &str,
) -> Option<BranchContextBarHit> {
    if row != term_rows {
        return None;
    }
    let layout = branch_context_bar_layout(
        term_rows,
        term_cols,
        branch,
        pull_request,
        pull_request_loading,
        container_name,
    )?;
    if let Some((start, end)) = layout.container_region
        && col >= start
        && col < end
    {
        return Some(BranchContextBarHit::Container);
    }
    if let Some((start, end)) = layout.left_region
        && col >= start
        && col < end
    {
        return Some(BranchContextBarHit::Context);
    }
    None
}

fn is_default_branch_name(branch: &str) -> bool {
    matches!(branch, "main" | "master" | "")
}

/// Width of `s` measured in terminal cells, ignoring ASCII control
/// bytes / OSC / CSI bytes. Control bytes report width 0 from
/// `unicode-width`, which would let a PR title like `\x1b[2J` slip
/// through the bar-truncation budget — and into the rendered output —
/// without consuming columns. Stripping them here makes the chrome
/// bar safe for arbitrary upstream strings (PR titles, branch names)
/// without each caller re-implementing the same guard.
fn display_cols(s: &str) -> usize {
    s.chars()
        .filter(|c| !is_control_char(*c))
        .map(|c| c.width().unwrap_or(0))
        .sum()
}

fn take_display_cols(s: &str, max_cols: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for c in s.chars() {
        if is_control_char(c) {
            continue;
        }
        let width = c.width().unwrap_or(0);
        if used + width > max_cols {
            break;
        }
        out.push(c);
        used += width;
    }
    out
}

/// True for any C0 / C1 control byte or DEL (`0x7f`). These bytes are
/// what terminal injection attacks (`\x1b[…m`, `\x9b…`, OSC, BEL)
/// build their sequences from; stripping them eliminates the class.
fn is_control_char(c: char) -> bool {
    (c as u32) < 0x20 || c == '\x7f' || ((c as u32) >= 0x80 && (c as u32) < 0xa0)
}

const GIT_CONTEXT_COMMAND_TIMEOUT: Duration = Duration::from_millis(1500);
const GH_PULL_REQUEST_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);

fn git_current_branch(workdir: &Path) -> Option<String> {
    // Try the cheap path first: read `.git/HEAD` and parse the symref.
    // For a normal checkout on a branch the file is one line of
    // `ref: refs/heads/<name>\n` (no subprocess fork, ~50µs vs ~3-15ms
    // for `git branch --show-current`). Detached HEAD writes the raw
    // SHA which we treat as "no branch" — the bar slot stays hidden,
    // matching `git branch --show-current` which prints empty.
    //
    // Falls back to the subprocess path for worktrees (where `.git`
    // is a file, not a directory) and for any other unusual layout
    // the file-read approach cannot handle.
    if let Some(branch) = read_branch_from_git_head(workdir) {
        return Some(branch);
    }
    command_stdout_trimmed_with_timeout(
        Command::new("git")
            .arg("-C")
            .arg(workdir)
            .args(["branch", "--show-current"]),
        GIT_CONTEXT_COMMAND_TIMEOUT,
    )
}

fn read_branch_from_git_head(workdir: &Path) -> Option<String> {
    let git_path = workdir.join(".git");
    let head_path = if git_path.is_dir() {
        git_path.join("HEAD")
    } else {
        let git_file = std::fs::read_to_string(&git_path).ok()?;
        let git_dir = git_file.trim().strip_prefix("gitdir:")?.trim();
        let git_dir = PathBuf::from(git_dir);
        let git_dir = if git_dir.is_absolute() {
            git_dir
        } else {
            workdir.join(git_dir)
        };
        git_dir.join("HEAD")
    };
    let head = std::fs::read_to_string(head_path).ok()?;
    let trimmed = head.trim();
    trimmed
        .strip_prefix("ref: refs/heads/")
        .map(|s| s.to_string())
}

/// Distinguishes "lookup succeeded but command was unavailable / failed
/// in a way that means we should not cache" from "lookup succeeded with
/// data (or with no data)". The string carries an operator-readable
/// reason for the daemon log; callers should not parse it.
#[derive(Debug)]
enum LookupError {
    Failed(String),
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LookupError::Failed(reason) => f.write_str(reason),
        }
    }
}

fn build_gh_command(workdir: &Path) -> Command {
    let mut cmd = Command::new("gh");
    cmd.current_dir(workdir)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1");
    cmd
}

fn gh_pull_request_info(
    workdir: &Path,
    branch: &str,
) -> Result<Option<PullRequestInfo>, LookupError> {
    #[derive(Deserialize)]
    struct GhPullRequest {
        number: u64,
        title: String,
        url: String,
        #[serde(rename = "isDraft")]
        is_draft: bool,
    }

    let mut cmd = build_gh_command(workdir);
    cmd.args([
        "pr",
        "list",
        "--head",
        branch,
        "--state",
        "open",
        "--limit",
        "1",
        "--json",
        "number,title,url,isDraft",
    ]);
    let json = run_command_capturing_output(&mut cmd, GH_PULL_REQUEST_COMMAND_TIMEOUT, &[0])?;
    // `gh pr list` with no matching PR prints an empty JSON array `[]`,
    // which `run_command_capturing_output` returns as `Some("[]")`. An
    // empty stdout indicates an authoritative "no rows" only when
    // `gh` chose to print nothing — accept either shape as "no PR".
    let Some(json) = json else {
        return Ok(None);
    };
    let prs: Vec<GhPullRequest> = serde_json::from_str(&json).map_err(|e| {
        LookupError::Failed(format!(
            "gh pr list JSON parse failed: {e}; payload prefix: {:.200?}",
            json
        ))
    })?;
    let Some(pr) = prs.into_iter().next() else {
        return Ok(None);
    };
    if url::Url::parse(&pr.url)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .is_none()
    {
        return Err(LookupError::Failed(format!(
            "gh pr list returned non-http(s) url: {:?}",
            pr.url
        )));
    }
    // Checks lookup is best-effort — a parse failure on checks should
    // not poison the PR cache. Demote any error to `None` checks.
    let checks = gh_pull_request_checks(workdir, &pr.url)
        .map_err(|e| crate::clog!("pull-request-context: gh pr checks failed: {e}"))
        .ok()
        .flatten();
    Ok(Some(PullRequestInfo {
        number: pr.number,
        title: pr.title,
        url: pr.url,
        is_draft: pr.is_draft,
        checks,
    }))
}

fn gh_pull_request_checks(
    workdir: &Path,
    url: &str,
) -> Result<Option<PullRequestChecks>, LookupError> {
    #[derive(Deserialize)]
    struct GhCheck {
        bucket: String,
    }

    let mut cmd = build_gh_command(workdir);
    cmd.args(["pr", "checks", url, "--json", "bucket"]);
    // `gh pr checks` exits with `8` when checks are pending and `0`
    // otherwise; both are accepted statuses.
    let json = run_command_capturing_output(&mut cmd, GH_PULL_REQUEST_COMMAND_TIMEOUT, &[0, 8])?;
    let Some(json) = json else {
        return Ok(None);
    };
    let checks: Vec<GhCheck> = serde_json::from_str(&json).map_err(|e| {
        LookupError::Failed(format!(
            "gh pr checks JSON parse failed: {e}; payload prefix: {:.200?}",
            json
        ))
    })?;
    let mut summary = PullRequestChecks {
        passing: 0,
        failing: 0,
        pending: 0,
        skipped: 0,
        cancelled: 0,
        total: checks.len(),
    };
    for check in checks {
        match check.bucket.as_str() {
            "pass" => summary.passing += 1,
            "fail" => summary.failing += 1,
            "pending" => summary.pending += 1,
            "skipping" => summary.skipped += 1,
            "cancel" => summary.cancelled += 1,
            other => {
                crate::cdebug!(
                    "pull-request-context: unknown gh pr checks bucket {:?}",
                    other
                );
            }
        }
    }
    Ok(Some(summary))
}

#[cfg(test)]
fn command_stdout_trimmed(command: &mut Command) -> Option<String> {
    command_stdout_trimmed_with_timeout(command, GIT_CONTEXT_COMMAND_TIMEOUT)
}

fn command_stdout_trimmed_with_timeout(command: &mut Command, timeout: Duration) -> Option<String> {
    command_stdout_trimmed_with_timeout_and_statuses(command, timeout, &[0])
}

/// Result-returning command runner that distinguishes success (returns
/// `Ok(Some(stdout))` or `Ok(None)` for empty stdout) from genuine
/// failure (returns `Err(LookupError::Failed)`). Used by the gh
/// helpers so cache-poisoning can be avoided.
///
/// Differences from `command_stdout_trimmed_with_timeout_and_statuses`:
/// - stdin is set to `Stdio::null()` so a misbehaving subprocess never
///   blocks reading from the daemon's stdin awaiting a prompt.
/// - stderr is captured into a bounded buffer and surfaced in the error
///   reason — the operator can see "gh: not logged in" / "HTTP 401"
///   when triaging via `multiplexer.log`.
fn run_command_capturing_output(
    command: &mut Command,
    timeout: Duration,
    accepted_statuses: &[i32],
) -> Result<Option<String>, LookupError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = format!("{:?}", command.get_program());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Err(LookupError::Failed(format!(
                "{program}: spawn failed: {e} (errno={:?})",
                e.raw_os_error()
            )));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LookupError::Failed(format!("{program}: stdout pipe missing")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| LookupError::Failed(format!("{program}: stderr pipe missing")))?;
    let stdout_reader = read_pipe_bounded(stdout, 64 * 1024);
    let stderr_reader = read_pipe_bounded(stderr, 4 * 1024);
    let started = Instant::now();
    let status_success: Option<bool>;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                status_success = Some(
                    status
                        .code()
                        .is_some_and(|code| accepted_statuses.contains(&code)),
                );
                break;
            }
            Ok(None) => {}
            Err(e) if e.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
                // PID 1 / sibling waitpid reaped the child; trust the
                // stdout pipe. Stderr is still read for the caller's
                // diagnostic context.
                status_success = None;
                break;
            }
            Err(e) => {
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(LookupError::Failed(format!(
                    "{program}: try_wait failed: {e} (errno={:?})",
                    e.raw_os_error()
                )));
            }
        }
        if started.elapsed() >= timeout {
            if let Err(e) = child.kill() {
                crate::clog!(
                    "{program}: timeout ({timeout:?}) and child.kill() failed: {e} (errno={:?})",
                    e.raw_os_error()
                );
            }
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(LookupError::Failed(format!(
                "{program}: timed out after {timeout:?}"
            )));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let stdout_bytes = stdout_reader
        .join()
        .map_err(|_| LookupError::Failed(format!("{program}: stdout reader panicked")))?
        .map_err(|e| LookupError::Failed(format!("{program}: stdout read failed: {e}")))?;
    let stderr_bytes = stderr_reader
        .join()
        .unwrap_or(Ok(Vec::new()))
        .unwrap_or_default();
    if status_success == Some(false) {
        let stderr_text = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        return Err(LookupError::Failed(format!(
            "{program}: non-accepted status; stderr: {stderr_text}"
        )));
    }
    let value = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn read_pipe_bounded<R: std::io::Read + Send + 'static>(
    mut pipe: R,
    cap: usize,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(cap.min(16 * 1024));
        let mut buf = [0u8; 4096];
        loop {
            let n = pipe.read(&mut buf)?;
            if n == 0 {
                break;
            }
            let take = (cap - bytes.len()).min(n);
            bytes.extend_from_slice(&buf[..take]);
            if bytes.len() >= cap {
                // Cap reached; drain remaining bytes so the writer
                // doesn't block on SIGPIPE waiting for us.
                while pipe.read(&mut buf)? > 0 {}
                break;
            }
        }
        Ok(bytes)
    })
}

fn command_stdout_trimmed_with_timeout_and_statuses(
    command: &mut Command,
    timeout: Duration,
    accepted_statuses: &[i32],
) -> Option<String> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            crate::clog!(
                "command spawn failed ({:?}): {e} (errno={:?})",
                command.get_program(),
                e.raw_os_error()
            );
            return None;
        }
    };
    let mut stdout = child.stdout.take()?;
    let stdout_reader = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    let started = Instant::now();
    let status_success: Option<bool>;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let accepted = status
                    .code()
                    .is_some_and(|code| accepted_statuses.contains(&code));
                status_success = Some(accepted);
                break;
            }
            Ok(None) => {}
            // The kernel reaped the child out from under us — either
            // PID 1's zombie reaper inside the daemon or a sibling
            // thread's waitpid. The exit status is lost; trust the
            // stdout pipe (callers like the Container info dialog
            // would otherwise show empty fields for healthy git/gh
            // commands). Non-ECHILD errnos are surfaced.
            Err(e) if e.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
                status_success = None;
                break;
            }
            Err(e) => {
                crate::clog!(
                    "command try_wait failed ({:?}): {e} (errno={:?})",
                    command.get_program(),
                    e.raw_os_error()
                );
                let _ = stdout_reader.join();
                return None;
            }
        }
        if started.elapsed() >= timeout {
            if let Err(e) = child.kill() {
                crate::clog!(
                    "command timeout ({timeout:?}): child.kill() failed: {e} (errno={:?}); child may linger",
                    e.raw_os_error()
                );
            }
            let _ = child.wait();
            // Joining the reader is bounded: kill() closed the pipe,
            // so read_to_end returns quickly. Without the join the
            // OS-thread is leaked across every timeout firing.
            let _ = stdout_reader.join();
            return None;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if status_success == Some(false) {
        return None;
    }
    let stdout = match stdout_reader.join() {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => {
            crate::clog!(
                "command stdout read failed: {e} (errno={:?})",
                e.raw_os_error()
            );
            return None;
        }
        Err(_) => {
            crate::clog!("command stdout reader thread panicked");
            return None;
        }
    };
    let value = String::from_utf8_lossy(&stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn prefix_full_redraw_reason(cmd: &PrefixCommand) -> FullRedrawReason {
    match cmd {
        PrefixCommand::NewTab | PrefixCommand::Palette => FullRedrawReason::PaletteOverlay,
        PrefixCommand::NextTab | PrefixCommand::PrevTab | PrefixCommand::JumpTab(_) => {
            FullRedrawReason::TabSwitch
        }
        PrefixCommand::SplitTopBottom | PrefixCommand::SplitSideBySide => {
            FullRedrawReason::LayoutChange
        }
        PrefixCommand::MoveFocus(_) => FullRedrawReason::FocusChange,
        PrefixCommand::ZoomToggle => FullRedrawReason::ZoomChange,
        PrefixCommand::KillPane | PrefixCommand::KillTab => FullRedrawReason::SplitClose,
        PrefixCommand::ClearPane => FullRedrawReason::PaneClear,
        PrefixCommand::Detach | PrefixCommand::Redraw => FullRedrawReason::ExplicitRedraw,
    }
}

#[derive(Default)]
struct PaneScrollbar {
    offset: usize,
    filled: usize,
}

impl PaneScrollbar {
    const fn visible(&self) -> bool {
        self.filled > 0
    }
}

fn pane_scrollbar(session: &mut Session, viewport_rows: u16, viewport_cols: u16) -> PaneScrollbar {
    let debug_enabled = crate::logging::debug_enabled();
    let (filled, vt_filled, inline_filled) = if debug_enabled {
        let (vt_filled, inline_filled) = session.scrollback_counts();
        (
            vt_filled.saturating_add(inline_filled),
            vt_filled,
            inline_filled,
        )
    } else {
        (session.scrollback_filled(), 0, 0)
    };
    let scrollbar = PaneScrollbar {
        offset: session.scrollback_offset,
        filled,
    };
    let metrics = if debug_enabled {
        screen_scroll_affordance_metrics(session.screen(), viewport_rows, viewport_cols)
    } else {
        None
    };
    crate::cdebug!(
        "scrollbar decision: agent={:?} alt_screen={} mouse_enabled={} viewport={}x{} screen={}x{} cursor={}x{} occupied_rows={} first_occupied_row={} last_occupied_row={} vt_scrollback={} inline_scrollback={} scrollback_filled={} visible={} reason={}",
        session.agent,
        session.screen().alternate_screen(),
        session.mouse_enabled(),
        viewport_rows,
        viewport_cols,
        metrics.as_ref().map_or(0, |m| m.screen_rows),
        metrics.as_ref().map_or(0, |m| m.screen_cols),
        metrics.as_ref().map_or(0, |m| m.cursor_row),
        metrics.as_ref().map_or(0, |m| m.cursor_col),
        metrics.as_ref().map_or(0, |m| m.occupied_rows),
        metrics
            .as_ref()
            .and_then(|m| m.first_occupied_row)
            .map_or(-1, i32::from),
        metrics
            .as_ref()
            .and_then(|m| m.last_occupied_row)
            .map_or(-1, i32::from),
        vt_filled,
        inline_filled,
        filled,
        scrollbar.visible(),
        if scrollbar.visible() {
            "retained-scrollback"
        } else {
            "none"
        }
    );
    scrollbar
}

struct ScrollAffordanceMetrics {
    screen_rows: u16,
    screen_cols: u16,
    cursor_row: u16,
    cursor_col: u16,
    occupied_rows: usize,
    first_occupied_row: Option<u16>,
    last_occupied_row: Option<u16>,
}

fn screen_scroll_affordance_metrics(
    screen: &vt100::Screen,
    viewport_rows: u16,
    viewport_cols: u16,
) -> Option<ScrollAffordanceMetrics> {
    let (screen_rows, screen_cols) = screen.size();
    let rows = viewport_rows.min(screen_rows);
    let cols = viewport_cols.min(screen_cols);
    if rows == 0 || cols == 0 {
        return None;
    }

    let mut occupied_rows = 0usize;
    let mut first_occupied_row = None;
    let mut last_occupied_row = None;
    for row in 0..rows {
        if (0..cols).any(|col| screen.cell(row, col).is_some_and(|c| c.has_contents())) {
            occupied_rows += 1;
            first_occupied_row.get_or_insert(row);
            last_occupied_row = Some(row);
        }
    }
    let (cursor_row, cursor_col) = screen.cursor_position();

    Some(ScrollAffordanceMetrics {
        screen_rows,
        screen_cols,
        cursor_row,
        cursor_col,
        occupied_rows,
        first_occupied_row,
        last_occupied_row,
    })
}

fn pane_wheel_cursor_fallback_reason(session: &Session) -> Option<&'static str> {
    if session.mouse_enabled() {
        return None;
    }
    if session.screen().alternate_screen() {
        return Some("alternate-screen");
    }
    None
}

/// SGR mouse wheel events set bit 6 of the button byte. Every value in
/// `64..=95` is a wheel event with some combination of modifier flags
/// (shift = +4, alt = +8, ctrl = +16). Panes that did not request
/// mouse mode must not receive these bytes because they dump raw SGR at
/// prompts or disappear into TUIs that never subscribed to mouse input.
fn is_wheel_button(button: u8) -> bool {
    (64..96).contains(&button)
}

fn mouse_event_allowed_for_mode(mode: vt100::MouseProtocolMode, button: u8, press: bool) -> bool {
    use vt100::MouseProtocolMode;

    if mode == MouseProtocolMode::None {
        return false;
    }
    if is_wheel_button(button) {
        return true;
    }

    let motion = button & 0b100000 != 0;
    let passive_motion = motion && button & 0b11 == 3;
    match mode {
        MouseProtocolMode::None => false,
        MouseProtocolMode::Press => press && !motion,
        MouseProtocolMode::PressRelease => !motion,
        MouseProtocolMode::ButtonMotion => !passive_motion,
        MouseProtocolMode::AnyMotion => true,
    }
}

fn mouse_event_encoding_for_session(
    session: &Session,
    button: u8,
    press: bool,
) -> Option<vt100::MouseProtocolEncoding> {
    if mouse_event_allowed_for_mode(session.mouse_protocol_mode(), button, press) {
        return Some(session.mouse_protocol_encoding());
    }
    None
}

fn encode_mouse_for_protocol(
    button: u8,
    col: u16,
    row: u16,
    press: bool,
    encoding: vt100::MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    match encoding {
        vt100::MouseProtocolEncoding::Sgr => {
            let final_byte = if press { 'M' } else { 'm' };
            Some(format!("\x1b[<{button};{col};{row}{final_byte}").into_bytes())
        }
        vt100::MouseProtocolEncoding::Default | vt100::MouseProtocolEncoding::Utf8 => {
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

fn encode_wheel_cursor_fallback(session: &Session, button: u8) -> Option<Vec<u8>> {
    if !is_wheel_button(button) || session.mouse_enabled() {
        return None;
    }
    let seq = if session.screen().application_cursor() {
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

fn push_xterm_mouse_number(
    out: &mut Vec<u8>,
    value: u32,
    encoding: vt100::MouseProtocolEncoding,
) -> Option<()> {
    match encoding {
        vt100::MouseProtocolEncoding::Default => {
            out.push(u8::try_from(value).ok()?);
        }
        vt100::MouseProtocolEncoding::Utf8 => {
            let ch = char::from_u32(value)?;
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
        vt100::MouseProtocolEncoding::Sgr => unreachable!("SGR does not use xterm fields"),
    }
    Some(())
}

/// Format a spawn-failure banner: save cursor → jump to row 1, col 1
/// → bold red text → clear to end of line → restore cursor. The
/// save/restore wrap prevents the banner from scrolling whichever
/// pane the composed frame left the cursor in.
fn spawn_failure_banner(reason: &str) -> Vec<u8> {
    format!("\x1b7\x1b[1;1H\x1b[1;31mjackin: {reason}\x1b[0m\x1b[K\x1b8").into_bytes()
}

/// OSC 52 clipboard-write sequence: `\x1b]52;c;<base64>\x07`. Targets
/// the system clipboard (`c`); the BEL terminator is the form Ghostty,
/// Kitty, iTerm2, and Alacritty all parse. Forwarded to the operator's
/// outer terminal via `send_output` from the `CopyToClipboard` dialog
/// action.
fn encode_osc52_clipboard_write(payload: &str) -> Vec<u8> {
    let encoded = BASE64.encode(payload.as_bytes());
    let mut out = Vec::with_capacity(8 + encoded.len());
    out.extend_from_slice(b"\x1b]52;c;");
    out.extend_from_slice(encoded.as_bytes());
    out.extend_from_slice(b"\x07");
    out
}

fn osc22_pointer_shape(shape: PointerShape) -> Vec<u8> {
    format!("\x1b]22;{}\x1b\\", shape.as_osc22_name()).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use portable_pty::{ChildKiller, MasterPty, PtySize};

    #[derive(Debug)]
    struct NullChildKiller;

    impl ChildKiller for NullChildKiller {
        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(Self)
        }
    }

    struct NullMasterPty;

    impl MasterPty for NullMasterPty {
        fn resize(&self, _size: PtySize) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_size(&self) -> anyhow::Result<PtySize> {
            Ok(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
        }

        fn try_clone_reader(&self) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
            Ok(Box::new(std::io::empty()))
        }

        fn take_writer(&self) -> anyhow::Result<Box<dyn std::io::Write + Send>> {
            Ok(Box::new(std::io::sink()))
        }

        #[cfg(unix)]
        fn process_group_leader(&self) -> Option<nix::libc::pid_t> {
            None
        }

        #[cfg(unix)]
        fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
            None
        }

        #[cfg(unix)]
        fn tty_name(&self) -> Option<std::path::PathBuf> {
            None
        }
    }

    #[test]
    fn spawn_failure_banner_wraps_in_save_restore_and_carries_reason() {
        let bytes = spawn_failure_banner("boom: agent slug rejected");
        assert!(bytes.starts_with(b"\x1b7\x1b[1;1H"));
        assert!(bytes.ends_with(b"\x1b8"));
        assert!(
            bytes
                .windows(b"boom: agent slug rejected".len())
                .any(|w| w == b"boom: agent slug rejected"),
            "reason missing from banner: {:?}",
            String::from_utf8_lossy(&bytes)
        );
        assert!(
            bytes.windows(2).any(|w| w == b"\x1b["),
            "missing SGR opener"
        );
    }

    fn test_mux(rows: u16, cols: u16) -> Multiplexer {
        Multiplexer::new(
            rows,
            cols,
            CapsuleConfig {
                role: "test-role".to_string(),
                workdir: "/workspace".to_string(),
                agents: Vec::new(),
                models: std::collections::BTreeMap::new(),
            },
        )
    }

    fn single_pane_tab_mux() -> Multiplexer {
        single_pane_tab_mux_with_size(24, 80)
    }

    fn single_pane_tab_mux_with_size(rows: u16, cols: u16) -> Multiplexer {
        let mut mux = test_mux(24, 80);
        mux.resize(rows, cols);
        mux.tabs.push(Tab::new_single("Shell", 1));
        mux
    }

    fn pull_request_fixture(number: u64) -> PullRequestInfo {
        PullRequestInfo {
            number,
            title: "Surface PR context in Capsule".to_string(),
            url: format!("https://github.com/jackin-project/jackin/pull/{number}"),
            is_draft: false,
            checks: None,
        }
    }

    #[test]
    fn outer_terminal_title_uses_workspace_and_pr_title() {
        let title = compose_outer_terminal_title(
            Path::new("/Users/operator/Projects/jackin"),
            Some("feat/capsule-pr-context-bar"),
            Some(&pull_request_fixture(436)),
        );

        assert_eq!(title, "jackin · PR #436 · Surface PR context in Capsule");
    }

    #[test]
    fn outer_terminal_title_falls_back_to_branch_without_pr() {
        let title = compose_outer_terminal_title(
            Path::new("/Users/operator/Projects/jackin"),
            Some("feat/capsule-pr-context-bar"),
            None,
        );

        assert_eq!(title, "jackin · feat/capsule-pr-context-bar");
    }

    #[test]
    fn outer_terminal_title_sanitizes_control_bytes() {
        let pull_request = PullRequestInfo {
            number: 436,
            title: "bad\x1b]2;owned\x07title".to_string(),
            url: "https://github.com/jackin-project/jackin/pull/436".to_string(),
            is_draft: false,
            checks: None,
        };
        let title =
            compose_outer_terminal_title(Path::new("/workspace/jackin"), None, Some(&pull_request));

        assert_eq!(title, "jackin · PR #436 · bad ]2;owned title");
    }

    #[test]
    fn full_frame_emits_outer_terminal_title_once_until_context_changes() {
        let mut mux = single_pane_tab_mux();
        mux.workdir = PathBuf::from("/workspace/jackin");
        mux.pull_request_context_branch = Some("feat/capsule-pr-context-bar".to_string());

        let first =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            first.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
            "first frame should set branch title: {first:?}"
        );

        let second =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            !second.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
            "unchanged full frame should not spam title: {second:?}"
        );

        mux.pull_request_context = Some(pull_request_fixture(436));
        let updated =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            updated.contains("\x1b]2;jackin · PR #436 · Surface PR context in Capsule\x1b\\"),
            "PR context change should refresh title: {updated:?}"
        );
    }

    fn test_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        test_session_with_agent(rows, cols, Some("codex".to_string()))
    }

    fn test_shell_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        test_session_with_agent(rows, cols, None)
    }

    fn pane_kind_cases() -> [(Option<&'static str>, &'static str); 2] {
        [(Some("codex"), "agent"), (None, "shell")]
    }

    fn test_pane_session(
        rows: u16,
        cols: u16,
        agent: Option<&str>,
    ) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        test_session_with_agent(rows, cols, agent.map(ToString::to_string))
    }

    fn assert_focused_scroll_chrome(frame: &[u8], context: &str) {
        let rendered = String::from_utf8_lossy(frame);
        assert!(
            rendered.contains("\x1b[0;38;2;0;255;65m"),
            "focused {context} should use green chrome"
        );
        assert!(
            rendered.contains('█'),
            "focused {context} should draw a scrollbar thumb"
        );
    }

    fn assert_no_scroll_thumb(frame: &[u8], context: &str) {
        assert!(
            !String::from_utf8_lossy(frame).contains('█'),
            "{context} should not draw fake scrollback chrome"
        );
    }

    fn assert_wheel_cursor_fallback_sent(
        input_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
        expected_bytes: &[u8],
    ) {
        assert_eq!(
            input_rx
                .try_recv()
                .expect("wheel fallback should reach PTY"),
            expected_bytes,
        );
        assert!(
            input_rx.try_recv().is_err(),
            "wheel should not produce extra PTY input"
        );
    }

    fn feed_top_anchored_inline_history(session: &mut Session, region_bottom: u16, lines: usize) {
        session.feed_pty(format!("\x1b[1;{region_bottom}r\x1b[{region_bottom};1H").as_bytes());
        for i in 0..lines {
            session.feed_pty(format!("\r\n\x1b[2Khistory {i}").as_bytes());
        }
        session.feed_pty(b"\x1b[r");
    }

    fn test_session_with_agent(
        rows: u16,
        cols: u16,
        agent: Option<String>,
    ) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        (
            Session::new_for_test(
                "Test".to_string(),
                agent,
                (rows, cols),
                100,
                input_tx,
                Arc::new(Mutex::new(Box::new(NullMasterPty))),
                Arc::new(Mutex::new(Box::new(NullChildKiller))),
            ),
            input_rx,
        )
    }

    fn split_tab_mux() -> Multiplexer {
        let mut mux = test_mux(24, 80);
        let mut tab = Tab::new_single("Shell", 1);
        assert!(tab.tree.split_h(1, 2, SplitPosition::After));
        mux.tabs.push(tab);
        mux
    }

    #[test]
    fn resize_zero_zero_normalizes_to_default_dimensions() {
        // A client sending Resize { rows: 0, cols: 0 } is asking for
        // "use the defaults"; the daemon must floor through
        // `normalize_size` and never store 0 in `term_rows`/`term_cols`,
        // because zero-row PTYs collapse vt100 rendering.
        let mut mux = test_mux(48, 160);
        mux.resize(0, 0);
        assert_eq!(
            (mux.term_rows, mux.term_cols),
            (
                crate::terminal_geometry::DEFAULT_ROWS,
                crate::terminal_geometry::DEFAULT_COLS
            )
        );
    }

    #[test]
    fn initial_spawn_request_is_data_only_agent_or_shell() {
        assert_eq!(
            initial_spawn_request("codex"),
            SpawnRequest::Agent("codex".to_string())
        );
        assert_eq!(initial_spawn_request(""), SpawnRequest::Shell);
    }

    #[test]
    fn spawn_request_rejects_agent_outside_allowlist_before_pty_spawn() {
        let mut mux = test_mux(24, 80);
        mux.available_agents = vec!["codex".to_string()];

        let err = mux
            .spawn_request(SpawnRequest::Agent("claude".to_string()), &[])
            .unwrap_err();

        assert!(err.to_string().contains("rejected agent \"claude\""));
        assert!(mux.sessions.is_empty());
    }

    #[test]
    fn command_palette_labels_single_pane_close_as_close_tab() {
        let mut mux = single_pane_tab_mux();
        mux.open_command_palette();

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::CommandPalette {
                close_label: PaletteCloseLabel::CloseTab,
                ..
            })
        ));
    }

    #[test]
    fn dialog_backdrop_dims_full_multiplexer_chrome() {
        fn mux_with_two_sessions() -> Multiplexer {
            let mut mux = split_tab_mux();
            let (session_one, _) = test_session(24, 80);
            let (session_two, _) = test_shell_session(24, 80);
            mux.sessions.insert(1, session_one);
            mux.sessions.insert(2, session_two);
            mux
        }

        fn assert_backdrop_dimmed(mut mux: Multiplexer, context: &str) {
            let frame =
                String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::DialogChange))
                    .to_string();

            assert!(
                frame.contains("\x1b[48;2;0;51;13m"),
                "{context} should dim the top status brand: {frame:?}"
            );
            assert!(
                frame.contains("\x1b[48;2;51;51;51m"),
                "{context} should dim the bottom context bar: {frame:?}"
            );
            assert!(
                frame.contains("\x1b[38;2;48;48;48m┌"),
                "{context} should dim pane borders behind the dialog: {frame:?}"
            );
            assert!(
                !frame.contains("\x1b[38;2;0;255;65m┌"),
                "{context} should not leave the active pane border green behind the dialog: {frame:?}"
            );
        }

        let mut menu_mux = mux_with_two_sessions();
        menu_mux.open_command_palette();
        assert_backdrop_dimmed(menu_mux, "menu dialog");

        let mut container_mux = mux_with_two_sessions();
        container_mux.open_container_info_dialog();
        assert_backdrop_dimmed(container_mux, "container info dialog");

        let mut github_mux = mux_with_two_sessions();
        github_mux.pull_request_context_branch = Some("feat/capsule-pr-context-bar".to_string());
        github_mux.pull_request_context = Some(pull_request_fixture(436));
        github_mux.open_github_context_dialog();
        assert_backdrop_dimmed(github_mux, "GitHub context dialog");
    }

    #[test]
    fn palette_close_single_pane_opens_confirm_directly() {
        let mut mux = single_pane_tab_mux();
        mux.handle_palette_command(PaletteCommand::Close);

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ConfirmAction {
                kind: ConfirmKind::CloseTab,
                selected_yes: false
            })
        ));
    }

    #[test]
    fn palette_close_split_tab_opens_target_picker() {
        let mut mux = split_tab_mux();
        mux.handle_palette_command(PaletteCommand::Close);

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::CloseTargetPicker {
                selected: 0,
                filter
            }) if filter.is_empty()
        ));
    }

    #[test]
    fn branch_context_bar_renders_pr_id_title_and_container_without_url() {
        let pr = pull_request_fixture(434);
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            120,
            Some("asa/pr-context"),
            Some(&pr),
            false,
            "jk-test-container",
            None,
            false,
        );
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("\x1b[24;1H"));
        assert!(rendered.contains("\x1b[48;2;255;255;255m"));
        assert!(rendered.contains("PR #434"));
        assert!(!rendered.contains("asa/pr-context"));
        assert!(rendered.contains("Surface PR context in Capsule"));
        assert!(rendered.contains("jk-test-container"));
        assert!(!rendered.contains("https://github.com/jackin-project/jackin/pull/434"));
        assert!(!rendered.contains("\x1b]8;;"));
    }

    #[test]
    fn branch_context_bar_renders_non_default_branch_without_pr() {
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            80,
            Some("feature/no-pr"),
            None,
            false,
            "jk-test-container",
            None,
            false,
        );
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("Branch · feature/no-pr"));
        assert!(rendered.contains("jk-test-container"));
        assert!(!rendered.contains("\x1b]8;;"));
    }

    #[test]
    fn branch_context_bar_shows_pr_lookup_in_progress() {
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            100,
            Some("feature/slow-gh"),
            None,
            true,
            "jk-test-container",
            None,
            false,
        );
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("Resolving PR · feature/slow-gh"));
        assert!(!rendered.contains("Branch · feature/slow-gh"));
    }

    #[test]
    fn branch_context_bar_truncates_left_chunk_on_narrow_terminal() {
        // 20-column terminal with a long PR title: the left chunk
        // must truncate via `take_display_cols` and the container
        // region must drop entirely rather than overlap the left
        // chunk. No panic from `u16::try_from` overflow.
        let mut pr = pull_request_fixture(999);
        pr.title =
            "Implement enormous feature with very long title that exceeds the bar".to_string();
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            20,
            Some("feature/x"),
            Some(&pr),
            false,
            "jk-test-container-with-extra-long-suffix",
            None,
            false,
        );
        let rendered = String::from_utf8_lossy(&buf);
        assert!(rendered.contains("PR #999"));
        // The container label should be omitted at this width — its
        // chunk would otherwise collide with the (already-truncated)
        // left chunk.
        assert!(
            !rendered.contains("jk-test-container-with-extra-long-suffix"),
            "narrow terminal must drop container chunk: {rendered:?}"
        );
    }

    #[test]
    fn branch_context_bar_layout_returns_none_for_zero_dimensions() {
        let pr = pull_request_fixture(1);
        assert!(
            branch_context_bar_layout(0, 80, Some("feature/x"), Some(&pr), false, "jk-test")
                .is_none()
        );
        assert!(
            branch_context_bar_layout(24, 0, Some("feature/x"), Some(&pr), false, "jk-test")
                .is_none()
        );
    }

    #[test]
    fn branch_context_bar_hit_rejects_columns_outside_region() {
        let pr = pull_request_fixture(7);
        let layout =
            branch_context_bar_layout(24, 120, Some("feature/x"), Some(&pr), false, "jk-test")
                .expect("layout fits");
        // left_region covers exactly its declared range.
        let (left_start, left_end) = layout.left_region.expect("left region present");
        assert_eq!(
            branch_context_bar_hit(
                24,
                left_start,
                24,
                120,
                Some("feature/x"),
                Some(&pr),
                false,
                "jk-test"
            ),
            Some(BranchContextBarHit::Context)
        );
        assert_eq!(
            branch_context_bar_hit(
                24,
                left_end - 1,
                24,
                120,
                Some("feature/x"),
                Some(&pr),
                false,
                "jk-test"
            ),
            Some(BranchContextBarHit::Context)
        );
        // `end` is exclusive — `col == end` is outside the region.
        let outside_left = branch_context_bar_hit(
            24,
            left_end,
            24,
            120,
            Some("feature/x"),
            Some(&pr),
            false,
            "jk-test",
        );
        // The column may belong to the container region if it abuts.
        assert!(matches!(
            outside_left,
            None | Some(BranchContextBarHit::Container)
        ));
        // Wrong row — never a hit.
        assert_eq!(
            branch_context_bar_hit(
                23,
                left_start,
                24,
                120,
                Some("feature/x"),
                Some(&pr),
                false,
                "jk-test"
            ),
            None
        );
    }

    #[test]
    fn branch_context_bar_hover_highlights_click_targets() {
        let pr = pull_request_fixture(434);
        let mut context_buf = Vec::new();
        render_branch_context_bar(
            &mut context_buf,
            24,
            120,
            Some("asa/pr-context"),
            Some(&pr),
            false,
            "jk-test-container",
            Some(HoverTarget::BranchContext),
            false,
        );
        let context_rendered = String::from_utf8_lossy(&context_buf);
        assert!(context_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_BG));
        assert!(context_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_FG));

        let mut container_buf = Vec::new();
        render_branch_context_bar(
            &mut container_buf,
            24,
            120,
            Some("asa/pr-context"),
            Some(&pr),
            false,
            "jk-test-container",
            Some(HoverTarget::Container),
            false,
        );
        let container_rendered = String::from_utf8_lossy(&container_buf);
        assert!(container_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_BG));
        assert!(container_rendered.contains("jk-test-container"));
    }

    #[test]
    fn branch_context_bar_leaves_left_side_empty_on_default_branches() {
        let mut main_buf = Vec::new();
        render_branch_context_bar(
            &mut main_buf,
            24,
            80,
            Some("main"),
            None,
            false,
            "jk-test-container",
            None,
            false,
        );
        let main_rendered = String::from_utf8_lossy(&main_buf);
        assert!(main_rendered.contains("jk-test-container"));
        assert!(!main_rendered.contains("jackin"));
        assert!(!main_rendered.contains("Branch · main"));
        assert!(!main_rendered.contains("PR #"));
        assert_eq!(
            branch_context_bar_hit(
                24,
                2,
                24,
                80,
                Some("main"),
                None,
                false,
                "jk-test-container",
            ),
            None
        );

        let mut master_buf = Vec::new();
        render_branch_context_bar(
            &mut master_buf,
            24,
            80,
            Some("master"),
            None,
            false,
            "jk-test-container",
            None,
            false,
        );
        let master_rendered = String::from_utf8_lossy(&master_buf);
        assert!(master_rendered.contains("jk-test-container"));
        assert!(!master_rendered.contains("jackin"));
        assert!(!master_rendered.contains("Branch · master"));
        assert!(!master_rendered.contains("PR #"));
    }

    #[test]
    fn branch_context_visibility_keeps_content_area_reserved() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        assert_eq!(mux.content_rows, 21);

        mux.pull_request_context_cache.insert(
            "asa/pr-context".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now,
                pull_request: Some(pull_request_fixture(434)),
            },
        );
        assert!(mux.apply_git_branch_context(Some("asa/pr-context".to_string()), now));
        assert_eq!(mux.content_rows, 21);
        assert_eq!(
            mux.pull_request_context.as_ref().map(|pr| pr.number),
            Some(434)
        );

        mux.pull_request_context_cache.insert(
            "feature/no-pr".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now,
                pull_request: None,
            },
        );
        assert!(mux.apply_git_branch_context(Some("feature/no-pr".to_string()), now));
        assert_eq!(mux.content_rows, 21);
        assert!(mux.pull_request_context.is_none());

        assert!(mux.apply_git_branch_context(Some("main".to_string()), now));
        assert_eq!(mux.content_rows, 21);
        assert!(mux.pull_request_context.is_none());
    }

    #[test]
    fn git_branch_context_updates_status_before_github_lookup() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_branch = Some("old/pr".to_string());
        mux.pull_request_context = Some(pull_request_fixture(434));
        mux.reconcile_content_rows();
        assert_eq!(mux.content_rows, 21);

        mux.pull_request_context_cache.insert(
            "new/local-branch".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now,
                pull_request: None,
            },
        );
        assert!(mux.apply_git_branch_context(Some("new/local-branch".to_string()), now));

        assert_eq!(
            mux.pull_request_context_branch.as_deref(),
            Some("new/local-branch")
        );
        assert!(mux.pull_request_context.is_none());
        assert_eq!(mux.content_rows, 21);
    }

    #[test]
    fn git_branch_context_recognizes_repo_after_startup() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.workdir_context.is_git_repo = false;
        mux.workdir_context.gh_available = false;

        assert!(mux.apply_git_branch_context(Some("feat/capsule-pr-context-bar".to_string()), now));

        assert!(mux.workdir_context.is_git_repo);
        assert_eq!(
            mux.context_bar_branch(),
            Some("feat/capsule-pr-context-bar")
        );
        assert!(mux.pull_request_context.is_none());
    }

    #[test]
    fn apply_pull_request_context_loaded_drops_stale_request() {
        let mut mux = test_mux(24, 100);
        mux.pull_request_lookup.request_id = 5;
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_branch = Some("feat/x".to_string());
        let pr = pull_request_fixture(99);
        let changed = mux.apply_pull_request_context_loaded(
            3,
            Some("feat/x".to_string()),
            PullRequestLookupOutcome::Resolved(Some(pr)),
            Instant::now(),
        );
        assert!(!changed, "stale request must not mutate state");
        assert!(
            mux.pull_request_lookup.in_flight,
            "stale request must leave in_flight untouched"
        );
        assert!(
            mux.pull_request_context.is_none(),
            "stale request must not write PR"
        );
    }

    #[test]
    fn apply_pull_request_context_loaded_transient_failure_preserves_prior_cache() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_lookup.request_id = 7;
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_branch = Some("feat/x".to_string());
        mux.pull_request_context = Some(pull_request_fixture(123));
        mux.pull_request_context_cache.insert(
            "feat/x".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now - Duration::from_secs(5),
                pull_request: Some(pull_request_fixture(123)),
            },
        );
        let changed = mux.apply_pull_request_context_loaded(
            7,
            Some("feat/x".to_string()),
            PullRequestLookupOutcome::TransientFailure,
            now,
        );
        assert!(!changed, "transient failure must not mutate visible state");
        assert!(
            !mux.pull_request_lookup.in_flight,
            "transient failure must clear in_flight so next tick retries"
        );
        assert_eq!(
            mux.pull_request_context_cache
                .get("feat/x")
                .and_then(|e| e.pull_request.as_ref().map(|p| p.number)),
            Some(123),
            "cache must be untouched by transient failure"
        );
    }

    #[test]
    fn apply_git_branch_context_loaded_drops_stale_request() {
        let mut mux = test_mux(24, 100);
        mux.git_branch_lookup.request_id = 4;
        mux.git_branch_lookup.in_flight = true;
        let changed =
            mux.apply_git_branch_context_loaded(2, Some("feat/x".to_string()), Instant::now());
        assert!(!changed);
        assert!(mux.git_branch_lookup.in_flight, "stale id leaves in_flight");
        assert!(mux.pull_request_context_branch.is_none());
    }

    #[test]
    fn apply_git_branch_context_bumps_pr_request_id_on_branch_change() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_branch = Some("feat/a".to_string());
        mux.workdir_context.gh_available = false;
        let id_before = mux.pull_request_lookup.request_id;
        let _ = mux.apply_git_branch_context(Some("feat/b".to_string()), now);
        assert_eq!(
            mux.pull_request_lookup.request_id,
            id_before.wrapping_add(1),
            "branch change must bump request_id so stale gh worker responses are rejected"
        );
    }

    #[test]
    fn purge_expired_pull_request_cache_entries_drops_old_entries() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        let ttl = PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL * 2;
        mux.pull_request_context_cache.insert(
            "feat/fresh".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now - Duration::from_secs(10),
                pull_request: Some(pull_request_fixture(1)),
            },
        );
        mux.pull_request_context_cache.insert(
            "feat/old".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now - ttl - Duration::from_secs(1),
                pull_request: Some(pull_request_fixture(2)),
            },
        );
        mux.purge_expired_pull_request_cache_entries(now);
        assert!(mux.pull_request_context_cache.contains_key("feat/fresh"));
        assert!(!mux.pull_request_context_cache.contains_key("feat/old"));
    }

    #[test]
    fn pull_request_cache_fresh_at_strict_boundary() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        // Just-fresh: at the boundary minus 1 ms.
        mux.pull_request_context_cache.insert(
            "branch-a".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL + Duration::from_millis(1),
                pull_request: None,
            },
        );
        // Just-stale: at the boundary plus 1 ms.
        mux.pull_request_context_cache.insert(
            "branch-b".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL - Duration::from_millis(1),
                pull_request: None,
            },
        );
        assert!(mux.pull_request_cache_is_fresh("branch-a", now));
        assert!(!mux.pull_request_cache_is_fresh("branch-b", now));
    }

    #[test]
    fn git_branch_context_keeps_current_pr_while_refreshing_same_branch() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_branch = Some("feature/current".to_string());
        mux.pull_request_context = Some(pull_request_fixture(436));
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_cache.insert(
            "feature/current".to_string(),
            PullRequestContextCacheEntry {
                checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL,
                pull_request: Some(pull_request_fixture(436)),
            },
        );

        assert!(!mux.apply_git_branch_context(Some("feature/current".to_string()), now));
        assert_eq!(
            mux.pull_request_context.as_ref().map(|pr| pr.number),
            Some(436)
        );
    }

    #[test]
    fn read_branch_from_git_head_reads_normal_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();

        assert_eq!(
            read_branch_from_git_head(temp.path()).as_deref(),
            Some("feat/context")
        );
    }

    #[test]
    fn read_branch_from_git_head_reads_worktree_gitdir_file() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workdir");
        let git_dir = temp.path().join("repo/.git/worktrees/workdir");
        std::fs::create_dir_all(&workdir).unwrap();
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(
            workdir.join(".git"),
            "gitdir: ../repo/.git/worktrees/workdir\n",
        )
        .unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/worktree\n").unwrap();

        assert_eq!(
            read_branch_from_git_head(&workdir).as_deref(),
            Some("feat/worktree")
        );
    }

    #[test]
    fn palette_exit_opens_exit_confirm() {
        let mut mux = single_pane_tab_mux();
        mux.handle_palette_command(PaletteCommand::Exit);

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ConfirmAction {
                kind: ConfirmKind::Exit,
                selected_yes: false
            })
        ));
    }

    #[test]
    fn kitty_escape_in_agent_picker_returns_to_menu() {
        let mut mux = single_pane_tab_mux();
        mux.open_command_palette();
        let frame = mux
            .handle_input(InputEvent::Data(b"\r".to_vec()))
            .expect("New tab command should redraw");
        assert!(String::from_utf8_lossy(&frame).contains("New tab"));
        assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));

        let events = mux.input_parser.parse(b"\x1b[27;1u");
        assert_eq!(events, vec![InputEvent::Data(b"\x1b".to_vec())]);
        for event in events {
            mux.handle_input(event);
        }

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::CommandPalette { .. })
        ));
    }

    #[test]
    fn mouse_sgr_encoding_preserves_press_and_release() {
        assert_eq!(
            encode_mouse_for_protocol(0, 12, 3, true, vt100::MouseProtocolEncoding::Sgr).unwrap(),
            b"\x1b[<0;12;3M"
        );
        assert_eq!(
            encode_mouse_for_protocol(0, 12, 3, false, vt100::MouseProtocolEncoding::Sgr).unwrap(),
            b"\x1b[<0;12;3m"
        );
    }

    #[test]
    fn mouse_default_encoding_uses_xterm_fields() {
        assert_eq!(
            encode_mouse_for_protocol(0, 12, 3, true, vt100::MouseProtocolEncoding::Default)
                .unwrap(),
            b"\x1b[M ,#"
        );
        assert_eq!(
            encode_mouse_for_protocol(0, 12, 3, false, vt100::MouseProtocolEncoding::Default)
                .unwrap(),
            b"\x1b[M#,#"
        );
    }

    #[test]
    fn mouse_mode_filter_respects_tracking_granularity() {
        use vt100::MouseProtocolMode;

        assert!(!mouse_event_allowed_for_mode(
            MouseProtocolMode::None,
            0,
            true
        ));
        assert!(mouse_event_allowed_for_mode(
            MouseProtocolMode::Press,
            0,
            true
        ));
        assert!(!mouse_event_allowed_for_mode(
            MouseProtocolMode::Press,
            0,
            false
        ));
        assert!(!mouse_event_allowed_for_mode(
            MouseProtocolMode::PressRelease,
            32,
            true
        ));
        assert!(mouse_event_allowed_for_mode(
            MouseProtocolMode::ButtonMotion,
            32,
            true
        ));
        assert!(!mouse_event_allowed_for_mode(
            MouseProtocolMode::ButtonMotion,
            SGR_NO_BUTTON_MOTION,
            true
        ));
        assert!(mouse_event_allowed_for_mode(
            MouseProtocolMode::AnyMotion,
            SGR_NO_BUTTON_MOTION,
            true
        ));
    }

    #[test]
    fn wheel_forwards_to_mouse_enabled_tui() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_session(20, 78);
        session.feed_pty(b"\x1b[?1049h\x1b[?1003h\x1b[?1006h");
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        assert!(
            redraw.is_none(),
            "pane-owned wheel should not redraw jackin'"
        );
        assert_eq!(
            input_rx.try_recv().expect("wheel should reach PTY"),
            b"\x1b[<64;1;1M"
        );
        assert!(
            input_rx.try_recv().is_err(),
            "wheel should not produce extra PTY input"
        );
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
    }

    #[test]
    fn wheel_scrolls_jackin_scrollback_when_mouse_is_disabled() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux();
            let (mut session, mut input_rx) = test_pane_session(20, 78, agent);
            for i in 0..40 {
                session.feed_pty(format!("line {i}\r\n").as_bytes());
            }
            assert_eq!(session.scrollback_offset, 0);
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            });

            assert!(
                redraw.is_some(),
                "{pane_kind} pane scrollback should redraw jackin'"
            );
            assert!(
                input_rx.try_recv().is_err(),
                "mouse-disabled {pane_kind} panes must not receive raw wheel bytes"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
        }
    }

    #[test]
    fn wheel_noops_for_focused_normal_screen_pane_without_scrollback() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(55, 200);
            let (mut session, mut input_rx) = test_pane_session(51, 198, agent);
            session.feed_pty(b"\x1b[49;3Hcodex prompt");
            assert_eq!(session.scrollback_filled(), 0);
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 10,
                col: 10,
                button: 64,
            });

            assert!(
                redraw.is_none(),
                "{pane_kind} normal-screen pane without scrollback should not redraw jackin'"
            );
            assert!(
                input_rx.try_recv().is_err(),
                "normal-screen {pane_kind} pane without scrollback must not receive cursor-key wheel fallback"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
        }
    }

    #[test]
    fn wheel_scrolls_top_anchored_inline_history_for_all_panes() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(12, 40);
            let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
            feed_top_anchored_inline_history(&mut session, 5, 12);
            session.feed_pty(b"\x1b[8;1Hlive prompt");
            assert!(
                session.scrollback_filled() >= 3,
                "{pane_kind} pane should retain top-anchored inline history"
            );
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            });

            let frame = redraw.expect("inline history wheel should redraw");
            assert!(
                input_rx.try_recv().is_err(),
                "{pane_kind} pane must not receive cursor-key wheel fallback"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
            assert_focused_scroll_chrome(
                &frame,
                &format!("normal-screen {pane_kind} pane with inline history"),
            );
            assert!(
                String::from_utf8_lossy(&frame).contains("history 4"),
                "normal-screen {pane_kind} wheel should render retained inline history"
            );
        }
    }

    #[test]
    fn wheel_scrolls_normal_screen_history_preserved_before_clear_for_all_panes() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(12, 40);
            let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
            for i in 0..5 {
                session.feed_pty(format!("release note {i}\r\n").as_bytes());
            }
            assert_eq!(
                session.scrollback_filled(),
                0,
                "{pane_kind} setup output fits without native scrollback before clear"
            );

            session.feed_pty(b"\x1b[1;1H\x1b[Jlive prompt");
            assert!(
                session.scrollback_filled() >= 5,
                "{pane_kind} pane should preserve normal-screen rows erased by clear/redraw"
            );
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            });

            let frame = redraw.expect("clear-preserved history wheel should redraw");
            assert!(
                input_rx.try_recv().is_err(),
                "{pane_kind} pane must not receive cursor-key wheel fallback"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
            assert_focused_scroll_chrome(
                &frame,
                &format!("normal-screen {pane_kind} pane with clear-preserved history"),
            );
            assert!(
                String::from_utf8_lossy(&frame).contains("release note"),
                "normal-screen {pane_kind} wheel should render rows preserved before clear"
            );
        }
    }

    #[test]
    fn wheel_scrolls_csi_scroll_up_inline_history_for_all_panes() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(12, 40);
            let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
            session.feed_pty(b"\x1b[1;5r\x1b[1;1Htop row\x1b[2;1Hsecond row\x1b[3;1Hthird row");
            session.feed_pty(b"\x1b[2S\x1b[r\x1b[8;1Hlive prompt");
            assert!(
                session.scrollback_filled() >= 2,
                "{pane_kind} pane should retain rows removed by top-anchored CSI S"
            );
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            });

            let frame = redraw.expect("CSI S inline history wheel should redraw");
            assert!(
                input_rx.try_recv().is_err(),
                "{pane_kind} pane must not receive cursor-key wheel fallback"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 2);
            assert_focused_scroll_chrome(
                &frame,
                &format!("normal-screen {pane_kind} pane with CSI S inline history"),
            );
            assert!(
                String::from_utf8_lossy(&frame).contains("top row"),
                "normal-screen {pane_kind} wheel should render CSI S retained history"
            );
        }
    }

    #[test]
    fn wheel_sends_cursor_fallback_to_mouse_disabled_alt_screen_tui() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_shell_session(20, 78);
        session.feed_pty(b"\x1b[?1049h");
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        assert!(
            redraw.is_none(),
            "pane-owned fallback should not redraw jackin'"
        );
        assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
    }

    #[test]
    fn wheel_sends_cursor_fallback_to_alt_screen_tui_with_retained_primary_scrollback() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_shell_session(20, 78);
        for i in 0..40 {
            session.feed_pty(format!("line {i}\r\n").as_bytes());
        }
        assert!(
            session.scrollback_filled() > 0,
            "setup should leave retained primary-screen scrollback"
        );
        session.feed_pty(b"\x1b[?1049h");
        assert!(
            session.screen().alternate_screen(),
            "setup should leave pane in the alternate screen"
        );
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        assert!(
            redraw.is_none(),
            "alternate-screen fallback should not redraw jackin'"
        );
        assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
    }

    #[test]
    fn wheel_cursor_fallback_respects_application_cursor_mode() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_session(20, 78);
        session.feed_pty(b"\x1b[?1049h\x1b[?1h");
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 65,
        });

        assert!(
            redraw.is_none(),
            "pane-owned fallback should not redraw jackin'"
        );
        assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1bOB\x1bOB\x1bOB");
    }

    #[test]
    fn alt_screen_overflow_does_not_draw_scrollbar_without_retained_scrollback() {
        let mut mux = single_pane_tab_mux();
        let (mut session, _input_rx) = test_session(8, 20);
        session.feed_pty(b"\x1b[?1049h");
        for i in 0..20 {
            session.feed_pty(format!("line {i}\r\n").as_bytes());
        }
        assert_eq!(session.scrollback_filled(), 0);
        mux.sessions.insert(1, session);

        let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
        assert_no_scroll_thumb(&frame, "alt-screen pane without retained scrollback");
    }

    #[test]
    fn normal_screen_panes_do_not_draw_scrollbar_when_grid_is_full_without_scrollback() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux();
            let (mut session, _input_rx) = test_pane_session(8, 20, agent);
            for row in 0..8 {
                session.feed_pty(format!("\x1b[{};1Hrow {row}", row + 1).as_bytes());
            }
            mux.sessions.insert(1, session);

            let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
            assert_no_scroll_thumb(
                &frame,
                &format!("normal-screen {pane_kind} pane with full grid but no scrollback"),
            );
        }
    }

    #[test]
    fn normal_screen_panes_do_not_draw_scrollbar_when_content_spans_viewport_without_scrollback() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux();
            let (mut session, _input_rx) = test_pane_session(8, 20, agent);
            session.feed_pty(b"\x1b[1;1Htop transcript\x1b[8;1Hbottom status");
            assert_eq!(session.scrollback_filled(), 0);
            mux.sessions.insert(1, session);

            let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
            assert_no_scroll_thumb(
                &frame,
                &format!(
                    "normal-screen {pane_kind} pane with viewport-spanning content but no scrollback"
                ),
            );
        }
    }

    #[test]
    fn normal_screen_panes_do_not_keep_scrollbar_when_cursor_moves_without_scrollback() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(55, 200);
            let (mut session, _input_rx) = test_pane_session(51, 198, agent);
            session.feed_pty(b"\x1b[1;1Hrelease notes\x1b[51;1Hstatus line\x1b[48;3Hx");
            assert_eq!(session.scrollback_filled(), 0);
            mux.sessions.insert(1, session);

            let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
            assert_no_scroll_thumb(
                &frame,
                &format!("normal-screen {pane_kind} transcript pane after cursor moved up"),
            );
        }
    }

    #[test]
    fn alt_screen_exit_resets_keyboard_modes_for_shell_prompt() {
        let (mut session, _input_rx) = test_session(8, 20);
        session.feed_pty(b"\x1b[?1049h\x1b[>1u\x1b[>4;2m");
        let _ = session.drain_passthrough();

        session.feed_pty(b"\x1b[?1049l");
        let drained = session.drain_passthrough();

        assert!(
            drained.iter().any(|bytes| bytes == b"\x1b[<u"),
            "kitty keyboard reset missing from {drained:?}"
        );
        assert!(
            drained.iter().any(|bytes| bytes == b"\x1b[>4;0m"),
            "modifyOtherKeys reset missing from {drained:?}"
        );
    }

    #[test]
    fn osc22_pointer_shape_uses_css_names() {
        assert_eq!(
            osc22_pointer_shape(PointerShape::Pointer),
            b"\x1b]22;pointer\x1b\\"
        );
        assert_eq!(
            osc22_pointer_shape(PointerShape::EwResize),
            b"\x1b]22;ew-resize\x1b\\"
        );
    }

    #[test]
    fn pointer_shape_updates_only_when_shape_changes() {
        let mut mux = test_mux(24, 80);
        mux.pointer_shapes_supported = true;
        mux.status_bar.identity_label = "jk-test-container".to_string();
        mux.status_bar.instance_id_label = "test".to_string();
        mux.pull_request_context_branch = Some("feature/context".to_string());
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let hit = branch_context_bar_layout(
            mux.term_rows,
            mux.term_cols,
            mux.pull_request_context_branch.as_deref(),
            mux.pull_request_context.as_ref(),
            mux.pull_request_context_loading(),
            mux.status_bar.instance_id_label(),
        )
        .and_then(|layout| layout.left_region)
        .expect("branch context should fit");

        mux.update_pointer_shape_for_mouse(23, hit.0 - 1, SGR_NO_BUTTON_MOTION);
        let first = rx.try_recv().expect("first pointer-shape update");
        assert!(first.ends_with(b"\x1b]22;pointer\x1b\\"));

        mux.update_pointer_shape_for_mouse(23, hit.0, SGR_NO_BUTTON_MOTION);
        assert!(rx.try_recv().is_err(), "unchanged shape should not re-emit");
    }

    #[test]
    fn bottom_container_click_opens_container_info_without_copying() {
        let mut mux = test_mux(24, 80);
        mux.pointer_shapes_supported = false;
        mux.status_bar.identity_label = "jk-test-container".to_string();
        mux.status_bar.instance_id_label = "test".to_string();
        mux.status_bar.role = "the-architect".to_string();
        mux.pull_request_context_branch = Some("feature/context".to_string());
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let hit = branch_context_bar_layout(
            mux.term_rows,
            mux.term_cols,
            mux.pull_request_context_branch.as_deref(),
            mux.pull_request_context.as_ref(),
            mux.pull_request_context_loading(),
            mux.status_bar.instance_id_label(),
        )
        .and_then(|layout| layout.container_region)
        .expect("container should fit");

        let frame = mux
            .handle_input(InputEvent::MousePress {
                row: mux.term_rows - 1,
                col: hit.0 - 1,
                button: 0,
            })
            .expect("container click should redraw");

        while let Ok(output) = rx.try_recv() {
            assert!(
                !output
                    .windows(b"\x1b]52;c;".len())
                    .any(|w| w == b"\x1b]52;c;"),
                "opening container info must not send OSC 52"
            );
        }
        assert!(!String::from_utf8_lossy(&frame).contains("Copied!"));
        let Some(Dialog::ContainerInfo {
            copied: false,
            workdir,
            ..
        }) = mux.dialog_top()
        else {
            panic!("identity click should open container info")
        };
        assert_eq!(workdir, "/workspace");
    }

    #[test]
    fn bottom_context_click_opens_github_context_dialog() {
        let mut mux = test_mux(24, 100);
        mux.status_bar.identity_label = "jk-test-container".to_string();
        mux.status_bar.instance_id_label = "test".to_string();
        mux.pull_request_context_branch = Some("feature/context".to_string());
        mux.pull_request_context = Some(pull_request_fixture(434));
        let hit = branch_context_bar_layout(
            mux.term_rows,
            mux.term_cols,
            mux.pull_request_context_branch.as_deref(),
            mux.pull_request_context.as_ref(),
            mux.pull_request_context_loading(),
            mux.status_bar.instance_id_label(),
        )
        .and_then(|layout| layout.left_region)
        .expect("GitHub context should fit");

        let frame = mux
            .handle_input(InputEvent::MousePress {
                row: mux.term_rows - 1,
                col: hit.0 - 1,
                button: 0,
            })
            .expect("context click should redraw");

        let rendered = String::from_utf8_lossy(&frame);
        assert!(rendered.contains("GitHub context"));
        assert!(
            rendered.contains("copy GitHub URL"),
            "dialog hint must render above the bottom branch/context bar: {rendered:?}"
        );
        assert!(
            rendered.rfind("copy GitHub URL") > rendered.rfind("test"),
            "dialog footer should be painted after the bottom branch/context bar so it clears its own rows: {rendered:?}"
        );
        let hint_row = mux.term_rows - 2;
        let bottom_row = mux.term_rows;
        assert!(
            rendered.contains(&format!("\x1b[{hint_row};")),
            "dialog hint should render one row above the spacer: {rendered:?}"
        );
        assert!(
            rendered.contains(&format!("\x1b[{bottom_row};")),
            "bottom branch/context bar should stay on the final row: {rendered:?}"
        );
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::GitHubContext {
                branch: Some(branch),
                pull_request: Some(pr),
                pull_request_loading: false,
                copied: false,
            }) if branch == "feature/context" && pr.number == 434
        ));
    }

    #[test]
    fn container_info_copy_feedback_expires() {
        let mut mux = test_mux(24, 80);
        mux.dialog_push(Dialog::ContainerInfo {
            container_name: "jk-test-container".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace".to_string(),
            copied: true,
        });
        let now = Instant::now();
        mux.dialog_copy_feedback_deadline = Some(now);

        assert!(mux.expire_dialog_copy_feedback(now));
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ContainerInfo { copied: false, .. })
        ));
    }

    #[test]
    fn container_info_id_click_copies_and_renders_feedback() {
        let mut mux = test_mux(40, 120);
        mux.pointer_shapes_supported = false;
        mux.dialog_push(Dialog::ContainerInfo {
            container_name: "jk-test-container".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace".to_string(),
            copied: false,
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let (box_row, box_col, _, _) = mux
            .dialog_top()
            .expect("container info dialog should be open")
            .box_rect(mux.term_rows, mux.term_cols);

        let frame = mux
            .handle_input(InputEvent::MousePress {
                row: box_row + 1,
                col: box_col + 1,
                button: 0,
            })
            .expect("container id click should redraw copy feedback");

        let mut saw_osc52 = false;
        while let Ok(output) = rx.try_recv() {
            saw_osc52 |= output
                .windows(b"\x1b]52;c;".len())
                .any(|w| w == b"\x1b]52;c;");
        }
        assert!(saw_osc52, "copy should emit OSC 52");
        assert!(String::from_utf8_lossy(&frame).contains("Copied!"));
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ContainerInfo { copied: true, .. })
        ));
    }

    #[test]
    fn prefix_ctrl_l_has_named_pane_clear_reason() {
        assert_eq!(
            prefix_full_redraw_reason(&PrefixCommand::ClearPane),
            FullRedrawReason::PaneClear
        );
    }

    #[test]
    fn command_stdout_trimmed_returns_trimmed_stdout() {
        let mut command = Command::new("printf");
        command.arg("  branch-name\n");

        assert_eq!(
            command_stdout_trimmed(&mut command),
            Some("branch-name".to_string())
        );
    }

    #[test]
    fn command_stdout_trimmed_rejects_known_failure_status() {
        // `sleep 0.05` keeps the child alive long enough for the
        // try_wait poll loop to observe `Ok(None)` first and then the
        // failing `Ok(Some(1))` exit on the next tick. Without the
        // sleep the child can vanish between spawn and the first
        // try_wait, which collapses the Err(ECHILD) "status lost"
        // arm and the Ok(Some(false)) "failed" arm into one path.
        let mut command = Command::new("sh");
        command.args(["-c", "printf branch-name; sleep 0.05; exit 1"]);

        assert_eq!(command_stdout_trimmed(&mut command), None);
    }
}
