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
use std::collections::HashMap;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::dialog::{Dialog, DialogAction, PaletteCommand, PickerIntent, SplitDirection};
use crate::input::{ArrowDir, InputEvent, InputParser, PrefixCommand};
use crate::layout::{Direction, Rect, SplitOrient, SplitPosition, Tab};
use crate::protocol::attach::{ClientFrame, ServerFrame, encode_server, read_client_frame};
use crate::protocol::control::{AgentState, SessionInfo};
use crate::render::{draw_scrollbar, render_pane};
use crate::session::{
    Session, SessionEvent, available_agents, build_agent_command, build_shell_command,
};
use crate::socket;
use crate::statusbar::{STATUS_BAR_ROWS, StatusBar, draw_pane_box};

pub struct Multiplexer {
    sessions: HashMap<u64, Session>,
    tabs: Vec<Tab>,
    active_tab: usize,
    term_rows: u16,
    term_cols: u16,
    status_bar: StatusBar,
    dialog: Option<Dialog>,
    content_rows: u16,
    available_agents: Vec<String>,
    env_passthrough: Vec<(String, String)>,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    zoomed: Option<u64>,
    input_parser: InputParser,
    detach_requested: bool,
    attached_out: Option<mpsc::UnboundedSender<Vec<u8>>>,
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
    /// Set whenever a state change would require a redraw. The render
    /// ticker drains this at most once per frame so a chatty PTY does
    /// not push N full frames per second to the client. Cleared after
    /// `compose_frame` runs.
    dirty: bool,
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

impl Multiplexer {
    pub fn new(rows: u16, cols: u16) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let content_rows = rows.saturating_sub(STATUS_BAR_ROWS);
        let agents = available_agents();

        let env_passthrough: Vec<(String, String)> = [
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GH_TOKEN",
            "JACKIN_DEBUG",
            "JACKIN_GIT_COAUTHOR_TRAILER",
            "JACKIN_GIT_DCO",
        ]
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
            dialog: None,
            content_rows,
            available_agents: agents,
            env_passthrough,
            event_tx,
            event_rx,
            zoomed: None,
            input_parser,
            detach_requested: false,
            attached_out: None,
            attached_task: None,
            last_tab_click: None,
            drag: None,
            selection: None,
            dirty: false,
        }
    }

    fn send_to_client(&self, frame: ServerFrame) {
        if let Some(tx) = &self.attached_out {
            let _ = tx.send(encode_server(frame));
        }
    }

    fn send_output(&self, bytes: Vec<u8>) {
        self.send_to_client(ServerFrame::Output(bytes));
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
        let tab_ids = self.tabs[self.active_tab].tree.all_ids();
        crate::clog!(
            "action: close_focused_tab tab_idx={} pane_count={}",
            self.active_tab,
            tab_ids.len()
        );
        for id in tab_ids {
            self.sessions.remove(&id);
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        self.zoomed = None;
        self.resize_panes();
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
                let prev_focused = self.active_focused_id();
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
                    let new_focused = self.active_focused_id();
                    self.synthesise_focus_swap(prev_focused, new_focused);
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
        self.zoomed = self.zoomed.filter(|&id| id != session_id);
        self.resize_panes();
    }

    pub fn spawn_initial(&mut self, agent: &str) -> Result<u64> {
        self.spawn_session(Some(agent.to_string()))
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
                self.dialog = None;
            }
            DialogAction::Redraw | DialogAction::Consume => {}
            DialogAction::Command(cmd) => {
                // `handle_palette_command` owns the dialog state — it
                // closes the dialog by default and overwrites it when
                // the command opens a sub-dialog (e.g. NewTab → agent
                // picker).
                self.handle_palette_command(cmd);
            }
            DialogAction::SpawnAgent { agent, intent } => {
                self.dialog = None;
                self.dispatch_spawn_intent(agent, intent);
            }
            DialogAction::RenameTab { tab_idx, label } => {
                self.dialog = None;
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
                    tab.custom_label = if label.is_empty() { None } else { Some(label) };
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
                self.dialog = None;
                self.send_output(encode_osc52_clipboard_write(&payload));
            }
            DialogAction::SplitDirection(direction) => {
                // Chain to the agent picker carrying the direction —
                // the standard agent-pick flow finishes the spawn via
                // `PickerIntent::Split(direction)` in
                // `dispatch_spawn_intent`.
                let agents = self.available_agents.clone();
                self.dialog = Some(Dialog::new_agent_picker(
                    agents,
                    PickerIntent::Split(direction),
                ));
            }
        }
        self.compose_frame()
    }

    /// Single dispatch point for `DialogAction::SpawnAgent`. Spawn
    /// failures (PTY allocation, missing agent binary, cap hit) are
    /// clog'd with their intent and agent label so a `jackin load
    /// --debug` shows the cause; the dialog dismisses regardless so
    /// the operator can retry.
    fn dispatch_spawn_intent(&mut self, agent: Option<String>, intent: PickerIntent) {
        let agent_label = agent.as_deref().unwrap_or("shell").to_string();
        let result: anyhow::Result<()> = match intent {
            PickerIntent::NewTab => self.spawn_session(agent).map(|_| ()),
            PickerIntent::Split(direction) => self.split_focused_into(direction, agent),
        };
        if let Err(err) = result {
            crate::clog!("spawn ({intent:?}, agent={agent_label}) failed: {err:?}");
        }
    }

    fn spawn_session(&mut self, agent: Option<String>) -> Result<u64> {
        // Bound the per-container surface so a runaway client (or an
        // operator mis-click loop) cannot allocate unbounded PTYs.
        // Each session retains ~SCROLLBACK_LEN lines of scrollback,
        // a master+slave PTY pair, and a child process — at MAX_TABS
        // sessions the container memory footprint is still well
        // under typical limits, but well past the size any operator
        // can usefully navigate.
        self.ensure_capacity_for_new_session(true)?;
        let (label, cmd) = match &agent {
            Some(slug) => (
                capitalize(slug),
                build_agent_command(slug, &self.env_passthrough),
            ),
            None => ("Shell".to_string(), build_shell_command()),
        };
        let (session, id) = Session::spawn(
            &label,
            agent.clone(),
            cmd,
            self.content_rows,
            self.term_cols,
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
        crate::clog!(
            "action: spawn_session id={id} agent={:?} label={label} tab_idx={}",
            agent,
            self.active_tab
        );
        Ok(id)
    }

    /// Split the focused pane and spawn a session of the operator's
    /// choice inside it. `agent_slug = None` opens a shell. Used by
    /// the AgentPicker → Split flow so the operator picks the new
    /// pane's identity instead of cloning the source pane's agent.
    /// Bound the per-container surface for any path that allocates a
    /// new PTY (top-level spawn, split, etc.). `add_tab=true` enforces
    /// both `MAX_TABS` and `MAX_SESSIONS`; `add_tab=false` enforces
    /// only `MAX_SESSIONS` because the caller is reusing an existing
    /// tab. Split-driven creation was previously bypassing the cap —
    /// the runaway-mis-click scenario the cap exists to defend
    /// against.
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

    fn split_focused_into(
        &mut self,
        direction: SplitDirection,
        agent_slug: Option<String>,
    ) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        // Any selection / drag-resize is anchored to a specific pane
        // rect that this reflow is about to invalidate.
        self.cancel_drag();
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Ok(());
        };
        let from_id = tab.focused_id;
        let (label, cmd) = match &agent_slug {
            Some(slug) => (
                capitalize(slug),
                build_agent_command(slug, &self.env_passthrough),
            ),
            None => ("Shell".to_string(), build_shell_command()),
        };
        let agent_for_log = agent_slug.clone();
        let (session, new_id) = Session::spawn(
            &label,
            agent_slug,
            cmd,
            self.content_rows / 2,
            self.term_cols,
            self.event_tx.clone(),
        )?;
        self.sessions.insert(new_id, session);
        let tab = &mut self.tabs[self.active_tab];
        match direction {
            SplitDirection::Left => {
                tab.tree.split_h(from_id, new_id, SplitPosition::Before);
            }
            SplitDirection::Right => {
                tab.tree.split_h(from_id, new_id, SplitPosition::After);
            }
            SplitDirection::Above => {
                tab.tree.split_v(from_id, new_id, SplitPosition::Before);
            }
            SplitDirection::Below => {
                tab.tree.split_v(from_id, new_id, SplitPosition::After);
            }
        }
        tab.focused_id = new_id;
        self.resize_panes();
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
        self.split_focused_into(direction, agent_slug)
    }

    fn close_focused_pane(&mut self) {
        self.cancel_drag();
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
        self.sessions.remove(&id);
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

    /// Rewrite each tab's `label` based on the current pane contents.
    /// Cheap (clones a few short strings) and easier to reason about
    /// than dispatching incremental updates from every spawn / split
    /// / remove site.
    fn refresh_tab_labels(&mut self) {
        let mut new_labels = Vec::with_capacity(self.tabs.len());
        for tab in &self.tabs {
            // Operator-set custom labels take priority — the deriver
            // would otherwise overwrite the name the operator just
            // typed every time a pane is added or removed.
            new_labels.push(
                tab.custom_label
                    .clone()
                    .unwrap_or_else(|| self.tab_display_label(tab)),
            );
        }
        for (tab, label) in self.tabs.iter_mut().zip(new_labels) {
            tab.label = label;
        }
    }

    /// True when there are no sessions left.
    /// `sessions.is_empty()` covers the operator-explicitly-killed-all
    /// case; `all !alive` covers the natural-exit case (every agent /
    /// shell process closed its PTY).
    fn no_live_sessions(&self) -> bool {
        self.sessions.is_empty()
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
            // Without the reset, mouse / focus / kitty / bracketed-
            // paste from the previous focused pane would leak into
            // shells and pre-TUI agents.
            if let Some(tx) = &self.attached_out {
                let _ = tx.send(encode_server(ServerFrame::Output(
                    crate::session::Session::focus_swap_reset().to_vec(),
                )));
                for bytes in s.current_mode_state() {
                    let _ = tx.send(encode_server(ServerFrame::Output(bytes)));
                }
            }
        }
    }

    /// Handle a parsed input event from the client terminal.
    /// Returns bytes to send to the client (e.g. redraws), if any.
    fn handle_input(&mut self, event: InputEvent) -> Option<Vec<u8>> {
        match event {
            InputEvent::OpenPalette => {
                self.cancel_drag();
                if self.dialog.is_some() {
                    self.dialog = None;
                } else {
                    self.dialog = Some(Dialog::CommandPalette { selected: 0, filter: String::new() });
                }
                Some(self.compose_frame())
            }
            InputEvent::PrefixCommand(cmd) => {
                // While a dialog is open the prefix gesture's payload
                // must not reach the focused pane — operator's intent
                // is to act on the dialog, not the agent underneath.
                if self.dialog.is_some() {
                    return None;
                }
                self.handle_prefix_command(cmd)
            }
            InputEvent::ResizePane(dir) => {
                if self.dialog.is_some() {
                    return None;
                }
                self.resize_focused(dir);
                Some(self.compose_frame())
            }
            InputEvent::FocusIn | InputEvent::FocusOut => {
                // Forward only when the focused agent actually
                // requested focus events (`?1004h`) — shells and
                // pre-mount agents leave the mode off and would
                // surface `[I` / `[O` as literal text at the prompt.
                if self.dialog.is_some() {
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
                if self.dialog.is_some() && button == 0 && !is_wheel_button(button) =>
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
                    .dialog
                    .as_mut()
                    .expect("dialog presence checked")
                    .handle_click(row + 1, col + 1, term_rows, term_cols);
                Some(self.apply_dialog_action(action))
            }
            InputEvent::MousePress { .. } if self.dialog.is_some() => {
                // Any non-wheel mouse event with the dialog up that
                // did not land on a row is swallowed so it never
                // reaches the agent underneath.
                None
            }
            InputEvent::MouseRelease { .. } if self.dialog.is_some() => {
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
                    return Some(self.compose_frame());
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
                if self.dialog.is_some() {
                    return None;
                }
                let delta = if (button & 1) == 0 { 3 } else { -3 };
                if let Some(focused) = self.active_focused_id()
                    && let Some(session) = self.sessions.get_mut(&focused)
                {
                    session.scroll_by(delta);
                }
                Some(self.compose_frame())
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
                        let initial = self.tabs[idx].custom_label.clone().unwrap_or_default();
                        let input = jackin_tui::TextField::new(initial)
                            .with_max_chars(crate::dialog::MAX_CUSTOM_LABEL_LEN);
                        self.dialog = Some(Dialog::RenameTab {
                            tab_idx: idx,
                            input,
                        });
                        self.last_tab_click = None;
                        return Some(self.compose_frame());
                    }
                    self.last_tab_click = Some((idx, now));
                    if idx != self.active_tab {
                        self.cancel_drag();
                        let prev = self.active_focused_id();
                        self.active_tab = idx;
                        self.synthesise_focus_swap(prev, self.active_focused_id());
                        return Some(self.compose_frame());
                    }
                    return None;
                }
                // 2) Click on the right-side hint acts as a
                //    palette-key gesture — gives the operator a
                //    mouse fallback when the keyboard shortcut
                //    isn't reaching the parser.
                if self.status_bar.hint_at(1, col + 1) {
                    self.dialog = if self.dialog.is_some() {
                        None
                    } else {
                        Some(Dialog::CommandPalette { selected: 0, filter: String::new() })
                    };
                    return Some(self.compose_frame());
                }
                None
            }
            InputEvent::MousePress {
                row: 1,
                col,
                button: 0,
            } => {
                // Click on the right-side container-name label opens
                // the read-only `ContainerInfo` modal so the operator
                // can copy the container ID + see the role / focused
                // agent. Clicks elsewhere on row 1 (the underline
                // strip) are no-ops.
                if self.status_bar.identity_at(2, col + 1) {
                    let focused_agent = self
                        .active_focused_id()
                        .and_then(|id| self.sessions.get(&id))
                        .and_then(|s| s.agent.clone());
                    self.dialog = Some(Dialog::ContainerInfo {
                        container_name: self.status_bar.container_name().to_string(),
                        role: self.status_bar.role().to_string(),
                        focused_agent,
                        copied: false,
                    });
                    return Some(self.compose_frame());
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
                        return Some(self.compose_frame());
                    }
                    self.forward_mouse_to_focused_pane(col, row, button);
                    return if switched_focus {
                        Some(self.compose_frame())
                    } else {
                        None
                    };
                }
                self.forward_mouse_to_focused_pane(col, row, button);
                None
            }
            InputEvent::Data(bytes) => {
                if let Some(ref mut dialog) = self.dialog {
                    let action = dialog.handle_key(&bytes);
                    Some(self.apply_dialog_action(action))
                } else {
                    // Any keyboard input from the operator returns the
                    // focused pane to the live tail. Matches the
                    // common multiplexer convention that "I'm typing
                    // again" implies "show me what's happening now."
                    let mut snapped = false;
                    if let Some(focused) = self.active_focused_id()
                        && let Some(session) = self.sessions.get_mut(&focused)
                    {
                        if session.scrollback_offset != 0 {
                            session.scroll_to_live();
                            snapped = true;
                        }
                        session.send_input(&bytes);
                    }
                    if snapped {
                        Some(self.compose_frame())
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
        match cmd {
            PrefixCommand::NewTab => {
                let agents = self.available_agents.clone();
                self.dialog = Some(Dialog::new_agent_picker(agents, PickerIntent::NewTab));
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
            PrefixCommand::Detach => {
                self.detach_requested = true;
            }
            PrefixCommand::Palette => {
                self.dialog = Some(Dialog::CommandPalette { selected: 0, filter: String::new() });
            }
            PrefixCommand::Redraw => {}
        }
        Some(self.compose_frame())
    }

    fn forward_mouse_to_focused_pane(&mut self, col: u16, row: u16, button: u8) {
        self.forward_mouse_to_focused_pane_with_kind(col, row, button, true);
    }

    /// Re-encode an SGR mouse event in the focused pane's local
    /// coordinate space and forward to its PTY. `press = true` emits
    /// the `M` final, `false` emits `m` (release). Forwarding is
    /// gated by `session.mouse_enabled()` so shells and pre-mount
    /// agents never see raw mouse bytes leak out as command-line
    /// garbage.
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
        if !session.mouse_enabled() {
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
        let final_byte = if press { 'M' } else { 'm' };
        let buf = format!(
            "\x1b[<{};{};{}{}",
            button,
            local_col + 1,
            local_row + 1,
            final_byte
        );
        session.send_input(buf.as_bytes());
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
        Some(self.compose_frame())
    }

    /// Commit the active selection: extract the selected text from
    /// the source session's `vt100` grid, emit OSC 52 to the
    /// attached client (which the outer terminal turns into a
    /// real clipboard write), and clear the highlight.
    fn finalize_selection(&mut self) -> Option<Vec<u8>> {
        let sel = self.selection.take()?;
        if let Some(session) = self.sessions.get(&sel.session_id) {
            let text = selection_text(session.screen(), &sel);
            if !text.is_empty()
                && let Some(tx) = &self.attached_out
            {
                // OSC 52 with `c` selection (clipboard) and a
                // base64-encoded payload. The outer terminal
                // (Ghostty / iTerm2 / kitty / wezterm) writes the
                // decoded bytes to the system clipboard.
                let encoded = BASE64.encode(text.as_bytes());
                let bytes = format!("\x1b]52;c;{}\x07", encoded).into_bytes();
                let _ = tx.send(encode_server(ServerFrame::Output(bytes)));
            }
        }
        Some(self.compose_frame())
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
        Some(self.compose_frame())
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
        // Default: close the dialog after the command runs. Commands
        // that need a follow-up choice (e.g. NewTab → "which agent?")
        // overwrite `self.dialog` themselves AFTER this reset, so the
        // sub-dialog survives this handler.
        self.dialog = None;
        match cmd {
            PaletteCommand::Split => {
                // Open the SplitDirectionPicker sub-dialog. The
                // operator picks the direction; that resolves to a
                // `DialogAction::SplitDirection(...)` which
                // `apply_dialog_action` chains into an `AgentPicker`
                // carrying `PickerIntent::Split(direction)`. Final
                // confirm spawns the new pane.
                self.dialog = Some(Dialog::SplitDirectionPicker {
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
                self.dialog = Some(Dialog::new_agent_picker(agents, PickerIntent::NewTab));
            }
            PaletteCommand::NextTab => self.next_tab(),
            PaletteCommand::PrevTab => self.prev_tab(),
            PaletteCommand::ClosePane => self.close_focused_pane(),
            PaletteCommand::CloseTab => self.close_focused_tab(),
            PaletteCommand::ZoomPane => self.toggle_zoom(),
            PaletteCommand::Detach => {
                self.detach_requested = true;
            }
        }
        None
    }

    fn compose_frame(&mut self) -> Vec<u8> {
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

        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let focused_id = self.active_focused_id();
        let mut focused_pane_rect: Option<Rect> = None;

        // Dim the panes when a dialog is open so the operator gets an
        // unmistakable "focus is inside the dialog" cue.
        let dim_panes = self.dialog.is_some();

        if let Some(zoom_id) = self.active_zoomed_id() {
            let outer = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
            let inner = outer.shrink(1);
            let mut filled_for_scrollbar = 0usize;
            let mut offset_for_scrollbar = 0usize;
            if let Some(session) = self.sessions.get_mut(&zoom_id) {
                let offset = session.scrollback_offset;
                let filled = session.scrollback_filled();
                filled_for_scrollbar = filled;
                offset_for_scrollbar = offset;
                render_pane(
                    session.screen(),
                    inner.row,
                    inner.col,
                    inner.rows,
                    inner.cols,
                    dim_panes,
                    &mut buf,
                );
                if Some(zoom_id) == focused_id {
                    focused_pane_rect = Some(inner);
                }
            }
            if let Some(session) = self.sessions.get(&zoom_id) {
                let title = session.title().unwrap_or(session.label.as_str());
                // Zoom mode shows exactly one pane — same single-pane
                // gray treatment as the unzoomed single-pane case
                // unless the pane has scrollback to advertise.
                let highlight_focus = filled_for_scrollbar > 0;
                draw_pane_box(
                    &mut buf,
                    outer.row,
                    outer.col,
                    outer.rows,
                    outer.cols,
                    title,
                    Some(zoom_id) == focused_id && highlight_focus,
                );
                draw_scrollbar(
                    &mut buf,
                    outer.row,
                    outer.col,
                    outer.rows,
                    outer.cols,
                    offset_for_scrollbar,
                    filled_for_scrollbar,
                    Some(zoom_id) == focused_id && highlight_focus,
                );
            }
        } else if let Some(tab) = self.tabs.get(self.active_tab) {
            let leaves = tab.tree.leaves(content_rect);
            let multi_pane = leaves.len() > 1;
            for (id, rect) in &leaves {
                let pane_focused = Some(*id) == focused_id;
                // Always draw a pane box, even for the single-pane
                // case — matches zellij's "every pane is framed"
                // convention and gives the operator a reliable place
                // to read the live `OSC 2` title.
                let inner = rect.shrink(1);
                let mut filled_for_scrollbar = 0usize;
                let mut offset_for_scrollbar = 0usize;
                if let Some(session) = self.sessions.get_mut(id) {
                    let offset = session.scrollback_offset;
                    let filled = session.scrollback_filled();
                    filled_for_scrollbar = filled;
                    offset_for_scrollbar = offset;
                    let dim_this_pane = dim_panes || (multi_pane && !pane_focused);
                    render_pane(
                        session.screen(),
                        inner.row,
                        inner.col,
                        inner.rows,
                        inner.cols,
                        dim_this_pane,
                        &mut buf,
                    );
                    if pane_focused {
                        focused_pane_rect = Some(inner);
                    }
                }
                if let Some(session) = self.sessions.get(id) {
                    // Title precedence: agent's OSC 2 window title →
                    // shell's OSC 7 cwd → static `Session::label`.
                    let title_owned: String;
                    let title: &str = if let Some(t) = session.title() {
                        t
                    } else if let Some(cwd) = session.cwd() {
                        title_owned = jackin_tui::shorten_home(cwd);
                        &title_owned
                    } else {
                        session.label.as_str()
                    };
                    // The phosphor-green focus highlight is reserved
                    // for chrome the operator can actually *do*
                    // something with — multiple panes (focus
                    // matters) or a pane with scrollback (the wheel
                    // matters). A lone, non-scrollable pane stays
                    // gray so the brand colour does not compete with
                    // the agent's own content for attention.
                    let highlight_focus = multi_pane || filled_for_scrollbar > 0;
                    draw_pane_box(
                        &mut buf,
                        rect.row,
                        rect.col,
                        rect.rows,
                        rect.cols,
                        title,
                        pane_focused && highlight_focus,
                    );
                    // Scrollbar overlays the right border column, so
                    // it has to be drawn AFTER the pane box paints
                    // the border. Non-thumb rows leave the border
                    // intact; thumb rows replace `│` with `█`.
                    draw_scrollbar(
                        &mut buf,
                        rect.row,
                        rect.col,
                        rect.rows,
                        rect.cols,
                        offset_for_scrollbar,
                        filled_for_scrollbar,
                        pane_focused && highlight_focus,
                    );
                }
            }
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

        if let Some(dialog) = &self.dialog {
            dialog.render(&mut buf, self.term_rows, self.term_cols);
        }

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
        if self.dialog.is_none() {
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

        buf
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
}

/// Run the multiplexer daemon. Called from `main` when PID == 1.
pub async fn run_daemon(initial_agent: String) -> Result<()> {
    crate::pid1::install_sigchld_reaper();

    let rows = std::env::var("JACKIN_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(24u16);
    let cols = std::env::var("JACKIN_COLS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80u16);

    // Initialise the file logger before anything else can emit a
    // diagnostic. Failures fall back to stderr-only, so this is safe
    // to call unconditionally.
    crate::logging::init();
    crate::clog!("daemon start: rows={rows} cols={cols} initial_agent={initial_agent:?}");

    let mut mux = Multiplexer::new(rows, cols);
    // Spawn the first tab. Treat any spawn error as fatal at boot —
    // it usually means the entrypoint binary is missing from the
    // derived image, and silently degrading to an empty multiplexer
    // would hide the real problem behind a blank screen.
    if !initial_agent.is_empty() {
        if let Err(err) = mux.spawn_initial(&initial_agent) {
            crate::clog!("initial agent spawn failed (agent={initial_agent:?}): {err:?}");
            return Err(err);
        }
    } else if let Err(err) = mux.spawn_session(None) {
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
                tokio::spawn(perform_handshake(
                    stream,
                    client_permit,
                    handshake_tx,
                    sessions_snapshot,
                ));
            }

            // Validated attach handshake from the spawned handshake task.
            Some(ready) = handshake_rx.recv() => {
                let AttachHandshake { stream, rows, cols, spawn_agent, client_permit } = ready;
                mux.resize(rows, cols);
                // Honor a spawn-agent intent from `jackin-container new
                // <agent>`. Spawn failures get clog'd at error level so
                // a `jackin load --debug` run shows the underlying
                // cause; the attach completes either way so the
                // operator can still interact with the existing focused
                // session and decide whether to retry.
                if let Some(agent_slug) = spawn_agent {
                    // Re-validate the wire-decoded slug. The CLI argv
                    // path validates via `validate_agent_slug`, but the
                    // attach protocol carries a raw String — a peer
                    // that wins the socket race could otherwise inject
                    // an unallowlisted agent name (or a control byte)
                    // straight into `build_agent_command`.
                    match crate::session::validate_agent_slug(&agent_slug) {
                        Ok(_) => {
                            if let Err(err) =
                                mux.spawn_session(Some(agent_slug.clone()))
                            {
                                crate::clog!(
                                    "attach: spawn_session for {agent_slug:?} failed: {err:?}"
                                );
                            }
                        }
                        Err(reason) => {
                            crate::clog!(
                                "attach: rejected Hello.spawn_agent {agent_slug:?}: {reason}"
                            );
                        }
                    }
                }
                // Take over from any existing attach client. Send the
                // Shutdown frame BEFORE aborting the reader task —
                // `abort()` drops the task's `out_rx` receiver, which
                // makes the subsequent `tx.send(Shutdown)` return Err
                // and the bytes never leave the socket. Yield once
                // afterwards so the writer side has a chance to drain
                // before the task is cancelled. Then `detach_client`
                // takes care of the abort + per-field bookkeeping.
                if let Some(tx) = mux.attached_out.take() {
                    let _ = tx.send(encode_server(ServerFrame::Shutdown));
                }
                tokio::task::yield_now().await;
                if let Some(handle) = mux.attached_task.take() {
                    handle.abort();
                }
                // Drain any stale frames the old client task pushed
                // into cmd_tx before its abort actually took effect —
                // without this drain, the next `cmd_rx.recv()` after
                // the new attach is wired processes Input / Resize /
                // Detach against the NEW mux state. Inline drain via
                // try_recv keeps the takeover path single-threaded.
                while cmd_rx.try_recv().is_ok() {}
                let (new_out_tx, new_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
                mux.attached_out = Some(new_out_tx.clone());
                let welcome = encode_server(ServerFrame::Welcome {
                    session_count: mux.sessions.len() as u32,
                });
                let _ = new_out_tx.send(welcome);
                // Initial mode-state restore: send the focused
                // session's current modes (bracketed paste, etc.) so
                // the outer terminal matches what the agent expects.
                // Without this, a re-attach loses bracketed-paste
                // and the operator's clipboard arrives unwrapped.
                if let Some(focused) = mux.active_focused_id()
                    && let Some(session) = mux.sessions.get(&focused)
                {
                    for bytes in session.current_mode_state() {
                        let _ = new_out_tx.send(encode_server(ServerFrame::Output(bytes)));
                    }
                }
                let mut initial = b"\x1b[2J".to_vec();
                initial.extend(mux.compose_frame());
                let _ = new_out_tx.send(encode_server(ServerFrame::Output(initial)));
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
                        // Mark dirty; the render ticker coalesces
                        // bursts of PTY output into one frame per
                        // tick. Dialog-open still marks dirty — the
                        // render ticker now paints the dialog overlay
                        // against the latest pane state, so dismiss
                        // doesn't produce a sudden burst of
                        // accumulated frames.
                        mux.dirty = true;
                    }
                    SessionEvent::Exited { session_id } => {
                        // Remove the pane / tab immediately rather than
                        // leaving a stale `○ Done` placeholder behind.
                        // Matches the operator's mental model: "agent
                        // exited → its tab is gone."
                        mux.remove_exited_session(session_id);
                        mux.dirty = true;
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

            // Render ticker: drain the dirty flag at ~30 fps. One
            // frame per tick at most, regardless of how many PTY
            // events arrived since the last tick. `compose_frame`
            // includes the dialog overlay when one is open, so the
            // open-dialog case still composes (and the operator sees
            // dialog content over the latest pane state) instead of
            // accumulating dirty until dismiss — without this the
            // dismiss frame was a sudden jump of N frames' worth of
            // accumulated PTY output that the operator had no way to
            // see coming.
            _ = render_ticker.tick(), if mux.dirty => {
                mux.dirty = false;
                let frame_data = mux.compose_frame();
                mux.send_output(frame_data);
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
            let frame_data = mux.compose_frame();
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
                    mux.dialog.is_some()
                );
                if let Some(redraw) = mux.handle_input(event) {
                    mux.send_output(redraw);
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
                let frame_data = mux.compose_frame();
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
            if mux.dialog.is_none()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.sessions.get(&focused)
                && s.focus_events_enabled()
            {
                s.send_input(b"\x1b[I");
            }
        }
        ClientFrame::FocusOut => {
            if mux.dialog.is_none()
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
    spawn_agent: Option<String>,
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
        socket::handle_control_request(stream, first[0], sessions_snapshot).await;
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
        spawn_agent,
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
        spawn_agent,
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
    if let Some(tx) = mux.attached_out.take() {
        let _ = tx.send(encode_server(ServerFrame::Shutdown));
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
                    break;
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
        for c in from_col..=to_col {
            if let Some(cell) = screen.cell(r, c)
                && cell.has_contents()
            {
                row_text.push_str(cell.contents());
            } else {
                row_text.push(' ');
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
/// selection rectangle. Emitted after `render_pane` so the agent's
/// content is preserved underneath — the operator sees the same
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
        for c in from_col..=to_col {
            if let Some(cell) = screen.cell(r, c)
                && cell.has_contents()
            {
                buf.extend_from_slice(cell.contents().as_bytes());
            } else {
                buf.push(b' ');
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

/// SGR mouse wheel events set bit 6 of the button byte. Every value in
/// `64..=95` is a wheel event with some combination of modifier flags
/// (shift = +4, alt = +8, ctrl = +16). Forwarding any of them to an
/// agent or shell that did not request mouse mode dumps the raw SGR
/// bytes at the prompt — so the multiplexer always intercepts the
/// wheel for scrollback regardless of modifiers.
fn is_wheel_button(button: u8) -> bool {
    (64..96).contains(&button)
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
