mod input;
mod preview;
mod render;
pub mod state;

pub use state::LaunchStage;
pub use state::LaunchState;
pub use state::WorkspaceChoice;

use crate::config::AppConfig;
use crate::selector::ClassSelector;
use crate::workspace::ResolvedWorkspace;

pub fn run_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
) -> anyhow::Result<(ClassSelector, ResolvedWorkspace)> {
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

    let mut state = LaunchState::new(config, cwd)?;
    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    let guard = TerminalGuard;
    stdout.execute(EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = loop {
        terminal.draw(|frame| match state.stage {
            LaunchStage::Workspace => render::draw_workspace_screen(frame, &state),
            LaunchStage::Agent => render::draw_agent_screen(frame, &state, config, cwd),
        })?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match input::handle_event(&mut state, key.code, config, cwd) {
                input::EventOutcome::Continue => {}
                input::EventOutcome::Exit(outcome) => break outcome,
            }
        }
    };

    drop(guard);
    result
}
