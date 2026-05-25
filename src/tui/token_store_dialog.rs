//! Standalone TUI dialog for interactive 1Password token-storage selection.
//!
//! Launched from `jackin workspace claude-token setup --interactive` when
//! `--vault` is not supplied. Initialises a raw-mode alternate-screen
//! terminal, runs the [`TokenStorePickerState`] event loop, then restores
//! the terminal before returning the operator's selection.

use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyEventState, KeyModifiers, MouseEventKind,
};
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

use crate::console::widgets::ModalOutcome;
use crate::console::widgets::token_store_picker::{
    TokenStorePickerState, TokenStoreSelection, render,
};

/// Guard that restores the terminal on drop, even on early return/panic.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = stdout.execute(DisableMouseCapture);
        let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
        let _ = stdout.execute(crossterm::cursor::Show);
    }
}

fn picker_area(full: Rect) -> Rect {
    // Centre a modal that is ~80% wide and ~80% tall.
    let h_margin = full.width / 10;
    let v_margin = full.height / 10;
    Rect {
        x: full.x + h_margin,
        y: full.y + v_margin,
        width: full.width.saturating_sub(h_margin * 2),
        height: full.height.saturating_sub(v_margin * 2),
    }
}

/// Run the token-store picker as a standalone TUI dialog.
///
/// `workspace` is used to build the default item-name suggestion
/// (`jackin · {ws} · claude-token`). `account` pins the 1Password
/// account for all underlying `op` queries.
///
/// Returns `Err` if the operator cancels (Esc) or the terminal cannot
/// be initialised.
pub fn run(workspace: &str, _account: Option<&str>) -> anyhow::Result<TokenStoreSelection> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        anyhow::bail!("--interactive requires a TTY. Pass --vault <name> for non-interactive use.");
    }

    let item_name_default =
        crate::workspace::token_setup::DEFAULT_ITEM_TEMPLATE.replace("{ws}", workspace);

    enable_raw_mode()?;
    let _guard = TerminalGuard;
    let mut stdout = std::io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut picker = TokenStorePickerState::new(&item_name_default);

    loop {
        picker.tick();

        terminal.draw(|frame| {
            let area = picker_area(frame.area());
            render::render(frame, area, &picker);
        })?;

        if !event::poll(std::time::Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        match ev {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match picker.handle_key(key) {
                    ModalOutcome::Commit(selection) => return Ok(selection),
                    ModalOutcome::Cancel => {
                        anyhow::bail!("token storage selection cancelled");
                    }
                    ModalOutcome::Continue => {}
                }
            }
            Event::Mouse(mouse) => {
                let code = match mouse.kind {
                    MouseEventKind::ScrollUp => KeyCode::Up,
                    MouseEventKind::ScrollDown => KeyCode::Down,
                    _ => continue,
                };
                picker.handle_key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::NONE,
                    kind: KeyEventKind::Press,
                    state: KeyEventState::NONE,
                });
            }
            _ => {}
        }
    }
}
