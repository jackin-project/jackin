// `ConsoleStage` collapsed to a single variant in PR #171's Modal::AgentPicker
// cleanup. The module is kept as-is (with `if let ConsoleStage::Manager(_)`
// patterns) so a future stage can be added without rewriting every match
// site. The irrefutable-pattern lint is allowed at the module level rather
// than peppering individual sites.
#![allow(irrefutable_let_patterns)]

pub mod manager;
pub mod op_cache;
mod preview;
pub mod state;
pub mod widgets;

pub use op_cache::OpCache;
pub use state::ConsoleStage;
pub use state::ConsoleState;
pub use state::WorkspaceChoice;

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use crate::workspace::ResolvedWorkspace;

impl ConsoleState {
    /// Shared `LaunchNamed` / `LaunchCurrentDir` transition: preselect
    /// the workspace at `idx` in `ConsoleState::workspaces`, then route
    /// by agent count.
    ///
    /// Three branches:
    /// 1. `default_agent` set on the workspace → launch immediately with
    ///    that agent.
    /// 2. Exactly one eligible agent (after the
    ///    `eligible_agents_for_workspace` filtering already baked into
    ///    `WorkspaceChoice.allowed_agents`) → launch immediately with it.
    /// 3. Multiple eligible agents and no default → open
    ///    `Modal::AgentPicker` on the manager list and stay in the
    ///    run-loop until the operator commits a choice.
    ///
    /// Returns `Ok(Some(_))` if the caller should break with that
    /// outcome, `Ok(None)` to stay in the run-loop (modal opened, or
    /// there are no eligible agents and we surface a toast). Errors only
    /// when workspace resolution itself fails.
    pub fn dispatch_launch_for_workspace(
        &mut self,
        config: &AppConfig,
        cwd: &std::path::Path,
        idx: usize,
    ) -> anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>> {
        self.selected_workspace = idx;

        let Some(choice) = self.workspaces.get(idx) else {
            return Ok(None);
        };
        let agents = choice.allowed_agents.clone();
        let default_agent = choice.default_agent.clone();

        // Branch 1: default agent set + present in the eligible set → direct launch.
        if let Some(default_key) = default_agent.as_deref()
            && let Some(agent) = agents.iter().find(|a| a.key() == default_key).cloned()
        {
            let workspace =
                preview::resolve_selected_workspace(config, cwd, &self.workspaces[idx], &agent)?;
            return Ok(Some((agent, workspace)));
        }

        // Branch 2: zero or one eligible agent.
        match agents.len() {
            0 => {
                // No eligible agents — toast and stay in the manager list so
                // the operator can edit the workspace's `allowed_agents` or
                // register an agent. Avoids a hard error that would terminate
                // the TUI from a single Enter press.
                let name = self
                    .workspaces
                    .get(idx)
                    .map_or("<unknown>", |choice| choice.name.as_str())
                    .to_string();
                if let ConsoleStage::Manager(ms) = &mut self.stage {
                    ms.toast = Some(crate::console::manager::state::Toast {
                        message: format!("no eligible agents for workspace \"{name}\""),
                        kind: crate::console::manager::state::ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
                Ok(None)
            }
            1 => {
                let agent = agents.into_iter().next().unwrap();
                let workspace = preview::resolve_selected_workspace(
                    config,
                    cwd,
                    &self.workspaces[idx],
                    &agent,
                )?;
                Ok(Some((agent, workspace)))
            }
            _ => {
                // Branch 3: multiple eligible — open the picker overlay.
                if let ConsoleStage::Manager(ms) = &mut self.stage {
                    ms.list_modal = Some(crate::console::manager::state::Modal::AgentPicker {
                        state: crate::console::widgets::agent_picker::AgentPickerState::new(agents),
                    });
                }
                Ok(None)
            }
        }
    }
}

/// Outer event-loop tick interval. 20 Hz keeps the picker's Braille
/// spinner visibly fluid and lets the per-tick worker-channel drain
/// surface `op` results within ~50 ms of the worker finishing — without
/// hot-spinning the CPU. Matched against [`crossterm::event::poll`]'s
/// timeout: when no input arrives within `TICK_MS`, the loop falls
/// through to the next iteration, re-renders, and re-polls. Picked at
/// the brief's recommended balance — tighter (≤16 ms) wastes cycles on
/// idle frames; looser (>100 ms) makes the spinner stutter.
const TICK_MS: u64 = 50;

pub fn run_console(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) -> anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>> {
    use std::time::Duration;

    use crossterm::ExecutableCommand;
    use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
    use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};

    // NOTE: `EnableMouseCapture` intercepts mouse events so the workspace
    // manager can drive a draggable split between the list and details
    // panes. A known side-effect is that the terminal's native text
    // selection (click-drag to select, cmd/ctrl-C to copy) stops working
    // while the TUI is running. Operators who need to copy text from the
    // TUI can hold Shift (Terminal.app, iTerm2) or Option (iTerm2) to
    // bypass capture at the terminal level. A runtime toggle could be
    // added later but is out of scope here.
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            let mut stdout = std::io::stdout();
            let _ = stdout.execute(DisableMouseCapture);
            let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
            let _ = stdout.execute(crossterm::cursor::Show);
        }
    }

    let mut state = ConsoleState::new(&config, cwd)?;
    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    let guard = TerminalGuard;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = loop {
        // Auto-expire manager toasts after 3 seconds.
        if let ConsoleStage::Manager(ms) = &mut state.stage
            && let Some(toast) = &ms.toast
            && toast.shown_at.elapsed() > std::time::Duration::from_secs(3)
        {
            ms.toast = None;
        }

        // Drain pending background-worker results (1Password picker
        // loading state, etc.) BEFORE rendering so a freshly-arrived
        // result lands in this frame rather than a one-tick-stale
        // Loading frame. The render path's `OpPickerState::tick` also
        // drains the channel; both call sites are idempotent on an
        // empty channel.
        if let ConsoleStage::Manager(ms) = &mut state.stage {
            ms.poll_picker_loads();
        }

        // Render the manager. `ConsoleStage` is single-variant today —
        // the legacy full-screen agent picker was replaced by a
        // `Modal::AgentPicker` overlay that the manager render already
        // handles via the list_modal slot.
        if let ConsoleStage::Manager(ms) = &mut state.stage {
            terminal.draw(|frame| manager::render(frame, ms, &config, cwd))?;
        }
        // Capture terminal size before polling for input so the mouse
        // handler can hit-test against the current seam position. Cheap
        // syscall; harmless to call every loop turn. Convert the `Size`
        // into a `Rect` with zero origin so the handler signature stays
        // aligned with ratatui's own area-based hit-test conventions.
        let term_size: ratatui::layout::Rect = terminal.size()?.into();

        // Non-blocking event poll with a tick timeout. When no input
        // arrives within `TICK_MS`, `poll` returns `false` and the loop
        // falls through to the next iteration — keeping the picker
        // spinner advancing and the worker channel draining at 20 Hz
        // regardless of operator input. (A prior blocking
        // `event::read()` froze both updates between keystrokes, making
        // the picker feel unresponsive while `op` was loading.)
        if event::poll(Duration::from_millis(TICK_MS))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let outcome = if let ConsoleStage::Manager(ms) = &mut state.stage {
                        manager::handle_key(ms, &mut config, paths, cwd, key)?
                    } else {
                        manager::InputOutcome::Continue
                    };
                    match outcome {
                        manager::InputOutcome::Continue => {}
                        manager::InputOutcome::ExitJackin => {
                            break Ok(None);
                        }
                        manager::InputOutcome::LaunchNamed(name) => {
                            // Find the workspace by name in ConsoleState.workspaces.
                            if let Some(idx) = state
                                .workspaces
                                .iter()
                                .position(|choice| choice.name == name)
                            {
                                match state.dispatch_launch_for_workspace(&config, cwd, idx) {
                                    Ok(Some(outcome)) => break Ok(Some(outcome)),
                                    Ok(None) => {}
                                    Err(e) => break Err(e),
                                }
                            }
                        }
                        manager::InputOutcome::LaunchCurrentDir => {
                            // Index 0 of ConsoleState.workspaces is the
                            // synthetic "Current directory" choice (built
                            // in ConsoleState::new). Route it through the
                            // same dispatcher as a saved workspace.
                            match state.dispatch_launch_for_workspace(&config, cwd, 0) {
                                Ok(Some(outcome)) => break Ok(Some(outcome)),
                                Ok(None) => {}
                                Err(e) => break Err(e),
                            }
                        }
                        manager::InputOutcome::LaunchWithAgent(agent) => {
                            // The `AgentPicker` modal just committed. The
                            // dispatcher pinned `selected_workspace` when
                            // it opened the picker, so resolve against
                            // that. Should be unreachable when the index
                            // is missing — the dispatcher validated it on
                            // open — but fall back to staying in the
                            // run-loop rather than panicking on a state-
                            // machine inconsistency.
                            let idx = state.selected_workspace;
                            if let Some(choice) = state.workspaces.get(idx) {
                                match preview::resolve_selected_workspace(
                                    &config, cwd, choice, &agent,
                                ) {
                                    Ok(workspace) => break Ok(Some((agent, workspace))),
                                    Err(e) => break Err(e),
                                }
                            }
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    // Only the Manager/List stage consumes mouse events
                    // today (list/details seam drag). Modals on other
                    // stages fall through as silent no-ops.
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        manager::input::handle_mouse(ms, mouse, term_size);
                    }
                }
                _ => {}
            }
        }
        // No `else` — when `poll` times out, fall through to the next
        // loop turn so the spinner ticks and channels drain.
    };

    drop(guard);
    result
}
