mod input;
pub mod manager;
mod preview;
mod render;
pub mod state;
pub mod widgets;

pub use state::LaunchStage;
pub use state::LaunchState;
pub use state::WorkspaceChoice;

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use crate::workspace::ResolvedWorkspace;

pub fn run_launch(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) -> anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>> {
    use crossterm::ExecutableCommand;
    use crossterm::event::{self, Event, KeyEventKind};
    use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};

    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            let mut stdout = std::io::stdout();
            let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
            let _ = stdout.execute(crossterm::cursor::Show);
        }
    }

    let mut state = LaunchState::new(&config, cwd)?;
    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    let guard = TerminalGuard;
    stdout.execute(EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = loop {
        terminal.draw(|frame| match &state.stage {
            LaunchStage::Workspace => render::draw_workspace_screen(frame, &state),
            LaunchStage::Agent => render::draw_agent_screen(frame, &state, &config, cwd),
            LaunchStage::Manager(ms) => manager::render(frame, ms),
        })?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            if matches!(state.stage, LaunchStage::Manager(_)) {
                if let LaunchStage::Manager(ms) = &mut state.stage {
                    match manager::handle_key(ms, &mut config, paths, key)? {
                        manager::InputOutcome::Continue => {}
                        manager::InputOutcome::ExitToLauncher => {
                            state = LaunchState::new(&config, cwd)?;
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
    };

    drop(guard);
    result
}
