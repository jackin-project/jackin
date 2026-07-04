// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Stateful interactors for interactive story previews.
//!
//! Each interactor holds a real component state and delegates key / mouse
//! events to the same public API that the rest of the jackin ecosystem uses.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use jackin_tui::components::{
    ButtonStrip, ButtonStripItem, ConfirmState, SaveDiscardState, SelectList, SelectListState,
    TabStrip, TextInput, TextInputState, render_confirm_dialog, render_save_discard_dialog,
    render_scrollable_block,
};
use jackin_tui::{
    lay_out_tabs,
    scroll::{self, ScrollSpan},
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
};

/// Interactive preview state for a story. The terminal lookbook holds one
/// of these per-story-selection and forwards events to it.
pub(crate) trait StoryInteraction {
    /// Render the component with current live state into `area`.
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect);
    /// Handle a keyboard event. Returns true if consumed.
    fn handle_key(&mut self, key: KeyEvent) -> bool;
    /// Handle a mouse event relative to the full terminal. `preview_area` is
    /// the Rect the preview occupies so the story can hit-test relative coords.
    fn handle_mouse(&mut self, mouse: MouseEvent, preview_area: Rect) -> bool;
}

// ── StaticStory: no-op wrapper for fn-pointer stories ────────────────────────

pub(crate) struct StaticStory {
    pub(crate) render_fn: fn(&mut Frame<'_>, Rect),
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

pub(crate) struct TabStripInteractor {
    labels: Vec<(&'static str, bool)>,
    selected: usize,
    hovered: Option<usize>,
}

impl TabStripInteractor {
    pub(crate) fn new() -> Self {
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

pub(crate) struct SelectListInteractor {
    state: SelectListState,
    context: Vec<Line<'static>>,
}

impl SelectListInteractor {
    pub(crate) fn new() -> Self {
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
                Span::styled("Workspace: ", jackin_tui::theme::BOLD_WHITE),
                Span::styled("jackin-core", jackin_tui::theme::GREEN),
            ]),
            Line::from(vec![
                Span::styled("Role: ", jackin_tui::theme::BOLD_WHITE),
                Span::styled("rust", jackin_tui::theme::GREEN),
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

pub(crate) struct ScrollablePanelInteractor {
    scroll_x: u16,
    scroll_y: u16,
    last_area: Rect,
}

impl ScrollablePanelInteractor {
    pub(crate) fn new() -> Self {
        Self {
            scroll_x: 0,
            scroll_y: 0,
            last_area: Rect::default(),
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
        self.last_area = area;
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
        let viewport_height = jackin_tui::components::viewport_height(self.last_area);
        if !scroll::is_scrollable(line_count, viewport_height) {
            return false;
        }
        match key.code {
            KeyCode::Up => {
                scroll::apply_delta_u16(line_count, viewport_height, &mut self.scroll_y, -1);
                true
            }
            KeyCode::Down => {
                scroll::apply_delta_u16(line_count, viewport_height, &mut self.scroll_y, 1);
                true
            }
            _ => false,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, preview_area: Rect) -> bool {
        let lines = Self::lines();
        let content_width = jackin_tui::components::max_line_width(&lines);
        let content_height = lines.len();
        let viewport_width = jackin_tui::components::viewport_width(preview_area);
        let viewport_height = jackin_tui::components::viewport_height(preview_area);
        let axes = scroll::ScrollAxes {
            vertical: scroll::is_scrollable(content_height, viewport_height),
            horizontal: scroll::is_scrollable(content_width, viewport_width),
        };
        scroll::apply_mouse_scroll_u16(
            mouse.kind,
            mouse.modifiers,
            axes,
            ScrollSpan::new(content_width, viewport_width),
            ScrollSpan::new(content_height, viewport_height),
            &mut self.scroll_x,
            &mut self.scroll_y,
        )
    }
}

// ── ConfirmDialog interactor ──────────────────────────────────────────────────

pub(crate) struct ConfirmInteractor {
    state: ConfirmState,
}

impl ConfirmInteractor {
    pub(crate) fn default_story() -> Self {
        Self {
            state: ConfirmState::new(
                "Delete workspace \"jackin-core\"?\nThis removes the saved workspace entry.",
            ),
        }
    }

    pub(crate) fn role_trust_story() -> Self {
        Self {
            state: role_trust_confirm_state(),
        }
    }
}

fn role_trust_confirm_state() -> ConfirmState {
    ConfirmState::details(
        "Trust role source",
        "Trust this role source?",
        vec![
            ("Role".into(), "rust".into()),
            (
                "Repository".into(),
                "https://github.com/jackin-project/roles".into(),
            ),
        ],
        vec![
            "Dockerfile can run during image builds.".into(),
            "The role can access mounted workspace files.".into(),
        ],
    )
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
        // The canonical dialog layout keeps one trailing spacer after the
        // button row and then the bottom border.
        let button_row = preview_area.y + preview_area.height.saturating_sub(3);
        if mouse.row != button_row {
            return false;
        }
        // ButtonStrip centres two buttons. Use column midpoint as a heuristic:
        // columns in the left half → Yes, right half → No.
        let mid = preview_area.x + preview_area.width / 2;
        if mouse.column < mid {
            self.state = self.state.clone().with_focus_yes();
        } else {
            self.state = self.state.clone().with_focus_no();
        }
        true
    }
}

// ── TextInput interactor ──────────────────────────────────────────────────────

pub(crate) struct TextInputInteractor {
    state: TextInputState<'static>,
}

impl TextInputInteractor {
    pub(crate) fn new() -> Self {
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

// ── ButtonStrip interactor ────────────────────────────────────────────────────

pub(crate) struct ButtonStripInteractor {
    items: [ButtonStripItem<'static>; 4],
    focused: usize,
}

impl ButtonStripInteractor {
    pub(crate) fn new() -> Self {
        Self {
            items: [
                ButtonStripItem::new("Save"),
                ButtonStripItem::new("Discard"),
                ButtonStripItem::disabled("Launch"),
                ButtonStripItem::new("Cancel"),
            ],
            focused: 0,
        }
    }
}

impl StoryInteraction for ButtonStripInteractor {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        use ratatui::layout::{Constraint, Layout};
        let [_, strip_area, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(area);
        ButtonStrip::new(&self.items)
            .focused(self.focused)
            .render(frame, strip_area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Left => {
                self.focused = self.focused.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                self.focused = (self.focused + 1).min(self.items.len().saturating_sub(1));
                true
            }
            _ => false,
        }
    }

    fn handle_mouse(&mut self, _mouse: MouseEvent, _preview_area: Rect) -> bool {
        false
    }
}

// ── SaveDiscard interactor ────────────────────────────────────────────────────

pub(crate) struct SaveDiscardInteractor {
    state: SaveDiscardState,
}

impl SaveDiscardInteractor {
    pub(crate) fn new() -> Self {
        Self {
            state: SaveDiscardState::new("Save workspace changes before leaving?"),
        }
    }
}

impl StoryInteraction for SaveDiscardInteractor {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        render_save_discard_dialog(frame, area, &self.state);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.state.handle_key(key);
        true
    }

    fn handle_mouse(&mut self, _mouse: MouseEvent, _preview_area: Rect) -> bool {
        false
    }
}
