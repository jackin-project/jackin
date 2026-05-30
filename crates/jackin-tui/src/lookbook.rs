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
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use crate::components::{
    BrandHeader, ButtonStrip, ButtonStripItem, ConfirmState, ErrorPopupState, FilterInput, HintBar,
    Panel, PanelFocus, SaveDiscardFocus, SaveDiscardState, SelectList, SelectListState,
    StatusFooter, StatusPopupState, TabStrip, TextInput, TextInputState, render_confirm_dialog,
    render_error_dialog, render_save_discard_dialog, render_scrollable_block, render_status_popup,
};
use crate::{
    HintSpan,
    theme::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE},
};

#[derive(Debug, Clone, Copy)]
pub struct Story {
    pub id: &'static str,
    pub title: &'static str,
    pub component: &'static str,
    pub description: &'static str,
    pub width: u16,
    pub height: u16,
    render: fn(&mut Frame<'_>, Rect),
}

impl Story {
    #[must_use]
    pub const fn new(
        id: &'static str,
        title: &'static str,
        component: &'static str,
        description: &'static str,
        width: u16,
        height: u16,
        render: fn(&mut Frame<'_>, Rect),
    ) -> Self {
        Self {
            id,
            title,
            component,
            description,
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
        Story::new(
            "brand-header/console",
            "Brand header",
            "BrandHeader",
            "Console row-0 brand pill with current surface label.",
            54,
            1,
            story_brand_header_console,
        ),
        Story::new(
            "panel/focused",
            "Panel focused",
            "Panel",
            "Focused workspace summary panel with realistic rows.",
            54,
            8,
            story_panel_focused,
        ),
        Story::new(
            "button-strip/basic",
            "Button strip",
            "ButtonStrip",
            "Save flow actions with one focused and one disabled button.",
            54,
            3,
            story_button_strip,
        ),
        Story::new(
            "tab-strip/basic",
            "Tab strip",
            "TabStrip",
            "Workspace editor tabs with active, inactive, and hovered state.",
            58,
            2,
            story_tab_strip,
        ),
        Story::new(
            "confirm/default",
            "Confirm dialog",
            "ConfirmDialog",
            "Destructive workspace-delete confirmation.",
            48,
            7,
            story_confirm_default,
        ),
        Story::new(
            "confirm/role-trust",
            "Role trust dialog",
            "ConfirmDialog",
            "Role-source trust confirmation with structured role and repository fields.",
            70,
            11,
            story_confirm_role_trust,
        ),
        Story::new(
            "error/default",
            "Error dialog",
            "ErrorDialog",
            "Launch failure modal with acknowledged OK action.",
            62,
            9,
            story_error_default,
        ),
        Story::new(
            "save-discard/default",
            "Save/discard dialog",
            "SaveDiscardDialog",
            "Dirty workspace editor exit with Cancel focused by default.",
            54,
            7,
            story_save_discard_default,
        ),
        Story::new(
            "status-popup/default",
            "Status popup",
            "StatusPopup",
            "Non-interactive role-resolution progress popup.",
            48,
            7,
            story_status_popup_default,
        ),
        Story::new(
            "filter-input/populated",
            "Filter input",
            "FilterInput",
            "Picker filter row with typed query and visible cursor.",
            42,
            1,
            story_filter_input_populated,
        ),
        Story::new(
            "hint-bar/manager-footer",
            "Hint bar",
            "HintBar",
            "Wrapped manager footer shortcuts grouped by action.",
            54,
            2,
            story_hint_bar_manager_footer,
        ),
        Story::new(
            "select-list/agent-picker",
            "Select list",
            "SelectList",
            "Agent picker with context copy and selected row.",
            58,
            11,
            story_select_list_agent_picker,
        ),
        Story::new(
            "scrollable-panel/mounts",
            "Scrollable panel",
            "ScrollablePanel",
            "Mount table that overflows both axes and shows scrollbars.",
            64,
            9,
            story_scrollable_panel_mounts,
        ),
        Story::new(
            "status-footer/launch-progress",
            "Status footer",
            "StatusFooter",
            "White launch status footer with instance and debug chips.",
            72,
            1,
            story_status_footer_launch_progress,
        ),
        Story::new(
            "text-input/workspace-name",
            "Text input",
            "TextInput",
            "Workspace rename dialog with current value and cursor.",
            58,
            7,
            story_text_input_workspace_name,
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
        // Pre-fill with black so cells not touched by the story match the
        // real terminal background instead of appearing as Color::Reset.
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Black)),
            area,
        );
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
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="{}" style="background:#000000">"##,
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
        Color::Reset => "#000000",
        Color::Rgb(_, _, _) | Color::Indexed(_) => "#ffffff",
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn story_brand_header_console(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(BrandHeader::new("Console · workspace editor"), area);
}

fn story_panel_focused(frame: &mut Frame<'_>, area: Rect) {
    let block = Panel::new()
        .title("Workspace")
        .focus(PanelFocus::FocusedScrollable)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
                Span::styled("jackin-core", Style::default().fg(PHOSPHOR_GREEN)),
            ]),
            Line::from(vec![
                Span::styled("Role: ", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
                Span::styled("github.com/jackin-project/roles/rust", Style::default().fg(PHOSPHOR_GREEN)),
            ]),
            Line::from(vec![
                Span::styled("Agent: ", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
                Span::styled("codex", Style::default().fg(PHOSPHOR_GREEN)),
            ]),
            Line::from(vec![
                Span::styled("Mounts: ", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
                Span::styled("repo rw, ~/.config/gh ro", Style::default().fg(PHOSPHOR_DIM)),
            ]),
        ]),
        inner,
    );
}

fn story_button_strip(frame: &mut Frame<'_>, area: Rect) {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let items = [
        ButtonStripItem::new("Save"),
        ButtonStripItem::new("Discard"),
        ButtonStripItem::disabled("Launch"),
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
    let state = ConfirmState::new("Delete workspace \"jackin-core\"?\nThis removes the saved workspace entry.");
    render_confirm_dialog(frame, area, &state);
}

fn story_confirm_role_trust(frame: &mut Frame<'_>, area: Rect) {
    let state = ConfirmState::role_trust(
        "rust",
        "https://github.com/jackin-project/roles",
    );
    render_confirm_dialog(frame, area, &state);
}

fn story_error_default(frame: &mut Frame<'_>, area: Rect) {
    let state = ErrorPopupState::new(
        "Launch failed",
        "Derived image build failed while installing role dependencies.\nOpen diagnostics run jk-run-3d7e23 for the full log.",
    );
    render_error_dialog(frame, area, &state);
}

fn story_save_discard_default(frame: &mut Frame<'_>, area: Rect) {
    let mut state = SaveDiscardState::new("Save workspace changes before leaving?");
    state.focus = SaveDiscardFocus::Cancel;
    render_save_discard_dialog(frame, area, &state);
}

fn story_status_popup_default(frame: &mut Frame<'_>, area: Rect) {
    let state = StatusPopupState::new("Loading role", "Resolving github:jackin-project/roles/rust");
    render_status_popup(frame, area, &state);
}

fn story_filter_input_populated(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(FilterInput::new("cod"), area);
}

fn story_hint_bar_manager_footer(frame: &mut Frame<'_>, area: Rect) {
    let spans = [
        HintSpan::Key("↑↓"),
        HintSpan::Text("select"),
        HintSpan::Sep,
        HintSpan::Key("Enter"),
        HintSpan::Text("open"),
        HintSpan::Sep,
        HintSpan::Key("D"),
        HintSpan::Text("delete"),
        HintSpan::GroupSep,
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::Sep,
        HintSpan::Key("Esc"),
        HintSpan::Text("back"),
    ];
    frame.render_widget(HintBar::new(&spans).wrapped(), area);
}

fn story_select_list_agent_picker(frame: &mut Frame<'_>, area: Rect) {
    let mut state = SelectListState::new(vec![
        "claude".to_owned(),
        "codex".to_owned(),
        "amp".to_owned(),
        "kimi".to_owned(),
        "opencode".to_owned(),
    ]);
    state.select_index(1);
    let context = [
        Line::from(vec![
            Span::styled("Workspace: ", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
            Span::styled("jackin-core", Style::default().fg(PHOSPHOR_GREEN)),
        ]),
        Line::from(vec![
            Span::styled("Role: ", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
            Span::styled("rust", Style::default().fg(PHOSPHOR_GREEN)),
        ]),
    ];
    frame.render_widget(SelectList::new(&state, "Choose agent").context(&context), area);
}

fn story_scrollable_panel_mounts(frame: &mut Frame<'_>, area: Rect) {
    let lines = vec![
        Line::from("repo               /workspace/jackin-project/jackin                       rw"),
        Line::from("github-cli         /jackin/host/config/gh                             ro"),
        Line::from("codex              /jackin/codex                                      ro"),
        Line::from("claude             /jackin/claude                                     ro"),
        Line::from("cache              /jackin/host/cache/cargo                           rw"),
        Line::from("socket             /jackin/run/jackin.sock                            rw"),
        Line::from("role-manifest      /workspace/jackin.role.toml                         ro"),
        Line::from("diagnostics        /jackin/state/diagnostics/jk-run-3d7e23             rw"),
        Line::from("ssh                /jackin/host/ssh                                   ro"),
        Line::from("op-session         /jackin/host/config/op                             ro"),
    ];
    let mut scroll_x = 12;
    let mut scroll_y = 2;
    render_scrollable_block(
        frame,
        area,
        lines,
        &mut scroll_x,
        &mut scroll_y,
        true,
        Some("Global mounts"),
    );
}

fn story_status_footer_launch_progress(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        StatusFooter::new("Building role image: rust-dev")
            .right("s7f8a2c1")
            .right_debug(Some("jk-run-3d7e23"))
            .left_hover(true),
        area,
    );
}

fn story_text_input_workspace_name(frame: &mut Frame<'_>, area: Rect) {
    let state = TextInputState::new("Workspace name", "jackin-core");
    frame.render_widget(TextInput::new(&state), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_exported_component_has_a_story() {
        let expected = BTreeSet::from([
            "BrandHeader",
            "ButtonStrip",
            "ConfirmDialog",
            "ErrorDialog",
            "FilterInput",
            "HintBar",
            "Panel",
            "SaveDiscardDialog",
            "ScrollablePanel",
            "SelectList",
            "StatusFooter",
            "StatusPopup",
            "TabStrip",
            "TextInput",
        ]);
        let actual: BTreeSet<&str> = stories().into_iter().map(|story| story.component).collect();

        assert_eq!(actual, expected);
    }
}
