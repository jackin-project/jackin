mod input;
pub mod manager;
mod preview;
mod render;
pub mod state;
pub mod widgets;

pub use state::ConsoleStage;
pub use state::ConsoleState;
pub use state::WorkspaceChoice;

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use crate::workspace::ResolvedWorkspace;

/// Shared `LaunchNamed` / `LaunchCurrentDir` transition: preselect the workspace
/// at `idx` in `ConsoleState::workspaces`, compute filtered agents, and either
/// short-circuit (single agent → immediate launch outcome) or stage the
/// agent-picker. Returns `Ok(Some(_))` if the caller should break with that
/// outcome, `Ok(None)` to stay in the run-loop.
fn transition_to_agent_stage(
    config: &AppConfig,
    cwd: &std::path::Path,
    state: &mut ConsoleState,
    idx: usize,
) -> anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>> {
    state.selected_workspace = idx;
    state.agent_query.clear();

    let agents = state.filtered_agents();
    if agents.is_empty() {
        let name = state
            .workspaces
            .get(idx)
            .map_or("<unknown>", |choice| choice.name.as_str());
        anyhow::bail!("no eligible agents for workspace {name}");
    }
    if agents.len() == 1 {
        let agent = agents[0].clone();
        let workspace = preview::resolve_selected_workspace(
            config,
            cwd,
            &state.workspaces[state.selected_workspace],
            &agent,
        )?;
        return Ok(Some((agent, workspace)));
    }

    let choice = &state.workspaces[state.selected_workspace];
    state.selected_agent = crate::app::context::preferred_agent_index(
        &agents,
        choice.last_agent.as_deref(),
        choice.default_agent.as_deref(),
    )
    .unwrap_or(0);
    state.stage = ConsoleStage::Agent;
    Ok(None)
}

pub fn run_console(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) -> anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>> {
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

        terminal.draw(|frame| match &state.stage {
            ConsoleStage::Agent => render::draw_agent_screen(frame, &state, &config, cwd),
            ConsoleStage::Manager(ms) => manager::render(frame, ms, &config, cwd),
        })?;
        // Capture terminal size before the blocking read so the mouse
        // handler can hit-test against the current seam position. Harmless
        // to call every loop turn — it's a cheap syscall and the render
        // path already needs the size via the Frame abstraction. Convert
        // the `Size` into a `Rect` with zero origin so the handler signature
        // stays aligned with ratatui's own area-based hit-test conventions.
        let term_size: ratatui::layout::Rect = terminal.size()?.into();
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if matches!(state.stage, ConsoleStage::Manager(_)) {
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        match manager::handle_key(ms, &mut config, paths, cwd, key)? {
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
                                    match transition_to_agent_stage(&config, cwd, &mut state, idx) {
                                        Ok(Some(outcome)) => break Ok(Some(outcome)),
                                        Ok(None) => {}
                                        Err(e) => break Err(e),
                                    }
                                }
                            }
                            manager::InputOutcome::LaunchCurrentDir => {
                                // Index 0 of ConsoleState.workspaces is the synthetic
                                // "Current directory" choice (built in
                                // ConsoleState::new). Route it through the same
                                // agent-picker path as a saved workspace.
                                match transition_to_agent_stage(&config, cwd, &mut state, 0) {
                                    Ok(Some(outcome)) => break Ok(Some(outcome)),
                                    Ok(None) => {}
                                    Err(e) => break Err(e),
                                }
                            }
                        }
                    }
                } else {
                    match input::handle_event(&mut state, key.code, &config, cwd) {
                        input::EventOutcome::Continue => {}
                        input::EventOutcome::Exit(outcome) => break outcome,
                    }
                }
            }
            Event::Mouse(mouse) => {
                // Only the Manager/List stage consumes mouse events today
                // (list/details seam drag). Agent-stage + modals on other
                // stages fall through as silent no-ops.
                if let ConsoleStage::Manager(ms) = &mut state.stage {
                    manager::input::handle_mouse(ms, mouse, term_size);
                }
            }
            _ => {}
        }
    };

    drop(guard);
    result
}
