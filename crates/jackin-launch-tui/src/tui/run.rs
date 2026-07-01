#![allow(clippy::too_many_lines)]
//! Launch rich terminal renderer and modal loops.

use std::io::Write;

use anyhow::Context;
use crossterm::ExecutableCommand;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use jackin_tui::ModalOutcome;
use jackin_tui::components::{ConfirmState, ErrorPopupState, SelectListState, TextInputState};
use ratatui::backend::Backend as _;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use tokio_util::sync::CancellationToken;

use crate::tui::components::prompts::{
    draw_confirm, draw_error_popup, draw_select, draw_text_prompt,
};
use crate::tui::input::{LaunchInput, restore_renderer_terminal_for_process_exit};
use crate::tui::message::LaunchMessage;
use crate::tui::subscriptions::{CockpitOutcome, SharedView, handle_cockpit_input};
use crate::tui::terminal::current_terminal_area;
use crate::tui::update::update_launch_view;
use crate::tui::view::{launch_hyperlink_overlays, render_launch_frame};
use crate::{LaunchHostTerminal, LaunchView, PromptContextLine, PromptResult};

pub fn rich_launch_dialog_required_message(what: &str) -> String {
    format!("{what} requires the rich launch dialog")
}

#[expect(
    missing_debug_implementations,
    reason = "RichRenderer owns terminal backend state that has no useful Debug representation."
)]
pub struct RichRenderer {
    terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    no_motion: bool,
    /// Whether this renderer entered the alternate screen on construction.
    /// Recorded so `drop` can leave it only when we entered it — under the
    /// host `TerminalSession` guard the screen persists into the capsule attach.
    entered_alt_screen: bool,
    /// Shared digital-rain engine (the same one the intro/outro use), ticked
    /// per frame and painted into the loading box. Sized to the terminal so
    /// the box shows a window into one continuous rainfall.
    rain: Option<crate::tui::components::rain::RainState>,
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
    input: LaunchInput,
}

/// Owns the background render task that ticks the cockpit independently of
/// launch work, so rain and animation continue while a launch step waits on I/O.
#[expect(
    missing_debug_implementations,
    reason = "RichDriver owns a render task handle and terminal renderer state that are not diagnostic data."
)]
pub struct RichDriver {
    renderer: std::sync::Arc<std::sync::Mutex<RichRenderer>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl RichDriver {
    #[allow(
        clippy::excessive_nesting,
        reason = "RichDriver spawn wires the render thread, the tick loop, the \
                  input loop, and the main task; the nesting is the per-loop-arm \
                  control flow (input drain / render / stop-check / event-poll) \
                  intrinsic to the multi-loop driver shape."
    )]
    pub fn spawn(
        renderer: RichRenderer,
        view: SharedView,
        run_id: String,
        run_log_path: String,
        host: &'static dyn LaunchHostTerminal,
        jackin_version: &'static str,
        cancel_token: CancellationToken,
    ) -> Self {
        use std::sync::atomic::Ordering;
        let renderer = std::sync::Arc::new(std::sync::Mutex::new(renderer));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handle = {
            let renderer = std::sync::Arc::clone(&renderer);
            let stop = std::sync::Arc::clone(&stop);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(33));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(mut rr) = renderer.try_lock() else {
                        continue;
                    };
                    let outcome = handle_cockpit_input(
                        &view,
                        &run_id,
                        &run_log_path,
                        host,
                        jackin_version,
                        &cancel_token,
                        &rr.input,
                    );
                    // Ctrl+C — immediate hard stop. Restore the terminal, then
                    // exit the process at once: no graceful teardown, no waiting
                    // on in-flight blocking work (binary download/extract,
                    // `docker build`). Stale docker resources are reclaimed by
                    // the next launch's `gc_orphaned_resources`. This is the one
                    // path that deliberately skips `LoadCleanup`.
                    if outcome == CockpitOutcome::HardExit {
                        rr.host.set_rich_surface_active(false);
                        restore_renderer_terminal_for_process_exit(&mut rr.terminal);
                        std::process::exit(0);
                    }
                    // Other cancellation sources can still ask the launch
                    // pipeline to unwind gracefully. Operator quit from the
                    // cockpit uses the HardExit arm above.
                    if cancel_token.is_cancelled() {
                        rr.restore_terminal();
                        break;
                    }
                    let snapshot = match view.lock() {
                        Ok(mut v) => {
                            let build_log_lines = jackin_diagnostics::build_log::snapshot();
                            let build_log_active = jackin_diagnostics::build_log::is_active();
                            let build_log_area = if v.build_log_open {
                                Some(current_terminal_area())
                            } else {
                                None
                            };
                            let _dirty = update_launch_view(
                                &mut v,
                                LaunchMessage::RenderTick {
                                    advance_frame: !rr.no_motion(),
                                    build_log_area,
                                    build_log_lines,
                                    build_log_active,
                                },
                            );
                            v.clone()
                        }
                        Err(_) => continue,
                    };
                    drop(rr.render(&snapshot, &run_id, &run_log_path));
                }
            })
        };
        Self {
            renderer,
            stop,
            handle: Some(handle),
        }
    }

    pub fn stop_detached(&mut self) {
        use std::sync::atomic::Ordering;
        self.stop.store(true, Ordering::Relaxed);
        drop(self.handle.take());
    }

    pub fn request_stop(&self) {
        use std::sync::atomic::Ordering;
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn with_renderer<T>(
        &mut self,
        f: impl FnOnce(&mut RichRenderer) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut renderer = self
            .renderer
            .lock()
            .map_err(|_| anyhow::anyhow!("launch renderer mutex poisoned"))?;
        f(&mut renderer)
    }
}

fn read_pressed_key(input: &LaunchInput, context: &'static str) -> anyhow::Result<KeyEvent> {
    loop {
        let key = input.recv_key(context)?;
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let is_ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        let is_ctrl_q =
            key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL);
        if is_ctrl_c || is_ctrl_q {
            return Err(crate::LaunchCancelled::err());
        }
        return Ok(key);
    }
}

#[derive(Debug, Clone, Copy)]
enum SelectLoopMessage {
    Key(KeyEvent),
}

#[derive(Debug, Clone, Copy)]
enum SelectPromptMessage {
    Key(KeyEvent),
}

#[derive(Debug, Clone, Copy)]
enum TextPromptMessage {
    Key(KeyEvent),
}

#[derive(Debug, Clone, Copy)]
enum ConfirmPromptMessage {
    Key(KeyEvent),
}

#[derive(Debug, Clone, Copy)]
enum ErrorPromptMessage {
    Key(KeyEvent),
}

fn update_forced_select(picker: &mut SelectListState, msg: SelectLoopMessage) -> Option<usize> {
    match msg {
        SelectLoopMessage::Key(key) => {
            // Esc reports Cancel; ignored here so the choice is forced.
            if let ModalOutcome::Commit(index) = picker.handle_key(key) {
                Some(index)
            } else {
                None
            }
        }
    }
}

fn update_error_prompt(state: &ErrorPopupState, msg: ErrorPromptMessage) -> Option<()> {
    match msg {
        ErrorPromptMessage::Key(key) => match state.handle_key(key) {
            ModalOutcome::Cancel => Some(()),
            ModalOutcome::Continue => None,
            ModalOutcome::Commit(()) => unreachable!("error popup never commits"),
        },
    }
}

fn update_confirm_prompt(state: &mut ConfirmState, msg: ConfirmPromptMessage) -> Option<bool> {
    match msg {
        ConfirmPromptMessage::Key(key) => match state.handle_key(key) {
            ModalOutcome::Commit(confirmed) => Some(confirmed),
            ModalOutcome::Cancel => Some(false),
            ModalOutcome::Continue => None,
        },
    }
}

fn update_text_prompt(
    input: &mut TextInputState<'_>,
    skippable: bool,
    msg: TextPromptMessage,
) -> Option<anyhow::Result<PromptResult>> {
    match msg {
        TextPromptMessage::Key(key) => match input.handle_key(key) {
            ModalOutcome::Commit(value) if value.is_empty() && skippable => {
                Some(Ok(PromptResult::Skipped))
            }
            ModalOutcome::Commit(value) => Some(Ok(PromptResult::Value(value))),
            ModalOutcome::Cancel => Some(Err(crate::LaunchCancelled::err())),
            ModalOutcome::Continue => None,
        },
    }
}

fn update_select_prompt(
    picker: &mut SelectListState,
    options: &[String],
    skippable: bool,
    msg: SelectPromptMessage,
) -> Option<anyhow::Result<PromptResult>> {
    match msg {
        SelectPromptMessage::Key(key) => match picker.handle_key(key) {
            ModalOutcome::Commit(index) if skippable && index == options.len() => {
                Some(Ok(PromptResult::Skipped))
            }
            ModalOutcome::Commit(index) => Some(Ok(PromptResult::Value(options[index].clone()))),
            ModalOutcome::Cancel => Some(Err(crate::LaunchCancelled::err())),
            ModalOutcome::Continue => None,
        },
    }
}

impl RichRenderer {
    fn enter_with_check(
        no_motion: bool,
        host: &'static dyn LaunchHostTerminal,
        jackin_version: &'static str,
        terminal_check: impl FnOnce() -> anyhow::Result<()>,
    ) -> anyhow::Result<Self> {
        terminal_check()?;
        let mut stdout = std::io::stdout();
        // When the launch flow's host guard already owns the alternate screen,
        // draw into it; only enter it ourselves when running standalone.
        let entered_alt_screen = !host.host_screen_owned();
        if entered_alt_screen {
            crossterm::terminal::enable_raw_mode().context("entering raw mode for launch TUI")?;
            stdout.execute(EnterAlternateScreen)?;
            jackin_tui::terminal_modes::enable_mouse_capture(&mut stdout)
                .context("enabling mouse capture for launch TUI")?;
        }
        stdout.execute(crossterm::cursor::Hide)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let mut terminal = ratatui::Terminal::new(backend)?;
        // Wipe whatever the previous surface left on the screen and force a full
        // first redraw. Under the host guard we skipped EnterAlternateScreen
        // (which would have cleared), so the console's last frame is still on
        // the inherited screen — clear it or the cockpit renders over it.
        // Use backend_mut().clear() instead of terminal.clear(): ratatui-core ≥ 0.1.1
        // added a cursor-position save/restore around the erase that blocks on a DSR
        // query. On non-interactive PTYs (e.g. the script-based E2E harness) the
        // terminal never answers, causing a timeout error. The backend call issues the
        // same erase without the query; a freshly constructed Terminal already has
        // default (empty) buffers so the next draw will repaint everything anyway.
        terminal
            .backend_mut()
            .clear()
            .context("clearing launch screen")?;
        // Ancillary status printers (spinners) go silent while this surface
        // owns the alternate screen.
        host.set_rich_surface_active(true);
        Ok(Self {
            terminal,
            no_motion,
            entered_alt_screen,
            rain: None,
            host,
            jackin_version,
            input: LaunchInput::spawn(),
        })
    }

    pub fn enter(
        no_motion: bool,
        host: &'static dyn LaunchHostTerminal,
        jackin_version: &'static str,
    ) -> anyhow::Result<Self> {
        Self::enter_with_check(
            no_motion,
            host,
            jackin_version,
            crate::tui::terminal::require_rich_terminal,
        )
    }

    pub fn enter_dialog(
        no_motion: bool,
        host: &'static dyn LaunchHostTerminal,
        jackin_version: &'static str,
    ) -> anyhow::Result<Self> {
        Self::enter_with_check(no_motion, host, jackin_version, || Ok(()))
    }

    pub fn no_motion(&self) -> bool {
        self.no_motion
    }

    pub fn render(
        &mut self,
        view: &LaunchView,
        run_id: &str,
        run_log_path: &str,
    ) -> anyhow::Result<()> {
        let no_motion = self.no_motion;
        // Keep the rain engine sized to the terminal. Advance it every other
        // render so the rainfall reads at the calmer main-branch speed while
        // the frame still redraws smoothly (~30fps). Paused under no-motion.
        if let Ok(size) = self.terminal.size() {
            let (cols, rows) = (size.width as usize, size.height as usize);
            let stale = self
                .rain
                .as_ref()
                .is_none_or(|rain| rain.cols != cols || rain.rows != rows);
            if stale && cols > 0 && rows > 0 {
                self.rain = Some(crate::tui::components::rain::RainState::new(cols, rows));
            }
            if !no_motion
                && !view.frame.is_multiple_of(3)
                && let Some(rain) = &mut self.rain
            {
                crate::tui::components::rain::tick_rain(rain);
            }
        }
        let rain = self.rain.as_ref();
        let size = self.terminal.size().ok();
        self.terminal
            .draw(|frame| {
                render_launch_frame(
                    frame,
                    view,
                    run_id,
                    run_log_path,
                    no_motion,
                    rain,
                    self.host.is_debug_mode(),
                    self.jackin_version,
                );
            })
            .map(|_| ())
            .context("rendering launch progress TUI")?;
        if let Some(size) = size {
            let overlays = launch_hyperlink_overlays(
                Rect::new(0, 0, size.width, size.height),
                view,
                run_id,
                run_log_path,
                self.host.is_debug_mode(),
                self.jackin_version,
            );
            if !overlays.is_empty() {
                let mut stdout = std::io::stdout();
                drop(stdout.write_all(&overlays));
                drop(stdout.flush());
            }
        }
        Ok(())
    }

    /// Run a modal dialog loop while raw mode is already held by either the
    /// host guard or this standalone renderer. `Ctrl-C` aborts the launch.
    fn with_raw_mode<T>(
        &mut self,
        _context: &'static str,
        f: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        f(self)
    }

    /// Present a forced-choice picker over the dimmed launch frame.
    pub fn select(&mut self, title: &str, items: Vec<String>) -> anyhow::Result<usize> {
        self.with_raw_mode("entering raw mode for launch picker", |renderer| {
            renderer.select_loop(title, &[], items)
        })
    }

    /// Forced-choice picker with a descriptive `context` block above the
    /// options. Used by the standalone post-attach cleanup prompt.
    pub fn select_with_context(
        &mut self,
        title: &str,
        context: &[PromptContextLine],
        items: Vec<String>,
    ) -> anyhow::Result<usize> {
        let context = prompt_context_lines(context);
        self.with_raw_mode("entering raw mode for cleanup picker", |renderer| {
            renderer.select_loop(title, &context, items)
        })
    }

    pub fn error_popup(&mut self, title: &str, message: &str) -> anyhow::Result<()> {
        self.with_raw_mode("entering raw mode for error popup", |renderer| {
            renderer.error_popup_loop(title, message)
        })
    }

    fn select_loop(
        &mut self,
        title: &str,
        context: &[Line<'_>],
        items: Vec<String>,
    ) -> anyhow::Result<usize> {
        let mut picker = SelectListState::new(items);
        loop {
            self.terminal
                .draw(|frame| draw_select(frame, title, context, &picker))
                .context("rendering launch picker")?;
            if let Some(index) = update_forced_select(
                &mut picker,
                SelectLoopMessage::Key(read_pressed_key(
                    &self.input,
                    "reading launch picker input",
                )?),
            ) {
                return Ok(index);
            }
        }
    }

    pub fn prompt_text(
        &mut self,
        title: &str,
        initial: &str,
        skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        self.with_raw_mode("entering raw mode for launch env prompt", |renderer| {
            renderer.prompt_text_loop(title, initial, skippable)
        })
    }

    fn prompt_text_loop(
        &mut self,
        title: &str,
        initial: &str,
        skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        let mut input = if skippable {
            TextInputState::new_allow_empty(title, initial)
        } else {
            TextInputState::new(title, initial)
        };
        loop {
            self.terminal
                .draw(|frame| draw_text_prompt(frame, &input, skippable))
                .context("rendering launch env text prompt")?;
            if let Some(result) = update_text_prompt(
                &mut input,
                skippable,
                TextPromptMessage::Key(read_pressed_key(
                    &self.input,
                    "reading launch env prompt input",
                )?),
            ) {
                return result;
            }
        }
    }

    pub fn prompt_select(
        &mut self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        self.with_raw_mode("entering raw mode for launch env select", |renderer| {
            renderer.prompt_select_loop(title, options, default, skippable)
        })
    }

    fn prompt_select_loop(
        &mut self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        let mut items = options.to_vec();
        if skippable {
            items.push("(skip)".to_owned());
        }
        let mut picker = SelectListState::new(items);
        if let Some(default) = default
            && let Some(index) = options.iter().position(|option| option == default)
        {
            picker.select_index(index);
        }
        loop {
            self.terminal
                .draw(|frame| draw_select(frame, title, &[], &picker))
                .context("rendering launch env select prompt")?;
            if let Some(result) = update_select_prompt(
                &mut picker,
                options,
                skippable,
                SelectPromptMessage::Key(read_pressed_key(
                    &self.input,
                    "reading launch env select input",
                )?),
            ) {
                return result;
            }
        }
    }

    pub fn confirm_prompt(&mut self, prompt: impl Into<String>) -> anyhow::Result<bool> {
        self.confirm(ConfirmState::new(prompt))
    }

    pub fn confirm_role_trust(
        &mut self,
        role: impl Into<String>,
        repository: impl Into<String>,
    ) -> anyhow::Result<bool> {
        self.confirm(role_trust_confirm_state(role.into(), repository.into()))
    }

    pub fn confirm(&mut self, mut state: ConfirmState) -> anyhow::Result<bool> {
        self.with_raw_mode("entering raw mode for launch confirmation", |renderer| {
            renderer.confirm_loop(&mut state)
        })
    }

    fn confirm_loop(&mut self, state: &mut ConfirmState) -> anyhow::Result<bool> {
        loop {
            self.terminal
                .draw(|frame| draw_confirm(frame, state))
                .context("rendering launch confirmation")?;
            if let Some(result) = update_confirm_prompt(
                state,
                ConfirmPromptMessage::Key(read_pressed_key(
                    &self.input,
                    "reading launch confirmation input",
                )?),
            ) {
                return Ok(result);
            }
        }
    }

    #[allow(
        clippy::excessive_nesting,
        reason = "Popup loop: per-modal-state (Picker / Confirm / Error / etc.) \
                  branches with per-arm draw + key-read + state-update nested \
                  through the modal dispatch. The modal nesting is the protocol."
    )]
    fn error_popup_loop(&mut self, title: &str, message: &str) -> anyhow::Result<()> {
        let state = ErrorPopupState::new(title, message);
        loop {
            self.terminal
                .draw(|frame| draw_error_popup(frame, &state))
                .context("rendering launch error popup")?;
            if update_error_prompt(
                &state,
                ErrorPromptMessage::Key(read_pressed_key(
                    &self.input,
                    "reading error popup input",
                )?),
            )
            .is_some()
            {
                return Ok(());
            }
        }
    }

    // ── D23 launch dialog with D21 delete-in-place ─────────────────────────

    /// D23/D21: rich launch picker supporting `Del` (delete candidate) and
    /// `I` (inspect dirty state via the D24 surface). Returns `LaunchDialogResult`.
    pub fn launch_dialog(
        &mut self,
        title: &str,
        candidates: &[crate::LaunchCandidate],
    ) -> anyhow::Result<crate::LaunchDialogResult> {
        self.with_raw_mode("launch dialog", |renderer| {
            renderer.launch_dialog_loop(title, candidates)
        })
    }

    #[allow(
        clippy::excessive_nesting,
        reason = "Launch dialog loop: mode (Picker / Inspect) branches with \
                  per-arm render + key-read + select-flow nested through the \
                  launcher dialog dispatch. Modal nesting is the protocol."
    )]
    #[allow(
        clippy::excessive_nesting,
        reason = "Launch dialog loop: mode (Picker / Inspect) branches with \
                  per-arm render + key-read + select-flow nested through the \
                  launcher dialog dispatch. Modal nesting is the protocol."
    )]
    fn launch_dialog_loop(
        &mut self,
        title: &str,
        candidates: &[crate::LaunchCandidate],
    ) -> anyhow::Result<crate::LaunchDialogResult> {
        use crate::tui::components::dialog::dialog_backdrop;
        use jackin_tui::HintSpan;
        use jackin_tui::centered_rect;
        use jackin_tui::components::render_hint_bar;
        use jackin_tui::components::{ConfirmState, render_confirm_dialog};

        // Item 0 = "Start new session"; items 1..=N = candidates.
        let mut labels = vec!["Start new session".to_owned()];
        labels.extend(candidates.iter().map(|c| c.label.clone()));
        let mut picker = SelectListState::new(labels);

        enum Mode {
            Picker,
            ConfirmDelete(usize),
        }
        let mut mode = Mode::Picker;

        let hint_normal: &[HintSpan<'static>] = &[
            HintSpan::Key("↑↓"),
            HintSpan::Text("navigate"),
            HintSpan::Sep,
            HintSpan::Key("↵"),
            HintSpan::Text("resume"),
            HintSpan::Sep,
            HintSpan::Key("I"),
            HintSpan::Text("inspect"),
            HintSpan::Sep,
            HintSpan::Key("Del"),
            HintSpan::Text("delete"),
            HintSpan::GroupSep,
            HintSpan::Text("type to filter"),
        ];

        loop {
            match &mut mode {
                Mode::Picker => {
                    self.terminal
                        .draw(|frame| {
                            let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
                            let picker_rect = {
                                let rows = u16::try_from(picker.len())
                                    .unwrap_or(u16::MAX)
                                    .saturating_add(4);
                                let height =
                                    rows.clamp(6, box_area.height.saturating_sub(2).max(6));
                                let width = (box_area.width.saturating_mul(4) / 5)
                                    .max(40.min(box_area.width));
                                centered_rect(width, height, box_area)
                            };
                            use jackin_tui::components::render_select_list;
                            render_select_list(frame, picker_rect, &picker, title, &[]);
                            render_hint_bar(frame, hint_area, hint_normal);
                        })
                        .context("rendering launch dialog")?;

                    let key = read_pressed_key(&self.input, "reading launch dialog input")?;
                    // Check for I (inspect) or Del before passing to picker.
                    let sel = picker.selected_index();
                    if key.code == KeyCode::Char('i') || key.code == KeyCode::Char('I') {
                        if let Some(s) = sel
                            && s > 0
                        {
                            let ci = s - 1;
                            if !candidates[ci].inspect.is_empty() {
                                self.inspect_surface_loop(&candidates[ci].inspect)?;
                            }
                        }
                        continue;
                    }
                    if key.code == KeyCode::Delete {
                        if let Some(s) = sel
                            && s > 0
                        {
                            let ci = s - 1;
                            if candidates[ci].is_dirty {
                                mode = Mode::ConfirmDelete(ci);
                            } else {
                                return Ok(crate::LaunchDialogResult::Delete(ci));
                            }
                        }
                        continue;
                    }
                    if let ModalOutcome::Commit(index) = picker.handle_key(key) {
                        return Ok(if index == 0 {
                            crate::LaunchDialogResult::StartFresh
                        } else {
                            crate::LaunchDialogResult::Restore(index - 1)
                        });
                    }
                }
                Mode::ConfirmDelete(ci) => {
                    let ci = *ci;
                    let label = &candidates[ci].label;
                    let mut confirm = ConfirmState::new(format!(
                        "Delete {label}?\n\nAny uncommitted changes will be lost."
                    ));
                    self.terminal
                        .draw(|frame| {
                            let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
                            use jackin_tui::components::{
                                confirm_hint_spans, confirm_required_height, confirm_width_pct,
                            };
                            let width =
                                box_area.width.saturating_mul(confirm_width_pct(&confirm)) / 100;
                            let height = confirm_required_height(&confirm);
                            let rect = centered_rect(width, height, box_area);
                            render_confirm_dialog(frame, rect, &confirm);
                            render_hint_bar(frame, hint_area, &confirm_hint_spans());
                        })
                        .context("rendering delete confirm")?;
                    let key = read_pressed_key(&self.input, "reading delete confirm input")?;
                    match update_confirm_prompt(&mut confirm, ConfirmPromptMessage::Key(key)) {
                        Some(true) => return Ok(crate::LaunchDialogResult::Delete(ci)),
                        Some(false) => mode = Mode::Picker,
                        None => {}
                    }
                }
            }
        }
    }

    // ── D24 Inspect surface ──────────────────────────────────────────────────

    /// D24: read-only inspect surface for dirty/unpushed worktrees.
    /// Returns when the operator presses Esc.
    #[allow(
        clippy::excessive_nesting,
        reason = "Inspect-surface loop: per-focus-tab (Repos / Files / Diff) nested \
                  arms for render + key-handle, plus the focus-state-machine nested \
                  per Tab key. Modal nesting is the per-tab render dispatch."
    )]
    fn inspect_surface_loop(&mut self, worktrees: &[crate::WorktreeInspect]) -> anyhow::Result<()> {
        use crate::tui::components::dialog::dialog_backdrop;
        use jackin_tui::HintSpan;
        use jackin_tui::components::{
            DiffViewState, SelectListState, SinglePaneKind, render_diff_view, render_hint_bar,
            render_select_list,
        };
        use ratatui::layout::{Constraint, Direction, Layout};

        if worktrees.is_empty() {
            return Ok(());
        }

        let hint: &[HintSpan<'static>] = &[
            HintSpan::Key("↑↓"),
            HintSpan::Text("files"),
            HintSpan::Sep,
            HintSpan::Key("Tab"),
            HintSpan::Text("pane"),
            HintSpan::Sep,
            HintSpan::Key("Esc"),
            HintSpan::Text("back"),
        ];

        // If only one worktree, repos pane is hidden.
        let mut wt_sel: usize = 0;
        let mut file_sel: usize = 0;
        #[derive(PartialEq, Clone, Copy)]
        enum InspFocus {
            Repos,
            Files,
            Diff,
        }
        let mut focus = if worktrees.len() > 1 {
            InspFocus::Repos
        } else {
            InspFocus::Files
        };
        let mut diff_scroll_y: u16 = 0;

        let build_diff = |wt: &crate::WorktreeInspect, fi: usize| -> Option<DiffViewState> {
            let file = wt.files.get(fi)?;
            Some(match (file.before.as_deref(), file.after.as_deref()) {
                (Some(before), Some(after)) => {
                    DiffViewState::side_by_side(before, after, "HEAD", "working")
                }
                (None, Some(after)) => {
                    DiffViewState::single_pane(after, SinglePaneKind::Added, &file.path)
                }
                (Some(before), None) => {
                    DiffViewState::single_pane(before, SinglePaneKind::Deleted, &file.path)
                }
                (None, None) => {
                    DiffViewState::single_pane("", SinglePaneKind::Untracked, &file.path)
                }
            })
        };

        // Build initial diff
        let mut diff_state = build_diff(&worktrees[wt_sel], file_sel);

        loop {
            let wt = &worktrees[wt_sel];
            let file_labels: Vec<String> = wt
                .files
                .iter()
                .map(|f| format!("{} {}", f.status, f.path))
                .collect();

            let wt_labels: Vec<String> = worktrees.iter().map(|w| w.label.clone()).collect();
            let wt_sel_c = wt_sel;
            let file_sel_c = file_sel;
            let focus_c = focus;
            let has_repos = worktrees.len() > 1;
            let mut diff_cloned = diff_state.clone();

            self.terminal
                .draw(|frame| {
                    let (body, hint_area) = dialog_backdrop(frame, frame.area());
                    render_hint_bar(frame, hint_area, hint);

                    // Split body: repos (if >1) | files | diff
                    let constraints = if has_repos {
                        vec![
                            Constraint::Percentage(20),
                            Constraint::Percentage(30),
                            Constraint::Percentage(50),
                        ]
                    } else {
                        vec![Constraint::Percentage(35), Constraint::Percentage(65)]
                    };
                    let panes = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(constraints)
                        .split(body);

                    let (repos_area, files_area, diff_area) = if has_repos {
                        (Some(panes[0]), panes[1], panes[2])
                    } else {
                        (None, panes[0], panes[1])
                    };

                    // Mark the Tab-focused pane with the ▸ selection glyph so the
                    // operator sees which pane Up/Down/PageUp drive.
                    if let Some(repos_area) = repos_area {
                        let mut repos_state = SelectListState::new(wt_labels.clone());
                        repos_state.select_index(wt_sel_c);
                        let title = if matches!(focus_c, InspFocus::Repos) {
                            "▸ Repos"
                        } else {
                            "Repos"
                        };
                        render_select_list(frame, repos_area, &repos_state, title, &[]);
                    }

                    let mut files_state = SelectListState::new(file_labels.clone());
                    files_state.select_index(file_sel_c);
                    let files_title = if matches!(focus_c, InspFocus::Files) {
                        "▸ Changed files"
                    } else {
                        "Changed files"
                    };
                    render_select_list(frame, files_area, &files_state, files_title, &[]);

                    if let Some(diff) = diff_cloned.as_mut() {
                        diff.set_scroll_y(diff_scroll_y);
                        render_diff_view(frame, diff_area, diff);
                        diff_scroll_y = diff.scroll_y();
                    }
                })
                .context("rendering inspect surface")?;

            let key = read_pressed_key(&self.input, "reading inspect surface input")?;
            match key.code {
                KeyCode::Esc => return Ok(()),
                KeyCode::Tab => {
                    focus = match focus {
                        InspFocus::Repos => InspFocus::Files,
                        InspFocus::Files => InspFocus::Diff,
                        InspFocus::Diff => {
                            if worktrees.len() > 1 {
                                InspFocus::Repos
                            } else {
                                InspFocus::Files
                            }
                        }
                    };
                }
                KeyCode::Up => match focus {
                    InspFocus::Repos => {
                        wt_sel = wt_sel.saturating_sub(1);
                        file_sel = 0;
                        diff_state = build_diff(&worktrees[wt_sel], file_sel);
                        diff_scroll_y = 0;
                    }
                    InspFocus::Files => {
                        file_sel = file_sel.saturating_sub(1);
                        diff_state = build_diff(&worktrees[wt_sel], file_sel);
                        diff_scroll_y = 0;
                    }
                    InspFocus::Diff => {
                        if let Some(d) = diff_state.as_mut() {
                            d.scroll_up();
                            diff_scroll_y = d.scroll_y();
                        }
                    }
                },
                KeyCode::Down => match focus {
                    InspFocus::Repos => {
                        if wt_sel + 1 < worktrees.len() {
                            wt_sel += 1;
                            file_sel = 0;
                            diff_state = build_diff(&worktrees[wt_sel], file_sel);
                            diff_scroll_y = 0;
                        }
                    }
                    InspFocus::Files => {
                        let max = worktrees[wt_sel].files.len().saturating_sub(1);
                        if file_sel < max {
                            file_sel += 1;
                            diff_state = build_diff(&worktrees[wt_sel], file_sel);
                            diff_scroll_y = 0;
                        }
                    }
                    InspFocus::Diff => {
                        if let Some(d) = diff_state.as_mut() {
                            d.scroll_down();
                            diff_scroll_y = d.scroll_y();
                        }
                    }
                },
                KeyCode::PageUp => {
                    if let Some(d) = diff_state.as_mut() {
                        d.page_up(20);
                        diff_scroll_y = d.scroll_y();
                    }
                }
                KeyCode::PageDown => {
                    if let Some(d) = diff_state.as_mut() {
                        d.page_down(20);
                        diff_scroll_y = d.scroll_y();
                    }
                }
                _ => {}
            }
        }
    }

    // ── D24 exit dialog with inspect ─────────────────────────────────────────

    /// Exit dialog with `I`-key inspect support. D23 three-way choice
    /// (Return/Keep/Discard) with D24 inspect reachable per worktree.
    pub fn exit_dialog_with_inspect(
        &mut self,
        title: &str,
        context: &[PromptContextLine],
        options: Vec<String>,
        worktrees_per_record: &[Vec<crate::WorktreeInspect>],
    ) -> anyhow::Result<usize> {
        let context = prompt_context_lines(context);
        self.with_raw_mode("exit dialog", |renderer| {
            renderer.exit_dialog_inspect_loop(title, &context, options, worktrees_per_record)
        })
    }

    fn exit_dialog_inspect_loop(
        &mut self,
        title: &str,
        context: &[Line<'_>],
        options: Vec<String>,
        worktrees_per_record: &[Vec<crate::WorktreeInspect>],
    ) -> anyhow::Result<usize> {
        use crate::tui::components::prompts::draw_select;

        let mut picker = SelectListState::new(options);

        loop {
            self.terminal
                .draw(|frame| {
                    draw_select(frame, title, context, &picker);
                })
                .context("rendering exit dialog")?;

            let key = read_pressed_key(&self.input, "reading exit dialog input")?;

            // Intercept I before passing to picker.
            if key.code == KeyCode::Char('i') || key.code == KeyCode::Char('I') {
                // Inspect the worktrees for the first record (or all).
                // The exit dialog selects a *batch* action (Keep all / Discard all),
                // so we show the inspect surface for all preserved records.
                let all_worktrees: Vec<crate::WorktreeInspect> = worktrees_per_record
                    .iter()
                    .flat_map(|wts| wts.iter().cloned())
                    .collect();
                if !all_worktrees.is_empty() {
                    self.inspect_surface_loop(&all_worktrees)?;
                }
                continue;
            }

            if let ModalOutcome::Commit(index) = picker.handle_key(key) {
                return Ok(index);
            }
        }
    }
}

fn role_trust_confirm_state(role: String, repository: String) -> ConfirmState {
    ConfirmState::details(
        "Trust role source",
        "Trust this role source?",
        vec![("Role".into(), role), ("Repository".into(), repository)],
        vec![
            "Dockerfile can run during image builds.".into(),
            "The role can access mounted workspace files.".into(),
        ],
    )
}

fn prompt_context_lines(context: &[PromptContextLine]) -> Vec<Line<'static>> {
    context
        .iter()
        .map(|line| match line {
            PromptContextLine::Emphasis(text) => Line::from(Span::styled(
                text.clone(),
                Style::default()
                    .fg(jackin_tui::theme::WHITE)
                    .add_modifier(Modifier::BOLD),
            )),
            PromptContextLine::Muted(text) => Line::from(Span::styled(
                text.clone(),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            )),
            PromptContextLine::Path(text) => Line::from(Span::styled(
                text.clone(),
                Style::default().fg(jackin_tui::theme::LINK_BLUE),
            )),
            PromptContextLine::Plain(text) => Line::from(text.clone()),
            PromptContextLine::Blank => Line::from(String::new()),
        })
        .collect()
}

impl RichRenderer {
    /// Restore the terminal to its pre-launch state immediately.
    ///
    /// Called explicitly from the render task on cancel detection so that the
    /// terminal is visible before cleanup runs (cleanup can take 10-30 s).
    /// Sets `entered_alt_screen = false` so the `Drop` impl is a no-op if this
    /// was already called — restoration is idempotent.
    pub(super) fn restore_terminal(&mut self) {
        self.host.set_rich_surface_active(false);
        drop(self.terminal.backend_mut().execute(crossterm::cursor::Show));
        if self.entered_alt_screen {
            drop(jackin_tui::terminal_modes::disable_mouse_capture(
                self.terminal.backend_mut(),
            ));
            drop(crossterm::terminal::disable_raw_mode());
            drop(self.terminal.backend_mut().execute(LeaveAlternateScreen));
            self.entered_alt_screen = false;
        }
        drop(std::io::stdout().flush());
    }
}

impl Drop for RichRenderer {
    fn drop(&mut self) {
        // `restore_terminal()` sets `entered_alt_screen = false` when called
        // explicitly on cancel, making this a no-op for the cancel path.
        self.restore_terminal();
    }
}

#[cfg(test)]
mod tests;
