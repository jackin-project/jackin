//! Launch rich terminal renderer and modal loops.

use std::io::Write;

use anyhow::Context;
use crossterm::ExecutableCommand;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use jackin_tui::ModalOutcome;
use jackin_tui::components::{ConfirmState, ErrorPopupState, SelectListState, TextInputState};
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::tui::cockpit::{emit_launch_hyperlink_overlays, render_launch_frame};
use crate::tui::prompts::{draw_confirm, draw_error_popup, draw_select, draw_text_prompt};
use crate::{LaunchHostTerminal, LaunchView, PromptResult};

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
    rain: Option<crate::tui::rain::RainState>,
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
}

fn read_pressed_key(context: &'static str) -> anyhow::Result<KeyEvent> {
    loop {
        let Event::Key(key) = crossterm::event::read().context(context)? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            anyhow::bail!("launch cancelled by operator");
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
            ModalOutcome::Cancel => Some(Err(anyhow::anyhow!("launch cancelled by operator"))),
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
            ModalOutcome::Cancel => Some(Err(anyhow::anyhow!("launch cancelled by operator"))),
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
            stdout.execute(EnterAlternateScreen)?;
        }
        stdout.execute(crossterm::cursor::Hide)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let mut terminal = ratatui::Terminal::new(backend)?;
        // Wipe whatever the previous surface left on the screen and force a full
        // first redraw. Under the host guard we skipped EnterAlternateScreen
        // (which would have cleared), so the console's last frame is still on
        // the inherited screen — clear it or the cockpit renders over it.
        terminal.clear().context("clearing launch screen")?;
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
        })
    }

    pub fn enter(
        no_motion: bool,
        host: &'static dyn LaunchHostTerminal,
        jackin_version: &'static str,
    ) -> anyhow::Result<Self> {
        Self::enter_with_check(no_motion, host, jackin_version, require_rich_terminal)
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
                self.rain = Some(crate::tui::rain::RainState::new(cols, rows));
            }
            if !no_motion
                && !view.frame.is_multiple_of(3)
                && let Some(rain) = &mut self.rain
            {
                crate::tui::rain::tick_rain(rain);
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
            emit_launch_hyperlink_overlays(
                Rect::new(0, 0, size.width, size.height),
                view,
                run_id,
                run_log_path,
                self.host.is_debug_mode(),
                self.jackin_version,
            );
        }
        Ok(())
    }

    /// Run a modal dialog loop with raw mode held for its duration so key
    /// events arrive un-buffered, restoring it on every exit path. The host
    /// guard already holds raw mode for the whole flow; only toggle it when
    /// this renderer is running standalone. `Ctrl-C` aborts the launch.
    fn with_raw_mode<T>(
        &mut self,
        context: &'static str,
        f: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let owns_raw = self.entered_alt_screen;
        if owns_raw {
            crossterm::terminal::enable_raw_mode().context(context)?;
        }
        let outcome = f(self);
        if owns_raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        outcome
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
        context: &[Line<'_>],
        items: Vec<String>,
    ) -> anyhow::Result<usize> {
        self.with_raw_mode("entering raw mode for cleanup picker", |renderer| {
            renderer.select_loop(title, context, items)
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
                SelectLoopMessage::Key(read_pressed_key("reading launch picker input")?),
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
                TextPromptMessage::Key(read_pressed_key("reading launch env prompt input")?),
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
            items.push("(skip)".to_string());
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
                SelectPromptMessage::Key(read_pressed_key("reading launch env select input")?),
            ) {
                return result;
            }
        }
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
                ConfirmPromptMessage::Key(read_pressed_key("reading launch confirmation input")?),
            ) {
                return Ok(result);
            }
        }
    }

    fn error_popup_loop(&mut self, title: &str, message: &str) -> anyhow::Result<()> {
        let state = ErrorPopupState::new(title, message);
        loop {
            self.terminal
                .draw(|frame| draw_error_popup(frame, &state))
                .context("rendering launch error popup")?;
            match state.handle_key(read_pressed_key("reading error popup input")?) {
                ModalOutcome::Cancel => return Ok(()),
                ModalOutcome::Continue => {}
                ModalOutcome::Commit(()) => unreachable!("error popup never commits"),
            }
        }
    }
}

impl Drop for RichRenderer {
    fn drop(&mut self) {
        self.host.set_rich_surface_active(false);
        let _ = self.terminal.backend_mut().execute(crossterm::cursor::Show);
        // Leave the alternate screen only when we entered it; under the host
        // guard the screen persists into the capsule attach.
        if self.entered_alt_screen {
            let _ = self.terminal.backend_mut().execute(LeaveAlternateScreen);
        }
        let _ = std::io::stdout().flush();
    }
}

fn require_rich_terminal() -> anyhow::Result<()> {
    if !crate::terminal::rich_terminal_supported() {
        anyhow::bail!(
            "jackin load requires a rich terminal: stdin/stdout/stderr must be TTYs, TERM must not be dumb, CI must be unset, and the terminal must be at least 80x24"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_select_message_commits_current_index() {
        let mut picker = SelectListState::new(vec!["alpha".into(), "beta".into()]);
        picker.select_index(1);

        let result = update_forced_select(
            &mut picker,
            SelectLoopMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );

        assert_eq!(result, Some(1));
    }

    #[test]
    fn forced_select_message_ignores_cancel() {
        let mut picker = SelectListState::new(vec!["alpha".into(), "beta".into()]);

        let result = update_forced_select(
            &mut picker,
            SelectLoopMessage::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        );

        assert_eq!(result, None);
    }

    #[test]
    fn select_prompt_message_commits_option_value() {
        let options = vec!["alpha".into(), "beta".into()];
        let mut picker = SelectListState::new(options.clone());
        picker.select_index(1);

        let result = update_select_prompt(
            &mut picker,
            &options,
            false,
            SelectPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        )
        .expect("enter commits")
        .expect("commit succeeds");

        assert_eq!(result, PromptResult::Value("beta".into()));
    }

    #[test]
    fn select_prompt_message_commits_skip_row_when_skippable() {
        let options = vec!["alpha".into(), "beta".into()];
        let mut picker = SelectListState::new(vec!["alpha".into(), "beta".into(), "(skip)".into()]);
        picker.select_index(2);

        let result = update_select_prompt(
            &mut picker,
            &options,
            true,
            SelectPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        )
        .expect("enter commits")
        .expect("skip succeeds");

        assert_eq!(result, PromptResult::Skipped);
    }

    #[test]
    fn text_prompt_message_commits_value() {
        let mut input = TextInputState::new("name", "demo");

        let result = update_text_prompt(
            &mut input,
            false,
            TextPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        )
        .expect("enter commits")
        .expect("commit succeeds");

        assert_eq!(result, PromptResult::Value("demo".into()));
    }

    #[test]
    fn text_prompt_message_commits_empty_as_skip_when_skippable() {
        let mut input = TextInputState::new_allow_empty("name", "");

        let result = update_text_prompt(
            &mut input,
            true,
            TextPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        )
        .expect("enter commits")
        .expect("skip succeeds");

        assert_eq!(result, PromptResult::Skipped);
    }

    #[test]
    fn confirm_prompt_message_commits_confirmation() {
        let mut state = ConfirmState::new("continue?").with_focus_yes();

        let result = update_confirm_prompt(
            &mut state,
            ConfirmPromptMessage::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );

        assert_eq!(result, Some(true));
    }

    #[test]
    fn confirm_prompt_message_cancel_returns_false() {
        let mut state = ConfirmState::new("continue?");

        let result = update_confirm_prompt(
            &mut state,
            ConfirmPromptMessage::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        );

        assert_eq!(result, Some(false));
    }
}
