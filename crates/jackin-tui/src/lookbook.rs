//! Renderable stories for shared TUI components.
//!
//! The docs/SVG generator can consume this module later; keeping the stories
//! beside the components means examples render the real widgets instead of a
//! hand-drawn copy.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
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
    fs, io,
    path::{Path, PathBuf},
};

use crate::components::{
    BrandHeader, ButtonStrip, ButtonStripItem, ConfirmState, ErrorPopupState, FilterInput, HintBar,
    Panel, PanelFocus, SaveDiscardFocus, SaveDiscardState, SelectList, SelectListState,
    StatusFooter, StatusPopupState, TabStrip, TextInput, TextInputState, render_confirm_dialog,
    render_error_dialog, render_save_discard_dialog, render_scrollable_block, render_status_popup,
};
use crate::{
    HintSpan, lay_out_tabs,
    theme::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE},
};

// ── Interactive story trait ───────────────────────────────────────────────────

/// Interactive preview state for a story. The terminal lookbook holds one
/// of these per-story-selection and forwards events to it.
pub trait StoryInteraction {
    /// Render the component with current live state into `area`.
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect);
    /// Handle a keyboard event. Returns true if consumed.
    fn handle_key(&mut self, key: KeyEvent) -> bool;
    /// Handle a mouse event relative to the full terminal. `preview_area` is
    /// the Rect the preview occupies so the story can hit-test relative coords.
    fn handle_mouse(&mut self, mouse: MouseEvent, preview_area: Rect) -> bool;
}

// ── StaticStory: no-op wrapper for fn-pointer stories ────────────────────────

struct StaticStory {
    render_fn: fn(&mut Frame<'_>, Rect),
}

impl StoryInteraction for StaticStory {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        (self.render_fn)(frame, area);
    }

    fn handle_key(&mut self, _key: KeyEvent) -> bool {
        false
    }

    fn handle_mouse(&mut self, _mouse: MouseEvent, _preview_area: Rect) -> bool {
        false
    }
}

// ── TabStrip interactor ───────────────────────────────────────────────────────

struct TabStripInteractor {
    labels: Vec<(&'static str, bool)>,
    selected: usize,
    hovered: Option<usize>,
}

impl TabStripInteractor {
    fn new() -> Self {
        Self {
            labels: vec![
                ("General", true),
                ("Mounts", false),
                ("Roles", false),
                ("Secrets", false),
            ],
            selected: 0,
            hovered: None,
        }
    }

    fn set_selected(&mut self, idx: usize) {
        if idx < self.labels.len() {
            for (i, (_, active)) in self.labels.iter_mut().enumerate() {
                *active = i == idx;
            }
            self.selected = idx;
        }
    }

    /// Return which tab index the column falls in, using the same cell geometry
    /// that `lay_out_tabs` / `TabStrip` produce.
    fn tab_at_col(&self, col: u16) -> Option<usize> {
        let cells = lay_out_tabs(&self.labels, 0);
        for (idx, cell) in cells.iter().enumerate() {
            let end = cell.start_col + cell.cell_cols;
            if col >= cell.start_col && col < end {
                return Some(idx);
            }
        }
        None
    }
}

impl StoryInteraction for TabStripInteractor {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        TabStrip::new(&self.labels)
            .focused(true)
            .hovered(self.hovered)
            .render(frame, area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Left => {
                let next = self.selected.saturating_sub(1);
                self.set_selected(next);
                true
            }
            KeyCode::Right => {
                let next = (self.selected + 1).min(self.labels.len().saturating_sub(1));
                self.set_selected(next);
                true
            }
            _ => false,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, preview_area: Rect) -> bool {
        // The tab strip occupies row 0 of the preview area (first content row).
        if mouse.row != preview_area.y {
            self.hovered = None;
            return false;
        }
        let col_in_preview = mouse.column.saturating_sub(preview_area.x);
        let tab_idx = self.tab_at_col(col_in_preview);
        match mouse.kind {
            MouseEventKind::Moved => {
                self.hovered = tab_idx;
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(idx) = tab_idx {
                    self.set_selected(idx);
                    self.hovered = Some(idx);
                }
                true
            }
            _ => false,
        }
    }
}

// ── SelectList interactor ─────────────────────────────────────────────────────

struct SelectListInteractor {
    state: SelectListState,
    context: Vec<Line<'static>>,
}

impl SelectListInteractor {
    fn new() -> Self {
        let mut state = SelectListState::new(vec![
            "claude".to_owned(),
            "codex".to_owned(),
            "amp".to_owned(),
            "kimi".to_owned(),
            "opencode".to_owned(),
        ]);
        state.select_index(1);
        let context = vec![
            Line::from(vec![
                Span::styled(
                    "Workspace: ",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
                Span::styled("jackin-core", Style::default().fg(PHOSPHOR_GREEN)),
            ]),
            Line::from(vec![
                Span::styled(
                    "Role: ",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
                Span::styled("rust", Style::default().fg(PHOSPHOR_GREEN)),
            ]),
        ];
        Self { state, context }
    }
}

impl StoryInteraction for SelectListInteractor {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(
            SelectList::new(&self.state, "Choose agent").context(&self.context),
            area,
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        // Delegate to SelectListState and report consumed for all keys it acts on.
        self.state.handle_key(key);
        matches!(
            key.code,
            KeyCode::Up | KeyCode::Down | KeyCode::Enter | KeyCode::Char(_) | KeyCode::Backspace
        )
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, preview_area: Rect) -> bool {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return false;
        }
        // The SelectList renders a block with 1-cell border, then a filter row,
        // a blank separator, then `context.len()` context lines, then another
        // blank, then list items. That's 1 (border) + 1 (filter) + 1 (sep) +
        // context_count + 1 (sep) = header rows before items.
        let header_rows = 1u16 + 1 + 1 + self.context.len() as u16 + 1;
        if mouse.row < preview_area.y + header_rows {
            return false;
        }
        let item_row = usize::from(mouse.row - preview_area.y - header_rows);
        self.state.select_index(item_row);
        true
    }
}

// ── ScrollablePanel interactor ────────────────────────────────────────────────

struct ScrollablePanelInteractor {
    scroll_x: u16,
    scroll_y: u16,
}

impl ScrollablePanelInteractor {
    fn new() -> Self {
        Self {
            scroll_x: 12,
            scroll_y: 2,
        }
    }

    fn lines() -> Vec<Line<'static>> {
        vec![
            Line::from(
                "repo               /workspace/jackin-project/jackin                       rw",
            ),
            Line::from("github-cli         /jackin/host/config/gh                             ro"),
            Line::from("codex              /jackin/codex                                      ro"),
            Line::from("claude             /jackin/claude                                     ro"),
            Line::from("cache              /jackin/host/cache/cargo                           rw"),
            Line::from("socket             /jackin/run/jackin.sock                            rw"),
            Line::from("role-manifest      /workspace/jackin.role.toml                         ro"),
            Line::from("diagnostics        /jackin/state/diagnostics/jk-run-3d7e23             rw"),
            Line::from("ssh                /jackin/host/ssh                                   ro"),
            Line::from("op-session         /jackin/host/config/op                             ro"),
        ]
    }
}

impl StoryInteraction for ScrollablePanelInteractor {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        render_scrollable_block(
            frame,
            area,
            Self::lines(),
            &mut self.scroll_x,
            &mut self.scroll_y,
            true,
            Some("Global mounts"),
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        let line_count = Self::lines().len();
        match key.code {
            KeyCode::Up => {
                crate::components::scrollable_panel::apply_scroll_delta(
                    &mut self.scroll_y,
                    -1,
                    10,
                    line_count,
                );
                true
            }
            KeyCode::Down => {
                crate::components::scrollable_panel::apply_scroll_delta(
                    &mut self.scroll_y,
                    1,
                    10,
                    line_count,
                );
                true
            }
            _ => false,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, _preview_area: Rect) -> bool {
        let line_count = Self::lines().len();
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                crate::components::scrollable_panel::apply_scroll_delta(
                    &mut self.scroll_y,
                    -1,
                    10,
                    line_count,
                );
                true
            }
            MouseEventKind::ScrollDown => {
                crate::components::scrollable_panel::apply_scroll_delta(
                    &mut self.scroll_y,
                    1,
                    10,
                    line_count,
                );
                true
            }
            _ => false,
        }
    }
}

// ── ConfirmDialog interactor ──────────────────────────────────────────────────

struct ConfirmInteractor {
    state: ConfirmState,
}

impl ConfirmInteractor {
    fn default_story() -> Self {
        Self {
            state: ConfirmState::new(
                "Delete workspace \"jackin-core\"?\nThis removes the saved workspace entry.",
            ),
        }
    }

    fn role_trust_story() -> Self {
        Self {
            state: ConfirmState::role_trust("rust", "https://github.com/jackin-project/roles"),
        }
    }
}

impl StoryInteraction for ConfirmInteractor {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        render_confirm_dialog(frame, area, &self.state);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        self.state.handle_key(key);
        true
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, preview_area: Rect) -> bool {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return false;
        }
        // Buttons are rendered in the last row of the dialog area. The dialog
        // fills `preview_area`, so the button row is the last row.
        let button_row = preview_area.y + preview_area.height.saturating_sub(1);
        if mouse.row != button_row {
            return false;
        }
        // ButtonStrip centres two buttons. Use column midpoint as a heuristic:
        // columns in the left half → Yes, right half → No.
        let mid = preview_area.x + preview_area.width / 2;
        if mouse.column < mid {
            self.state = self.state.clone().with_focus_yes();
        } else {
            use crate::components::ConfirmFocus;
            self.state.focus = ConfirmFocus::No;
        }
        true
    }
}

// ── TextInput interactor ──────────────────────────────────────────────────────

struct TextInputInteractor {
    state: TextInputState<'static>,
}

impl TextInputInteractor {
    fn new() -> Self {
        Self {
            state: TextInputState::new("Workspace name", "jackin-core"),
        }
    }
}

impl StoryInteraction for TextInputInteractor {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(TextInput::new(&self.state), area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        // Delegate everything to the real TextInputState — it handles what it
        // knows and ignores the rest. We always return true so the preview redraws.
        self.state.handle_key(key);
        true
    }

    fn handle_mouse(&mut self, _mouse: MouseEvent, _preview_area: Rect) -> bool {
        false
    }
}

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

    /// Create a stateful interactor for this story. Stories with known
    /// interactive implementations return a live interactor; all others fall
    /// back to a `StaticStory` that simply calls the fn-pointer render.
    #[must_use]
    pub fn make_interactor(&self) -> Box<dyn StoryInteraction> {
        match self.id {
            "tab-strip/basic" => Box::new(TabStripInteractor::new()),
            "select-list/agent-picker" => Box::new(SelectListInteractor::new()),
            "scrollable-panel/mounts" => Box::new(ScrollablePanelInteractor::new()),
            "confirm/default" => Box::new(ConfirmInteractor::default_story()),
            "confirm/role-trust" => Box::new(ConfirmInteractor::role_trust_story()),
            "text-input/workspace-name" => Box::new(TextInputInteractor::new()),
            _ => Box::new(StaticStory {
                render_fn: self.render,
            }),
        }
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
                Span::styled(
                    "Name: ",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
                Span::styled("jackin-core", Style::default().fg(PHOSPHOR_GREEN)),
            ]),
            Line::from(vec![
                Span::styled(
                    "Role: ",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "github.com/jackin-project/roles/rust",
                    Style::default().fg(PHOSPHOR_GREEN),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "Agent: ",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
                Span::styled("codex", Style::default().fg(PHOSPHOR_GREEN)),
            ]),
            Line::from(vec![
                Span::styled(
                    "Mounts: ",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "repo rw, ~/.config/gh ro",
                    Style::default().fg(PHOSPHOR_DIM),
                ),
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
    let state = ConfirmState::new(
        "Delete workspace \"jackin-core\"?\nThis removes the saved workspace entry.",
    );
    render_confirm_dialog(frame, area, &state);
}

fn story_confirm_role_trust(frame: &mut Frame<'_>, area: Rect) {
    let state = ConfirmState::role_trust("rust", "https://github.com/jackin-project/roles");
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
        HintSpan::Key("↵"),
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
            Span::styled(
                "Workspace: ",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
            Span::styled("jackin-core", Style::default().fg(PHOSPHOR_GREEN)),
        ]),
        Line::from(vec![
            Span::styled(
                "Role: ",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
            Span::styled("rust", Style::default().fg(PHOSPHOR_GREEN)),
        ]),
    ];
    frame.render_widget(
        SelectList::new(&state, "Choose agent").context(&context),
        area,
    );
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
