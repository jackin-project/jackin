//! Renderable stories for shared TUI components.
//!
//! The docs/SVG generator can consume this module later; keeping the stories
//! beside the components means examples render the real widgets instead of a
//! hand-drawn copy.

use ratatui::{
    Frame, Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::{Block, Borders},
};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use crate::components::{
    ButtonStrip, ButtonStripItem, ConfirmState, ErrorPopupState, Panel, PanelFocus,
    SaveDiscardState, StatusPopupState, TabStrip, render_confirm_dialog, render_error_dialog,
    render_save_discard_dialog, render_status_popup,
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
        Story::new("tab-strip/basic", "Tab strip", 54, 2, story_tab_strip),
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
        Story::new(
            "save-discard/default",
            "Save/discard dialog",
            54,
            7,
            story_save_discard_default,
        ),
        Story::new(
            "status-popup/default",
            "Status popup",
            48,
            7,
            story_status_popup_default,
        ),
    ]
}

#[must_use]
pub fn render_story_to_text(story: Story) -> String {
    let buffer = render_story_to_buffer(story);
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

#[must_use]
pub fn render_story_to_svg(story: Story) -> String {
    let buffer = render_story_to_buffer(story);
    buffer_to_svg(&buffer, story.title)
}

#[must_use]
pub fn story_svg_filename(story: Story) -> String {
    format!("{}.svg", story.id.replace('/', "-"))
}

pub fn write_story_svgs(out_dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;
    let mut paths = Vec::new();
    for story in stories() {
        let path = out_dir.join(story_svg_filename(story));
        fs::write(&path, render_story_to_svg(story))?;
        paths.push(path);
    }
    Ok(paths)
}

fn render_story_to_buffer(story: Story) -> Buffer {
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
    terminal.backend().buffer().clone()
}

fn buffer_to_svg(buffer: &Buffer, title: &str) -> String {
    const CELL_W: u16 = 9;
    const CELL_H: u16 = 18;
    const BASELINE: u16 = 14;

    let area = buffer.area;
    let width = area.width.saturating_mul(CELL_W);
    let height = area.height.saturating_mul(CELL_H);
    let mut out = String::new();
    out.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="{}">"##,
        escape_xml(title)
    ));
    out.push_str(r##"<rect width="100%" height="100%" fill="#000000"/>"##);
    out.push_str(r#"<g font-family="ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" font-size="14">"#);

    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            let px = x.saturating_mul(CELL_W);
            let py = y.saturating_mul(CELL_H);
            let bg = color_to_css(cell.bg);
            if bg != "#000000" {
                out.push_str(&format!(
                    r#"<rect x="{px}" y="{py}" width="{CELL_W}" height="{CELL_H}" fill="{bg}"/>"#
                ));
            }
            let symbol = cell.symbol();
            if !symbol.trim().is_empty() {
                let fg = color_to_css(cell.fg);
                let text_y = py.saturating_add(BASELINE);
                out.push_str(&format!(
                    r#"<text x="{px}" y="{text_y}" fill="{fg}">{}</text>"#,
                    escape_xml(symbol)
                ));
            }
        }
    }
    out.push_str("</g></svg>\n");
    out
}

fn color_to_css(color: Color) -> &'static str {
    match color {
        Color::Black => "#000000",
        Color::Red => "#ff0000",
        Color::Green => "#00ff41",
        Color::Yellow => "#ffd85e",
        Color::Blue => "#0050b4",
        Color::Magenta => "#ff00ff",
        Color::Cyan => "#00ffff",
        Color::Gray | Color::DarkGray => "#808080",
        Color::LightRed => "#ff5e7a",
        Color::LightGreen => "#00ff41",
        Color::LightYellow => "#ffd85e",
        Color::LightBlue => "#7aa2ff",
        Color::LightMagenta => "#ff7aff",
        Color::LightCyan => "#7affff",
        Color::White => "#ffffff",
        Color::Rgb(0, 255, 65) => "#00ff41",
        Color::Rgb(0, 140, 30) => "#008c1e",
        Color::Rgb(0, 80, 18) => "#005012",
        Color::Rgb(255, 94, 122) => "#ff5e7a",
        Color::Rgb(255, 216, 94) => "#ffd85e",
        Color::Rgb(0, 80, 180) => "#0050b4",
        Color::Rgb(204, 92, 0) => "#cc5c00",
        Color::Rgb(80, 80, 80) => "#505050",
        Color::Rgb(_, _, _) | Color::Indexed(_) | Color::Reset => "#ffffff",
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

fn story_tab_strip(frame: &mut Frame<'_>, area: Rect) {
    let labels = [
        ("General", true),
        ("Mounts", false),
        ("Roles", false),
        ("Secrets", false),
    ];
    TabStrip::new(&labels)
        .focused(true)
        .hovered(Some(2))
        .render(frame, area);
}

fn story_confirm_default(frame: &mut Frame<'_>, area: Rect) {
    let state = ConfirmState::new("Delete workspace?");
    render_confirm_dialog(frame, area, &state);
}

fn story_error_default(frame: &mut Frame<'_>, area: Rect) {
    let state = ErrorPopupState::new("Launch failed", "Derived image build failed.");
    render_error_dialog(frame, area, &state);
}

fn story_save_discard_default(frame: &mut Frame<'_>, area: Rect) {
    let state = SaveDiscardState::new("Save changes before leaving?");
    render_save_discard_dialog(frame, area, &state);
}

fn story_status_popup_default(frame: &mut Frame<'_>, area: Rect) {
    let state = StatusPopupState::new("Loading role", "Resolving role source...");
    render_status_popup(frame, area, &state);
}
