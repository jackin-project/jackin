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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::dialog::{
    ConfirmKind, Dialog, DialogAction, PaletteCloseLabel, PaletteCommand, PickerIntent,
    SplitDirection,
};
use crate::input::{ArrowDir, InputEvent, InputParser, PrefixCommand};
use crate::layout::{Direction, Rect, SplitOrient, SplitPosition, Tab};
use crate::protocol::attach::{
    ClientFrame, ServerFrame, SpawnRequest, encode_server, read_client_frame,
};
use crate::protocol::control::{AgentState, SessionInfo};
use crate::render::{PaneBodyCache, PaneBodyRenderMode, draw_scrollbar};
use crate::session::{
    SESSION_ENV_PASSTHROUGH, Session, SessionEvent, available_agents, build_agent_command,
    build_shell_command,
};
use crate::socket;
use crate::statusbar::{STATUS_BAR_ROWS, StatusBar, draw_pane_box};
use crate::terminal_geometry::{DEFAULT_COLS, DEFAULT_ROWS, normalize_size};

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
    /// Deadline for hiding the transient "Copied!" badge in the
    /// container-info dialog after a jackin-owned OSC 52 copy.
    container_info_copy_deadline: Option<Instant>,
    /// Monotonic token for the background git / GitHub metadata
    /// lookup backing the currently-open container-info dialog.
    container_info_request_id: u64,
    /// Git / GitHub metadata lookup waiting until after the initial
    /// container-info frame has been queued to the attach writer.
    pending_container_info_lookup: Option<ContainerInfoLookupRequest>,
    /// Workspace workdir read from `JACKIN_WORKDIR` at daemon startup.
    /// Every spawned PTY (agent or shell) receives this as its `cwd`
    /// so the operator's panes open in the workspace they configured
    /// instead of `$HOME` (portable_pty's CommandBuilder default).
    workdir: PathBuf,
}

struct ContainerInfoLookupRequest {
    request_id: u64,
    workdir: PathBuf,
}

#[allow(dead_code)]
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
    ConfirmOverlay,
    FocusChange,
    PaneChromeChanged,
    ThemeStyleChange,
    SessionExit,
    PaneClear,
    ExplicitRedraw,
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
            Self::ConfirmOverlay => "confirm-overlay",
            Self::FocusChange => "focus-change",
            Self::PaneChromeChanged => "pane-chrome-changed",
            Self::ThemeStyleChange => "theme-style-change",
            Self::SessionExit => "session-exit",
            Self::PaneClear => "pane-clear",
            Self::ExplicitRedraw => "explicit-redraw",
            Self::PaneCacheMiss => "pane-cache-miss",
            Self::UnsafePartial => "unsafe-partial",
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

const CONTAINER_INFO_COPY_FEEDBACK_DURATION: std::time::Duration =
    std::time::Duration::from_secs(2);

impl Multiplexer {
    pub fn new(rows: u16, cols: u16, workdir: PathBuf) -> Self {
        let (rows, cols) = normalize_size(rows, cols);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let content_rows = rows.saturating_sub(STATUS_BAR_ROWS);
        let agents = available_agents();

        let env_passthrough: Vec<(String, String)> = SESSION_ENV_PASSTHROUGH
            .iter()
            .filter_map(|&k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
            .collect();

        let input_parser = InputParser::default();
        let mut status_bar = StatusBar::new();
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
            pointer_shapes_supported: pointer_shapes_supported_from_env(),
            container_info_copy_deadline: None,
            container_info_request_id: 0,
            pending_container_info_lookup: None,
            workdir,
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
        if row_1based == 1
            && (self.status_bar.tab_at_col(col_1based).is_some()
                || self.status_bar.hint_at(row_1based, col_1based))
        {
            return PointerShape::Pointer;
        }
        if row_1based == 2 && self.status_bar.identity_at(row_1based, col_1based) {
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
        if matches!(d, Dialog::ContainerInfo { .. }) {
            self.container_info_copy_deadline = None;
        }
        self.dialog_stack.push(d);
    }

    fn open_container_info_dialog(&mut self) {
        let focused_agent = self
            .active_focused_id()
            .and_then(|id| self.sessions.get(&id))
            .and_then(|s| s.agent.clone());
        let container_name = self.status_bar.container_name().to_string();
        self.container_info_request_id = self.container_info_request_id.wrapping_add(1);
        let request_id = self.container_info_request_id;
        self.dialog_push(Dialog::ContainerInfo {
            container_name,
            role: self.status_bar.role().to_string(),
            focused_agent,
            workdir: self.workdir.to_string_lossy().into_owned(),
            git_loading: true,
            git_branch: None,
            pull_request_loading: true,
            pull_request_url: None,
            copied: false,
        });
        self.pending_container_info_lookup = Some(ContainerInfoLookupRequest {
            request_id,
            workdir: self.workdir.clone(),
        });
    }

    fn spawn_pending_container_info_lookup(&mut self) {
        let Some(request) = self.pending_container_info_lookup.take() else {
            return;
        };
        let event_tx = self.event_tx.clone();
        // Resolve on every open. The operator may switch branches from
        // any pane while the container stays alive, so branch / PR
        // metadata must never be cached on the Multiplexer. The git
        // and gh commands can touch disk and network, so they run only
        // after the initial dialog frame has been queued to the client.
        std::thread::spawn(move || {
            let branch = git_current_branch(&request.workdir);
            if event_tx
                .send(SessionEvent::ContainerInfoBranchLoaded {
                    request_id: request.request_id,
                    branch: branch.clone(),
                })
                .is_err()
            {
                crate::clog!(
                    "container-info: event channel closed before branch reached the main loop"
                );
                return;
            }
            let pull_request_url = branch
                .as_deref()
                .and_then(|branch| gh_pull_request_url(&request.workdir, branch));
            if event_tx
                .send(SessionEvent::ContainerInfoPullRequestLoaded {
                    request_id: request.request_id,
                    pull_request_url,
                })
                .is_err()
            {
                crate::clog!(
                    "container-info: event channel closed before pull-request URL reached the main loop"
                );
            }
        });
    }

    fn apply_container_info_branch_loaded(
        &mut self,
        request_id: u64,
        branch: Option<String>,
    ) -> bool {
        if request_id != self.container_info_request_id {
            return false;
        }
        let Some(Dialog::ContainerInfo {
            git_loading,
            git_branch,
            ..
        }) = self.dialog_top_mut()
        else {
            return false;
        };
        *git_loading = false;
        *git_branch = branch;
        true
    }

    fn apply_container_info_pull_request_loaded(
        &mut self,
        request_id: u64,
        pull_request_url: Option<String>,
    ) -> bool {
        if request_id != self.container_info_request_id {
            return false;
        }
        let Some(Dialog::ContainerInfo {
            pull_request_loading,
            pull_request_url: current_pull_request_url,
            ..
        }) = self.dialog_top_mut()
        else {
            return false;
        };
        *pull_request_loading = false;
        *current_pull_request_url = pull_request_url;
        true
    }

    /// Pop the top dialog. Returns `Some(prev)` when something was on
    /// the stack. The Esc handler uses this for back-navigation:
    /// popping a sub-dialog exposes its parent again rather than
    /// dismissing the whole flow.
    fn dialog_pop_one(&mut self) -> Option<Dialog> {
        let popped = self.dialog_stack.pop();
        if !matches!(
            self.dialog_stack.last(),
            Some(Dialog::ContainerInfo { copied: true, .. })
        ) {
            self.container_info_copy_deadline = None;
        }
        popped
    }

    /// Clear every dialog on the stack — used by action paths that
    /// finish the flow (`SpawnAgent` after picking an agent,
    /// destructive confirmations after they fire, etc.) so the
    /// operator returns straight to the focused pane.
    fn dialog_clear(&mut self) {
        self.dialog_stack.clear();
        self.container_info_copy_deadline = None;
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
        self.pending_container_info_lookup = None;
        self.container_info_copy_deadline = None;
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
        // cheaper than per-field re-validation and matches the
        // close_focused_pane / split path that already does the same.
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

    pub fn spawn_initial(&mut self, agent: &str) -> Result<u64> {
        self.spawn_session(Some(agent.to_string()), &[])
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
                // The ContainerInfo dialog stays on the stack — the
                // operator's "did it actually copy?" question is
                // answered by the green "✓ Copied!" badge the
                // renderer paints on the Container ID row now that
                // `copied = true` (flipped by the dialog's handle_key
                // or row-click handler before this action returned).
                // The badge expires from the daemon's tick loop.
                self.send_output(encode_osc52_clipboard_write(&payload));
                self.container_info_copy_deadline =
                    Some(Instant::now() + CONTAINER_INFO_COPY_FEEDBACK_DURATION);
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
        let agent_label = agent.as_deref().unwrap_or("shell").to_string();
        let result: anyhow::Result<()> = match intent {
            PickerIntent::NewTab => self.spawn_session(agent, &[]).map(|_| ()),
            PickerIntent::Split(direction) => self.split_focused_into(direction, agent, &[]),
        };
        if let Err(err) = result {
            crate::clog!("spawn ({intent:?}, agent={agent_label}) failed: {err:?}");
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
        let prev_focused = self.active_focused_id();
        let env_passthrough = self.env_for_spawn(env_overrides);
        let cwd = self.workdir.as_path();
        let (label, cmd) = match &agent {
            Some(slug) => (
                capitalize(slug),
                build_agent_command(slug, &env_passthrough, cwd),
            ),
            None => (
                "Shell".to_string(),
                build_shell_command(&env_passthrough, cwd),
            ),
        };
        let (session, id) = Session::spawn(
            &label,
            agent.clone(),
            cmd,
            self.content_rows.saturating_sub(2),
            self.term_cols.saturating_sub(2),
            self.event_tx.clone(),
        )?;
        let tab_label = label.clone();
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
            "action: spawn_session id={id} agent={:?} label={label} tab_idx={}",
            agent,
            self.active_tab
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
        let cwd = self.workdir.as_path();
        let (label, cmd) = match &agent_slug {
            Some(slug) => (
                capitalize(slug),
                build_agent_command(slug, &env_passthrough, cwd),
            ),
            None => (
                "Shell".to_string(),
                build_shell_command(&env_passthrough, cwd),
            ),
        };
        let agent_for_log = agent_slug.clone();
        let (session, new_id) = Session::spawn(
            &label,
            agent_slug,
            cmd,
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
        );
        Ok(())
    }

    /// Split the focused pane and clone the source pane's agent into
    /// the new pane. Kept for the tmux-style `Ctrl+B %` / `Ctrl+B "`
    /// prefix bindings, which spawn-and-go without an agent picker.
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
        // Mirror remove_exited_session: drop the zoomed reference when
        // the killed pane was the zoom target. Otherwise the next
        // compose_frame's `if let Some(zoom_id) = self.zoomed` branch
        // calls sessions.get_mut(&zoom_id) → None and the operator
        // sees a blank zoom area until they manually unzoom.
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
        self.content_rows = rows.saturating_sub(STATUS_BAR_ROWS);
        self.resize_panes();
    }

    fn active_focused_id(&self) -> Option<u64> {
        self.tabs.get(self.active_tab).map(|t| t.focused_id)
    }

    /// `self.zoomed` narrowed to "only when the zoomed session belongs
    /// to the active tab." The zoom field is global (one value across
    /// all tabs), but render / input / scroll / mouse paths must
    /// behave as if zoom is per-tab — switching tabs has to surface
    /// the new tab's panes normally even when a different tab still
    /// has a zoomed session pinned, or the operator opens a new tab
    /// and only sees the previously-zoomed pane painted full-screen
    /// (the regression operators reported as "I selected Shell but I
    /// still see Claude"). Returning `None` from the active-tab check
    /// routes every consumer of zoom state through the normal
    /// multi-pane path for tabs that don't hold the zoom.
    fn active_zoomed_id(&self) -> Option<u64> {
        let zoom_id = self.zoomed?;
        let tab = self.tabs.get(self.active_tab)?;
        if tab.tree.all_ids().contains(&zoom_id) {
            Some(zoom_id)
        } else {
            None
        }
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

    fn expire_container_info_copy_feedback(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.container_info_copy_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.container_info_copy_deadline = None;
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
    fn synthesise_focus_swap(&self, old: Option<u64>, new: Option<u64>) {
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
            if let Some(tx) = &self.attached_out {
                let mut stage = "focus_swap_reset";
                let mut failed = false;
                if tx
                    .send(encode_server(ServerFrame::Output(
                        crate::session::Session::focus_swap_reset().to_vec(),
                    )))
                    .is_err()
                {
                    failed = true;
                } else {
                    stage = "client_owned_mode_state";
                    if tx
                        .send(encode_server(ServerFrame::Output(
                            crate::session::Session::client_owned_mode_state().to_vec(),
                        )))
                        .is_err()
                    {
                        failed = true;
                    } else {
                        for bytes in s.current_mode_state() {
                            stage = "current_mode_state";
                            if tx.send(encode_server(ServerFrame::Output(bytes))).is_err() {
                                failed = true;
                                break;
                            }
                        }
                    }
                }
                if failed {
                    crate::clog!(
                        "focus swap: client receiver dropped during {stage}; outer terminal mode-state is partial"
                    );
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
                // outside-the-box and immediately dismisses the
                // dialog, which is exactly the regression operators
                // reported as "the dialog disappears when I click on
                // it."
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
            InputEvent::MousePress { button, .. } if is_wheel_button(button) => {
                // SGR mouse wheel: bits 6/7 indicate wheel events, with
                // low bits selecting direction (even = up, odd = down)
                // and modifier flags possibly OR'd in (shift = +4, alt
                // = +8, ctrl = +16). Buttons 64–95 cover every wheel
                // variant — never forward any of them to the PTY,
                // because shells and pre-mount agents never asked for
                // mouse mode and the SGR bytes would surface as
                // garbage text at the prompt. Dialog overlay swallows
                // the wheel too so background pane scrollback does
                // not move while the operator is interacting with
                // the modal.
                if self.dialog_open() {
                    return None;
                }
                let delta = if (button & 1) == 0 { 3 } else { -3 };
                if let Some(focused) = self.active_focused_id()
                    && let Some(session) = self.sessions.get_mut(&focused)
                {
                    session.scroll_by(delta);
                }
                Some(self.compose_full_frame(FullRedrawReason::ScrollbackMovement))
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
                // 2) Click on the right-side hint acts as a
                //    palette-key gesture — gives the operator a
                //    mouse fallback when the keyboard shortcut
                //    isn't reaching the parser.
                if self.status_bar.hint_at(1, col + 1) {
                    if self.dialog_open() {
                        self.dialog_clear();
                    } else {
                        self.open_command_palette();
                    }
                    return Some(self.compose_full_frame(FullRedrawReason::PaletteOverlay));
                }
                None
            }
            InputEvent::MousePress {
                row: 1,
                col,
                button: 0,
            } => {
                // Click on the right-side container-name label opens
                // the read-only `ContainerInfo` modal. Copying is an
                // explicit second action on the Container ID row.
                // Clicks elsewhere on row 1 (the underline strip) are
                // no-ops.
                if self.status_bar.identity_at(2, col + 1) {
                    self.open_container_info_dialog();
                    return Some(self.compose_dialog_overlay_frame(FullRedrawReason::DialogChange));
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

    fn forward_mouse_to_focused_pane(&mut self, col: u16, row: u16, button: u8) {
        self.forward_mouse_to_focused_pane_with_kind(col, row, button, true);
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
    ) {
        let Some(focused) = self.active_focused_id() else {
            return;
        };
        let Some(session) = self.sessions.get(&focused) else {
            return;
        };
        let mouse_mode = session.mouse_protocol_mode();
        if !mouse_event_allowed_for_mode(mouse_mode, button, press) {
            return;
        }
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let outer = if let Some(zoom_id) = self.active_zoomed_id() {
            if zoom_id == focused {
                Some(content_rect)
            } else {
                None
            }
        } else {
            self.tabs
                .get(self.active_tab)
                .and_then(|tab| {
                    tab.tree
                        .leaves(content_rect)
                        .into_iter()
                        .find(|(id, _)| *id == focused)
                })
                .map(|(_, r)| r)
        };
        let Some(outer) = outer else {
            return;
        };
        let inner = outer.shrink(1);
        if row < inner.row || row >= inner.row + inner.rows {
            return;
        }
        if col < inner.col || col >= inner.col + inner.cols {
            return;
        }
        let local_row = row - inner.row;
        let local_col = col - inner.col;
        let Some(buf) = encode_mouse_for_protocol(
            button,
            local_col + 1,
            local_row + 1,
            press,
            session.mouse_protocol_encoding(),
        ) else {
            return;
        };
        session.send_input(&buf);
    }

    /// Test whether the click at `(row, col)` lands on a shared pane
    /// border in the active tab. Returns a populated `DragState` to
    /// start a mouse-drag resize.
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
            if !text.is_empty()
                && let Some(tx) = &self.attached_out
            {
                let bytes = encode_osc52_clipboard_write(&text);
                if tx.send(encode_server(ServerFrame::Output(bytes))).is_err() {
                    crate::clog!(
                        "OSC52 clipboard write: client receiver dropped; copy did not reach outer terminal"
                    );
                }
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
    fn focus_pane_at(&mut self, row: u16, col: u16) -> bool {
        if row < STATUS_BAR_ROWS {
            return false;
        }
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return false;
        };
        let prev = tab.focused_id;
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

    fn compose_full_frame(&mut self, reason: FullRedrawReason) -> Vec<u8> {
        let started = Instant::now();
        let mut buf = Vec::with_capacity(65536);
        buf.extend_from_slice(b"\x1b[?25l");

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
        );

        let focused_id = self.active_focused_id();
        let mut focused_pane_rect: Option<Rect> = None;
        let panes = self.visible_panes();
        let multi_pane = panes.len() > 1;
        let zoomed = self.active_zoomed_id().is_some();
        let mut pane_rows_emitted = 0usize;
        let mut pane_body_bytes = 0usize;

        for pane in &panes {
            let mut filled_for_scrollbar = 0usize;
            let mut offset_for_scrollbar = 0usize;
            let mut title = None;
            if let Some(session) = self.sessions.get_mut(&pane.id) {
                offset_for_scrollbar = session.scrollback_offset;
                filled_for_scrollbar = session.scrollback_filled();
                title = Some(display_title(session));
                let before = buf.len();
                let stats = self
                    .pane_body_caches
                    .entry(pane.id)
                    .or_default()
                    .render_full(
                        session.screen(),
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
                    filled_for_scrollbar > 0
                } else {
                    multi_pane || filled_for_scrollbar > 0
                };
                draw_pane_box(
                    &mut buf,
                    pane.outer.row,
                    pane.outer.col,
                    pane.outer.rows,
                    pane.outer.cols,
                    &title,
                    pane.focused && highlight_focus,
                );
                draw_scrollbar(
                    &mut buf,
                    pane.outer.row,
                    pane.outer.col,
                    pane.outer.rows,
                    pane.outer.cols,
                    offset_for_scrollbar,
                    filled_for_scrollbar,
                    pane.focused && highlight_focus,
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

        if let Some(dialog) = self.dialog_top() {
            dialog.render(&mut buf, self.term_rows, self.term_cols);
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

    fn compose_dialog_overlay_frame(&self, reason: FullRedrawReason) -> Vec<u8> {
        let started = Instant::now();
        let mut buf = Vec::with_capacity(8192);
        buf.extend_from_slice(b"\x1b[?25l");

        if let Some(dialog) = self.dialog_top() {
            dialog.render(&mut buf, self.term_rows, self.term_cols);
        }

        crate::cdebug!(
            "render: kind=dialog-overlay reason={} bytes={} duration_us={}",
            reason.as_str(),
            buf.len(),
            started.elapsed().as_micros()
        );

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
        buf.extend_from_slice(b"\x1b[?25l");
        let mut rows_emitted = 0usize;
        let mut panes_rendered = 0usize;
        let mut pane_body_bytes = 0usize;
        let multi_pane = panes.len() > 1;
        let zoomed = self.active_zoomed_id().is_some();
        for pane in panes.iter().filter(|pane| dirty_panes.contains(&pane.id)) {
            let mut filled_for_scrollbar = 0usize;
            let mut offset_for_scrollbar = 0usize;
            let mut title = None;
            if let Some(session) = self.sessions.get_mut(&pane.id) {
                offset_for_scrollbar = session.scrollback_offset;
                filled_for_scrollbar = session.scrollback_filled();
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
                    filled_for_scrollbar > 0
                } else {
                    multi_pane || filled_for_scrollbar > 0
                };
                draw_pane_box(
                    &mut buf,
                    pane.outer.row,
                    pane.outer.col,
                    pane.outer.rows,
                    pane.outer.cols,
                    &title,
                    pane.focused && highlight_focus,
                );
                draw_scrollbar(
                    &mut buf,
                    pane.outer.row,
                    pane.outer.col,
                    pane.outer.rows,
                    pane.outer.cols,
                    offset_for_scrollbar,
                    filled_for_scrollbar,
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
pub async fn run_daemon(initial_agent: String) -> Result<()> {
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

    // JACKIN_WORKDIR is the explicit contract between the host launcher
    // and the daemon for "where panes open". Missing/empty means the
    // host code path forgot to forward it — a deployment bug, not a
    // user error — so bail loudly instead of silently degrading to
    // portable_pty's `$HOME` default.
    let workdir = std::env::var("JACKIN_WORKDIR")
        .map_err(|_| anyhow::anyhow!("JACKIN_WORKDIR is not set; the host launcher must export it"))
        .and_then(|v| {
            if v.trim().is_empty() {
                Err(anyhow::anyhow!("JACKIN_WORKDIR is set but empty"))
            } else {
                Ok(PathBuf::from(v))
            }
        })?;

    // Initialise the file logger before anything else can emit a
    // diagnostic. Failures fall back to stderr-only, so this is safe
    // to call unconditionally.
    crate::logging::init();
    crate::clog!(
        "daemon start: rows={rows} cols={cols} initial_agent={initial_agent:?} workdir={}",
        workdir.display()
    );

    let mut mux = Multiplexer::new(rows, cols, workdir);
    // Spawn the first tab. Treat any spawn error as fatal at boot —
    // it usually means the entrypoint binary is missing from the
    // derived image, and silently degrading to an empty multiplexer
    // would hide the real problem behind a blank screen.
    if !initial_agent.is_empty() {
        if let Err(err) = mux.spawn_initial(&initial_agent) {
            crate::clog!("initial agent spawn failed (agent={initial_agent:?}): {err:?}");
            return Err(err);
        }
    } else if let Err(err) = mux.spawn_session(None, &[]) {
        crate::clog!("initial shell spawn failed: {err:?}");
        return Err(err);
    }

    let mut new_clients = socket::start_listener()?;
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

    // Resolve the operator's escape-time once at startup. Reading
    // the env var inside the event loop was a per-iteration syscall
    // for a value that never changes for the lifetime of the daemon.
    // A present-but-unparseable env var emits a debug line so the
    // operator can see their config was rejected rather than
    // silently falling back to the default.
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
    // Symptom was "press Esc, dialog never dismisses while an agent
    // is producing output."
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
                detach_client(&mut mux);
                return Ok(());
            }
            _ = sigint.recv() => {
                detach_client(&mut mux);
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
                    focus_session,
                    client_permit,
                } = ready;
                mux.resize(rows, cols);
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
                match spawn {
                    Some(SpawnRequest::Agent(agent_slug)) => {
                        // Re-validate the wire-decoded slug. The CLI argv
                        // path validates via `validate_agent_slug`, but the
                        // attach protocol carries a raw String — a peer
                        // that wins the socket race could otherwise inject
                        // an unallowlisted agent name (or a control byte)
                        // straight into `build_agent_command`.
                        match crate::session::validate_agent_slug(&agent_slug) {
                            Ok(_) => {
                                if let Err(err) =
                                    mux.spawn_session(Some(agent_slug.clone()), &env)
                                {
                                    crate::clog!(
                                        "attach: spawn_session for {agent_slug:?} failed: {err:#}"
                                    );
                                    spawn_failure = Some(format!(
                                        "spawn agent {agent_slug:?} failed: {err:#}"
                                    ));
                                }
                            }
                            Err(reason) => {
                                crate::clog!(
                                    "attach: rejected Hello.spawn.Agent {agent_slug:?}: {reason}"
                                );
                                spawn_failure =
                                    Some(format!("rejected agent {agent_slug:?}: {reason}"));
                            }
                        }
                    }
                    Some(SpawnRequest::Shell) => {
                        if let Err(err) = mux.spawn_session(None, &env) {
                            crate::clog!("attach: spawn_session (shell) failed: {err:#}");
                            spawn_failure = Some(format!("spawn shell failed: {err:#}"));
                        }
                    }
                    None => {}
                }
                // Take over from any existing attach client. Send the
                // Shutdown frame BEFORE aborting the reader task —
                // `abort()` drops the task's `out_rx` receiver, which
                // makes the subsequent `tx.send(Shutdown)` return Err
                // and the bytes never leave the socket. Yield once
                // afterwards so the writer side has a chance to drain
                // before the task is cancelled. Then `detach_client`
                // takes care of the abort + per-field bookkeeping.
                if let Some(tx) = mux.attached_out.take()
                    && tx.send(encode_server(ServerFrame::Shutdown)).is_err()
                {
                    crate::clog!(
                        "takeover: prior client receiver already dropped; Shutdown frame not delivered"
                    );
                }
                // A single `yield_now()` does not guarantee the writer
                // task drained `out_rx` and finished `write_all` to the
                // socket before `abort()` cancels it. Sleep briefly so
                // the buffered Shutdown bytes reach the kernel and the
                // old client's terminal sees a clean detach signal.
                tokio::time::sleep(Duration::from_millis(50)).await;
                if let Some(handle) = mux.attached_task.take() {
                    handle.abort();
                }
                // Drain any stale frames the old client task pushed
                // into cmd_tx before its abort actually took effect —
                // without this drain, the next `cmd_rx.recv()` after
                // the new attach is wired processes Input / Resize /
                // Detach against the NEW mux state. Inline drain via
                // try_recv keeps the takeover path single-threaded.
                // On a first-attach (no prior task) cmd_rx is already
                // empty so the loop exits on the first iteration.
                while cmd_rx.try_recv().is_ok() {}
                let (new_out_tx, new_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
                mux.attached_out = Some(new_out_tx.clone());
                mux.attached_out_dead_logged = false;
                let welcome = encode_server(ServerFrame::Welcome {
                    session_count: mux.sessions.len() as u32,
                });
                let _ = new_out_tx.send(welcome);
                // Initial mode-state restore: send the focused
                // session's current modes (bracketed paste, etc.) after
                // reasserting the attach-client-owned mouse/focus modes.
                // Without this, a re-attach loses bracketed-paste
                // and the operator's clipboard arrives unwrapped.
                let _ = new_out_tx.send(encode_server(ServerFrame::Output(
                    Session::client_owned_mode_state().to_vec(),
                )));
                if let Some(focused) = mux.active_focused_id()
                    && let Some(session) = mux.sessions.get(&focused)
                {
                    for bytes in session.current_mode_state() {
                        let _ = new_out_tx.send(encode_server(ServerFrame::Output(bytes)));
                    }
                }
                let mut initial = b"\x1b[2J".to_vec();
                initial.extend(mux.compose_full_frame(FullRedrawReason::FirstAttach));
                let _ = new_out_tx.send(encode_server(ServerFrame::Output(initial)));
                if let Some(reason) = spawn_failure {
                    let _ = new_out_tx.send(encode_server(ServerFrame::Output(
                        spawn_failure_banner(&reason),
                    )));
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
                    detach_client(&mut mux);
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
                        // `mux.send_output` (which needs &Multiplexer).
                        let mut to_emit: Vec<Vec<u8>> = Vec::new();
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
                                to_emit.extend(drained);
                                to_emit.extend(mode_transitions);
                            }
                        }
                        for bytes in to_emit {
                            mux.send_output(bytes);
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
                    SessionEvent::ContainerInfoBranchLoaded {
                        request_id,
                        branch,
                    } => {
                        if mux.apply_container_info_branch_loaded(request_id, branch) {
                            let frame =
                                mux.compose_dialog_overlay_frame(FullRedrawReason::DialogChange);
                            mux.send_output(frame);
                        }
                    }
                    SessionEvent::ContainerInfoPullRequestLoaded {
                        request_id,
                        pull_request_url,
                    } => {
                        if mux.apply_container_info_pull_request_loaded(
                            request_id,
                            pull_request_url,
                        ) {
                            let frame =
                                mux.compose_dialog_overlay_frame(FullRedrawReason::DialogChange);
                            mux.send_output(frame);
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
                        mux.spawn_pending_container_info_lookup();
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
                for session in mux.sessions.values_mut() {
                    session.refresh_state();
                }
                if mux.expire_container_info_copy_feedback(Instant::now()) {
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
                mux.status_bar.render(&mut sbuf, mux.term_cols, &mux.tabs, mux.active_tab, &states);
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
                    mux.spawn_pending_container_info_lookup();
                }
            }
            // Reflect prefix-await state in the status bar so the right
            // hint switches between `detach: …` and `prefix…`.
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
            // (`?1004h`). Without the gate, primary-screen shells
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

/// Send `Shutdown` to the attached client and pause briefly so the
/// frame actually leaves the socket before PID 1 exits. Called when
/// the daemon decides to tear the container down (last session died,
/// last pane killed, or SIGTERM arrived).
/// A validated attach handshake produced by `perform_handshake`. The
/// main loop applies these — `client_permit` is kept alive until the
/// spawned persistent attach task drops it.
struct AttachHandshake {
    stream: UnixStream,
    rows: u16,
    cols: u16,
    spawn: Option<SpawnRequest>,
    env: Vec<(String, String)>,
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
    let mut first = [0u8; 1];
    if let Err(e) = stream.read_exact(&mut first).await {
        crate::clog!("attach: handshake read_exact(first byte) failed: {e}");
        drop(client_permit);
        return;
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
    let initial_frame = match read_client_frame(&mut stream, first[0]).await {
        Ok(Some(frame)) => frame,
        Ok(None) => {
            crate::clog!("attach: handshake EOF before initial frame");
            drop(client_permit);
            return;
        }
        Err(e) => {
            crate::clog!("attach: handshake frame decode failed: {e}");
            drop(client_permit);
            return;
        }
    };
    let ClientFrame::Hello {
        rows,
        cols,
        spawn,
        env,
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
        focus_session,
        client_permit,
    };
    if handshake_tx.send(handshake).is_err() {
        crate::clog!("attach: handshake channel closed; daemon shutting down");
    }
}

async fn drain_and_exit(mux: &mut Multiplexer) {
    detach_client(mux);
    tokio::time::sleep(Duration::from_millis(200)).await;
}

/// Centralised detach for the currently-attached client. Mirrors the
/// pairing the takeover path uses: take the out-channel sender (so
/// the next frame queue allocation does not race with the old
/// receiver), send Shutdown best-effort, yield once so any buffered
/// writer cycle drains, then abort the attach task so its reader
/// stops pushing into the shared `cmd_tx`. Used by SIGTERM / SIGINT
/// shutdown, explicit detach, and `drain_and_exit` so the
/// `attached_task` field cannot drift Some(handle) without a
/// corresponding live attach.
fn detach_client(mux: &mut Multiplexer) {
    if let Some(tx) = mux.attached_out.take()
        && tx.send(encode_server(ServerFrame::Shutdown)).is_err()
    {
        crate::clog!(
            "detach_client: client receiver already dropped; Shutdown frame not delivered"
        );
    }
    if let Some(handle) = mux.attached_task.take() {
        handle.abort();
    }
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

const GIT_CONTEXT_COMMAND_TIMEOUT: Duration = Duration::from_millis(1500);
const GH_PULL_REQUEST_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);

fn git_current_branch(workdir: &Path) -> Option<String> {
    command_stdout_trimmed_with_timeout(
        Command::new("git")
            .arg("-C")
            .arg(workdir)
            .args(["branch", "--show-current"]),
        GIT_CONTEXT_COMMAND_TIMEOUT,
    )
}

fn gh_pull_request_url(workdir: &Path, branch: &str) -> Option<String> {
    let url = command_stdout_trimmed_with_timeout(
        Command::new("gh")
            .current_dir(workdir)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .args([
                "pr", "list", "--head", branch, "--state", "open", "--limit", "1", "--json", "url",
                "--jq", ".[0].url",
            ]),
        GH_PULL_REQUEST_COMMAND_TIMEOUT,
    )?;
    if url == "null" || !(url.starts_with("https://") || url.starts_with("http://")) {
        None
    } else {
        Some(url)
    }
}

#[cfg(test)]
fn command_stdout_trimmed(command: &mut Command) -> Option<String> {
    command_stdout_trimmed_with_timeout(command, GIT_CONTEXT_COMMAND_TIMEOUT)
}

fn command_stdout_trimmed_with_timeout(command: &mut Command, timeout: Duration) -> Option<String> {
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
    let mut status_success = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                status_success = Some(status.success());
                break;
            }
            Ok(None) => {}
            // PID 1's zombie reaper can win the wait race for very
            // short-lived git/gh children. The stdout pipe still
            // carries the useful data, so treat ECHILD as "exited".
            Err(_) => break,
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

/// SGR mouse wheel events set bit 6 of the button byte. Every value in
/// `64..=95` is a wheel event with some combination of modifier flags
/// (shift = +4, alt = +8, ctrl = +16). Forwarding any of them to an
/// agent or shell that did not request mouse mode dumps the raw SGR
/// bytes at the prompt — so the multiplexer always intercepts the
/// wheel for scrollback regardless of modifiers.
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

fn pointer_shapes_supported_from_env() -> bool {
    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let term_program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    term.contains("ghostty")
        || term.contains("kitty")
        || term.contains("foot")
        || term_program.contains("ghostty")
        || term_program.contains("kitty")
        || term_program.contains("iterm")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        Multiplexer::new(rows, cols, PathBuf::from("/workspace"))
    }

    fn single_pane_tab_mux() -> Multiplexer {
        let mut mux = test_mux(24, 80);
        mux.tabs.push(Tab::new_single("Shell", 1));
        mux
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
        mux.status_bar.identity_region = Some((70, 80));
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);

        mux.update_pointer_shape_for_mouse(1, 69, SGR_NO_BUTTON_MOTION);
        let first = rx.try_recv().expect("first pointer-shape update");
        assert!(first.ends_with(b"\x1b]22;pointer\x1b\\"));

        mux.update_pointer_shape_for_mouse(1, 70, SGR_NO_BUTTON_MOTION);
        assert!(rx.try_recv().is_err(), "unchanged shape should not re-emit");
    }

    #[test]
    fn status_identity_click_opens_container_info_without_copying() {
        let mut mux = test_mux(24, 80);
        mux.pointer_shapes_supported = false;
        mux.status_bar.identity_label = "jk-test-container".to_string();
        mux.status_bar.role = "the-architect".to_string();
        mux.status_bar.identity_region = Some((60, 78));
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);

        let frame = mux
            .handle_input(InputEvent::MousePress {
                row: 1,
                col: 59,
                button: 0,
            })
            .expect("identity click should redraw");

        assert!(
            rx.try_recv().is_err(),
            "opening container info must not send OSC 52"
        );
        assert!(
            mux.pending_container_info_lookup.is_some(),
            "git metadata lookup should wait until after the first dialog frame"
        );
        assert!(!String::from_utf8_lossy(&frame).contains("Copied!"));
        assert!(String::from_utf8_lossy(&frame).contains("loading"));
        let Some(Dialog::ContainerInfo {
            copied: false,
            workdir,
            git_loading,
            pull_request_loading,
            ..
        }) = mux.dialog_top()
        else {
            panic!("identity click should open container info")
        };
        assert_eq!(workdir, "/workspace");
        assert!(*git_loading);
        assert!(*pull_request_loading);
    }

    #[test]
    fn container_info_copy_feedback_expires() {
        let mut mux = test_mux(24, 80);
        mux.dialog_push(Dialog::ContainerInfo {
            container_name: "jk-test-container".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace".to_string(),
            git_loading: false,
            git_branch: Some("main".to_string()),
            pull_request_loading: false,
            pull_request_url: Some("https://github.com/jackin-project/jackin/pull/1".to_string()),
            copied: true,
        });
        let now = Instant::now();
        mux.container_info_copy_deadline = Some(now);

        assert!(mux.expire_container_info_copy_feedback(now));
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ContainerInfo { copied: false, .. })
        ));
    }

    #[test]
    fn container_info_loaded_updates_matching_open_dialog() {
        let mut mux = test_mux(24, 100);
        mux.container_info_request_id = 7;
        mux.dialog_push(Dialog::ContainerInfo {
            container_name: "jk-test-container".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace".to_string(),
            git_loading: true,
            git_branch: None,
            pull_request_loading: true,
            pull_request_url: None,
            copied: false,
        });

        let branch_applied =
            mux.apply_container_info_branch_loaded(7, Some("feature/container-info".to_string()));
        assert!(branch_applied);
        let Some(Dialog::ContainerInfo {
            git_loading,
            git_branch,
            pull_request_loading,
            pull_request_url,
            ..
        }) = mux.dialog_top()
        else {
            panic!("container info dialog should still be open")
        };
        assert!(!*git_loading);
        assert!(*pull_request_loading);
        assert_eq!(git_branch.as_deref(), Some("feature/container-info"));
        assert_eq!(pull_request_url, &None);

        assert!(mux.apply_container_info_pull_request_loaded(
            7,
            Some("https://github.com/jackin-project/jackin/pull/414".to_string()),
        ));

        let Some(Dialog::ContainerInfo {
            git_loading,
            git_branch,
            pull_request_loading,
            pull_request_url,
            ..
        }) = mux.dialog_top()
        else {
            panic!("container info dialog should still be open")
        };
        assert!(!*git_loading);
        assert!(!*pull_request_loading);
        assert_eq!(git_branch.as_deref(), Some("feature/container-info"));
        assert_eq!(
            pull_request_url.as_deref(),
            Some("https://github.com/jackin-project/jackin/pull/414")
        );
    }

    #[test]
    fn container_info_loaded_ignores_stale_request() {
        let mut mux = test_mux(24, 100);
        mux.container_info_request_id = 7;
        mux.dialog_push(Dialog::ContainerInfo {
            container_name: "jk-test-container".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace".to_string(),
            git_loading: true,
            git_branch: None,
            pull_request_loading: true,
            pull_request_url: None,
            copied: false,
        });

        let stale_branch_applied =
            mux.apply_container_info_branch_loaded(6, Some("stale".to_string()));
        assert!(!stale_branch_applied);
        assert!(!mux.apply_container_info_pull_request_loaded(
            6,
            Some("https://github.com/jackin-project/jackin/pull/1".to_string()),
        ));

        let Some(Dialog::ContainerInfo {
            git_loading,
            git_branch,
            pull_request_loading,
            pull_request_url,
            ..
        }) = mux.dialog_top()
        else {
            panic!("container info dialog should still be open")
        };
        assert!(*git_loading);
        assert!(*pull_request_loading);
        assert_eq!(git_branch, &None);
        assert_eq!(pull_request_url, &None);
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
            git_loading: false,
            git_branch: Some("main".to_string()),
            pull_request_loading: false,
            pull_request_url: Some("https://github.com/jackin-project/jackin/pull/1".to_string()),
            copied: false,
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);

        let frame = mux
            .handle_input(InputEvent::MousePress {
                row: 17,
                col: 18,
                button: 0,
            })
            .expect("container id click should redraw copy feedback");

        let osc52 = rx.try_recv().expect("copy should emit OSC 52");
        assert!(
            osc52
                .windows(b"\x1b]52;c;".len())
                .any(|w| w == b"\x1b]52;c;")
        );
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
        let mut command = Command::new("sh");
        command.args(["-c", "printf branch-name; exit 1"]);

        assert_eq!(command_stdout_trimmed(&mut command), None);
    }
}
