//! Renderable stories for shared TUI components.
//!
//! The docs/SVG generator can consume this module later; keeping the stories
//! beside the components means examples render the real widgets instead of a
//! hand-drawn copy.

use ratatui::{
    Frame, Terminal,
    backend::TestBackend,
    layout::Rect,
    widgets::{Block, Borders},
};

use crate::components::{
    ButtonStrip, ButtonStripItem, ConfirmState, ErrorPopupState, Panel, PanelFocus,
    render_confirm_dialog, render_error_dialog,
};

#[derive(Debug, Clone, Copy)]
pub struct Story {
    pub id: &'static str,
    pub title: &'static str,
    pub width: u16,
    pub height: u16,
    render: fn(&mut Frame<'_>, Rect),
}

impl Story {
    #[must_use]
    pub const fn new(
        id: &'static str,
        title: &'static str,
        width: u16,
        height: u16,
        render: fn(&mut Frame<'_>, Rect),
    ) -> Self {
        Self {
            id,
            title,
            width,
            height,
            render,
        }
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        (self.render)(frame, area);
    }
}

#[must_use]
pub fn stories() -> Vec<Story> {
    vec![
        Story::new("panel/focused", "Panel focused", 42, 7, story_panel_focused),
        Story::new(
            "button-strip/basic",
            "Button strip",
            42,
            3,
            story_button_strip,
        ),
        Story::new(
            "confirm/default",
            "Confirm dialog",
            48,
            7,
            story_confirm_default,
        ),
        Story::new(
            "error/default",
            "Error dialog",
            54,
            9,
            story_error_default,
        ),
    ]
}

#[must_use]
pub fn render_story_to_text(story: Story) -> String {
    let backend = TestBackend::new(story.width, story.height);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => match error {},
    };
    match terminal.draw(|frame| {
        let area = frame.area();
        story.render(frame, area);
    }) {
        Ok(_) => {}
        Err(error) => match error {},
    }
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..story.height {
        for x in 0..story.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        if y + 1 < story.height {
            out.push('\n');
        }
    }
    out
}

fn story_panel_focused(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Panel::new()
            .title("Workspace")
            .focus(PanelFocus::FocusedScrollable)
            .block(),
        area,
    );
}

fn story_button_strip(frame: &mut Frame<'_>, area: Rect) {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let items = [
        ButtonStripItem::new("Save"),
        ButtonStripItem::new("Discard"),
        ButtonStripItem::new("Cancel"),
    ];
    ButtonStrip::new(&items).focused(1).render(frame, inner);
}

fn story_confirm_default(frame: &mut Frame<'_>, area: Rect) {
    let state = ConfirmState::new("Delete workspace?");
    render_confirm_dialog(frame, area, &state);
}

fn story_error_default(frame: &mut Frame<'_>, area: Rect) {
    let state = ErrorPopupState::new("Launch failed", "Derived image build failed.");
    render_error_dialog(frame, area, &state);
}
