// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-owned jackin❯ operator-information vocabulary and `TermRock` projection.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Style},
    widgets::StatefulWidget,
};

use jackin_core::ModalOutcome;
use termrock::scroll::DialogScroll;
use termrock::style::Role;
use termrock::text::display_cols;
use termrock::widgets::{
    DetailCapability, DetailRow, DetailTable, DetailTableOutcome, DetailTableState, HintSpan,
    Panel, PanelEmphasis,
};

#[derive(Debug, Clone)]
pub struct ContainerInfoRow {
    label: String,
    value: String,
    href: Option<String>,
    copyable: bool,
    emphasised: bool,
    /// Optional accent colour for the row's meter/value, used to severity-grade
    /// a usage bucket (warn/danger). `None` keeps the default rendering.
    accent: Option<Color>,
}

impl ContainerInfoRow {
    #[must_use]
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            href: None,
            copyable: false,
            emphasised: false,
            accent: None,
        }
    }

    /// Set the accent colour used to severity-grade this row's meter.
    #[must_use]
    pub const fn accent(mut self, accent: Color) -> Self {
        self.accent = Some(accent);
        self
    }

    /// The row's accent colour, if any.
    #[must_use]
    pub const fn accent_color(&self) -> Option<Color> {
        self.accent
    }

    #[must_use]
    pub fn hyperlink(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }

    #[must_use]
    pub const fn copyable(mut self) -> Self {
        self.copyable = true;
        self
    }

    #[must_use]
    pub const fn emphasised(mut self) -> Self {
        self.emphasised = true;
        self
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub fn href(&self) -> Option<&str> {
        self.href.as_deref()
    }

    #[must_use]
    pub const fn is_copyable(&self) -> bool {
        self.copyable
    }
}

/// Accumulating model for the shared "Debug info" dialog.
///
/// The same dialog is shown on every surface — the console manager, the launch
/// cockpit, and the in-container capsule — and gains rows as the corresponding
/// facts become known: the console knows only the run; launch additionally
/// knows the container id, role, agent, and target; the capsule additionally
/// knows its own binary version. Each surface fills the fields it knows and
/// calls [`DebugInfo::into_state`]; absent fields are simply omitted, so the
/// row set grows as the operator moves console -> launch -> capsule while the
/// ordering, labels, copy affordances, and styling stay identical.
///
/// Version strings are passed in as data because the canonical values live in
/// build-time env vars (`JACKIN_VERSION`, `JACKIN_CAPSULE_VERSION`) that are
/// only in scope in the binary crates. Pass the exact string `jackin --version`
/// / `jackin-capsule --version` print so the dialog never disagrees with the CLI.
#[derive(Debug, Clone, Default)]
pub struct DebugInfo {
    /// `jackin --version` output. Shown as the `jackin version` row.
    pub jackin_version: Option<String>,
    /// `jackin-capsule --version` output. Shown as the `jackin-capsule` row
    /// (capsule surface only).
    pub capsule_version: Option<String>,
    /// Container name, once one has been assigned (launch onward).
    pub container_id: Option<String>,
    pub role: Option<String>,
    pub agent: Option<String>,
    /// Working directory / target label.
    pub target: Option<String>,
    /// Bare run id — never the log path.
    pub run_id: Option<String>,
    /// Absolute path to the run's diagnostics JSONL. Rendered copyable with a
    /// `file://` hyperlink; the bare run id goes in [`Self::run_id`] instead.
    pub diagnostics_log_path: Option<String>,
}

impl DebugInfo {
    /// Build the dialog state in canonical row order, omitting unknown fields.
    #[must_use]
    pub fn into_state(self) -> ContainerInfoState {
        let mut rows = Vec::new();
        if let Some(run_id) = self.run_id {
            rows.push(ContainerInfoRow::new("Run ID", run_id).copyable());
        }
        if let Some(container_id) = self.container_id {
            rows.push(ContainerInfoRow::new("Container ID", container_id).copyable());
        }
        if let Some(version) = self.jackin_version {
            rows.push(ContainerInfoRow::new("jackin version", version));
        }
        if let Some(version) = self.capsule_version {
            rows.push(ContainerInfoRow::new("jackin-capsule", version));
        }
        if let Some(role) = self.role {
            rows.push(ContainerInfoRow::new("Role", role));
        }
        if let Some(agent) = self.agent {
            rows.push(ContainerInfoRow::new("Agent", agent));
        }
        if let Some(target) = self.target {
            rows.push(ContainerInfoRow::new("Target", target));
        }
        if let Some(path) = self.diagnostics_log_path {
            let href = format!("file://{path}");
            rows.push(
                ContainerInfoRow::new("Diagnostics log", path)
                    .copyable()
                    .hyperlink(href),
            );
        }
        ContainerInfoState::new("Debug info", rows)
    }
}

#[derive(Debug, Clone)]
pub struct ContainerInfoState {
    title: String,
    rows: Vec<ContainerInfoRow>,
    copied_row: Option<usize>,
    hovered_row: Option<usize>,
    viewport: Option<Rect>,
    /// Scroll offsets for when the content overflows the dialog area. Shared
    /// with every other dialog through [`DialogScroll`] so vertical and
    /// horizontal scroll behave identically everywhere.
    pub scroll: DialogScroll,
}

impl ContainerInfoState {
    #[must_use]
    pub fn new(title: impl Into<String>, rows: Vec<ContainerInfoRow>) -> Self {
        Self {
            title: title.into(),
            rows,
            copied_row: None,
            hovered_row: None,
            viewport: None,
            scroll: DialogScroll::new(),
        }
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn rows(&self) -> &[ContainerInfoRow] {
        &self.rows
    }

    pub fn push_row(&mut self, row: ContainerInfoRow) {
        self.rows.push(row);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<()> {
        if let Some(dialog_rect) = self.viewport {
            let content_height = self.content_height();
            let content_width = self.content_width();
            let axes =
                termrock::scroll::dialog_scroll_axes(content_width, content_height, dialog_rect);
            let viewport_width = usize::from(dialog_rect.width.saturating_sub(2));
            let viewport_height = usize::from(dialog_rect.height.saturating_sub(2));
            return self.handle_scroll_key(key, viewport_height, viewport_width, axes);
        }
        // Viewport is unknown here; pass 0 so the key is accepted and the next
        // render-time clamp settles the final offset, and advertise both axes.
        self.handle_scroll_key(
            key,
            0,
            0,
            termrock::scroll::ScrollAxes {
                vertical: true,
                horizontal: true,
            },
        )
    }

    pub fn set_viewport(&mut self, dialog_rect: Rect) {
        self.viewport = Some(dialog_rect);
    }

    /// Shared Esc/q dismiss + scroll-key dispatch for the Debug-info dialog. The
    /// two public entry points differ only in the viewport extents and the
    /// available axes they derive; the key set and dismiss behaviour are one.
    /// Content extents are clamped at render time, so a generous viewport here is
    /// harmless — the renderer never shows past the last row/col.
    fn handle_scroll_key(
        &mut self,
        key: KeyEvent,
        viewport_height: usize,
        viewport_width: usize,
        axes: termrock::scroll::ScrollAxes,
    ) -> ModalOutcome<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 'Q') => ModalOutcome::Cancel,
            // Scroll keys (Up/Down/Left/Right + vim h/j/k/l + PageUp/PageDown).
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Char('h' | 'H' | 'j' | 'J' | 'k' | 'K' | 'l' | 'L') => {
                let content_height = self.content_height();
                let content_width = self.content_width();
                let _consumed = self.scroll.handle_key_for_axes(
                    key.into(),
                    content_height,
                    viewport_height,
                    content_width,
                    viewport_width,
                    axes,
                );
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }

    /// Display-column width of the widest rendered body line (label column +
    /// `" : "` + value), including the 2-space indent. Drives horizontal scroll.
    /// Matches the unpadded width `render_scrollable_dialog_body` measures.
    #[must_use]
    pub fn content_width(&self) -> usize {
        let label_width = self.label_width();
        self.rows
            .iter()
            .map(|row| {
                display_cols("▸ ")
                    + label_width
                    + display_cols(" : ")
                    + display_cols(&row.value)
                    + if row.copyable {
                        display_cols("  ⧉")
                    } else {
                        0
                    }
            })
            .max()
            .unwrap_or(0)
    }

    /// Rendered body height: one leading spacer row + one row per fact.
    #[must_use]
    pub fn content_height(&self) -> usize {
        self.rows.len().saturating_add(1)
    }

    /// Clamp the scroll offsets to the content given the dialog's outer rect, so
    /// over-scrolling (holding →/↓ past the end, or a wheel that out-runs the
    /// content) cannot inflate the stored offset and make the opposite key feel
    /// dead while it unwinds. Call after handling a scroll key/wheel. `vp` is the
    /// inner viewport (rect minus the 1-col border on each side), matching what
    /// `render_scrollable_dialog_body` uses.
    pub fn clamp_scroll(&mut self, dialog_rect: Rect) {
        let content_width = self.content_width();
        let content_height = self.content_height();
        clamp_dialog_scroll(&mut self.scroll, content_width, content_height, dialog_rect);
    }

    fn label_width(&self) -> usize {
        self.rows
            .iter()
            .map(|row| display_cols(&row.label))
            .max()
            .unwrap_or(0)
    }

    pub fn mark_copied(&mut self, row: usize) {
        self.copied_row = Some(row);
    }

    #[must_use]
    pub const fn copied_row(&self) -> Option<usize> {
        self.copied_row
    }

    /// Set the row index the pointer is hovering (a copyable value), or `None`.
    /// Drives the link hover-colour change. Callers feed this from a mouse-move
    /// hit-test via [`copy_payload_at`], which returns the row index.
    pub fn set_hovered_row(&mut self, row: Option<usize>) {
        self.hovered_row = row;
    }

    #[must_use]
    pub const fn hovered_row(&self) -> Option<usize> {
        self.hovered_row
    }

    /// Default keyboard copy target for the Debug-info dialog.
    ///
    /// Mouse hit-testing copies the row under the pointer. Keyboard copy has no
    /// row cursor, so every surface uses the first copyable row in canonical
    /// row order as the stable default.
    #[must_use]
    pub fn keyboard_copy_payload(&self) -> Option<(usize, String)> {
        self.rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.copyable)
            .map(|(idx, row)| (idx, row.value.clone()))
    }
}

/// Clamp a dialog body's scroll offsets to the content within `dialog_rect`'s
/// inner viewport. Shared by surfaces whose scroll state lives outside a
/// persistent `ContainerInfoState` (the cockpit's `LaunchView`, the capsule's
/// `Dialog` enum) so they get the same over-scroll guard as `clamp_scroll`.
pub fn clamp_dialog_scroll(
    scroll: &mut DialogScroll,
    content_width: usize,
    content_height: usize,
    dialog_rect: Rect,
) {
    use termrock::scroll::effective_offset;
    let vp_w = usize::from(dialog_rect.width.saturating_sub(2));
    let vp_h = usize::from(dialog_rect.height.saturating_sub(2));
    scroll.scroll_x = effective_offset(content_width, vp_w, scroll.scroll_x);
    scroll.scroll_y = effective_offset(content_height, vp_h, scroll.scroll_y);
}

/// Keys for the Debug-info dialog hint bar: the *available* scroll axes (per
/// `axes`), keyboard copy, dismiss, then click-to-copy. The scroll segment is
/// omitted entirely when the body fits, and shows only the axis/axes that
/// actually overflow — the dialog never advertises a direction the operator
/// cannot move.
///
/// Single source of truth for the Debug-info hint bar: the console list modal
/// and the launch cockpit both render this exact sequence so the same dialog
/// never drifts between surfaces. The keyboard and mouse affordances are
/// inline-handled (Enter copies the hovered row, Esc dismisses, left-click
/// copies) with no backing `Keymap<A>`, so each span carries an
/// `// UNREGISTERABLE` annotation per the keymap/hint-bar enforcement rule.
#[must_use]
pub fn debug_info_hint_spans(axes: termrock::scroll::ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut spans = termrock::scroll::scroll_hint_spans(axes);
    if axes.any() {
        spans.push(HintSpan::GroupSep);
    }
    // UNREGISTERABLE(container-info-copy): Enter copies the active row inline; no ContainerInfo keymap.
    spans.push(HintSpan::Key("↵"));
    spans.push(HintSpan::Text("copy value"));
    spans.push(HintSpan::GroupSep);
    // UNREGISTERABLE(container-info-reveal): R/O toggle reveals diagnostics inline; no ContainerInfo keymap.
    spans.push(HintSpan::Key("R/O"));
    spans.push(HintSpan::Text("reveal diagnostics"));
    spans.push(HintSpan::GroupSep);
    // UNREGISTERABLE(container-info-no-keymap): Esc dismisses inline.
    spans.push(HintSpan::Key("Esc"));
    spans.push(HintSpan::Text("dismiss"));
    spans.push(HintSpan::GroupSep);
    // UNREGISTERABLE(mouse): mouse click cannot be expressed as a KeyChord.
    spans.push(HintSpan::Key("click"));
    spans.push(HintSpan::Text("copy value"));
    spans
}

#[must_use]
pub fn required_height(state: &ContainerInfoState) -> u16 {
    u16::try_from(state.rows.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .max(7)
}

pub fn render_container_info(frame: &mut Frame<'_>, area: Rect, state: &ContainerInfoState) {
    if area.width < 20 || area.height < 5 {
        return;
    }
    let theme = termrock::Theme::default();
    let panel = Panel::new(&theme)
        .title(&state.title)
        .emphasis(PanelEmphasis::Focused);
    let table_area = detail_table_area(panel.inner(area));
    frame.render_widget(&panel, area);

    let rows = detail_rows(state);
    let table = DetailTable::new(&rows, &theme).content_revision(detail_content_revision(state));
    let mut table_state = detail_state(state);
    frame.render_stateful_widget(&table, table_area, &mut table_state);

    // DetailTable paints its own scrollbars from table_state during render.
    let _ = (table_state.content_height, table_state.content_width);
}

fn detail_table_area(inner: Rect) -> Rect {
    Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1),
    )
}

fn detail_rows(state: &ContainerInfoState) -> Vec<DetailRow<'_, usize>> {
    state
        .rows
        .iter()
        .enumerate()
        .map(|(id, row)| DetailRow {
            id,
            label: row.label(),
            value: row.value(),
            href: row.href(),
            capability: match (row.copyable, row.href.is_some()) {
                (true, true) => DetailCapability::CopyAndLink,
                (true, false) => DetailCapability::Copy,
                (false, true) => DetailCapability::Link,
                (false, false) => DetailCapability::None,
            },
            emphasis: row.emphasised,
            style: row.accent.map(|color| Style::default().fg(color)),
        })
        .collect()
}

fn detail_state(state: &ContainerInfoState) -> DetailTableState<usize> {
    let mut table_state = DetailTableState::default();
    table_state.hovered = state.hovered_row;
    table_state.copied = state.copied_row;
    table_state.scroll.scroll_x = state.scroll.scroll_x;
    table_state.scroll.scroll_y = state.scroll.scroll_y;
    table_state
}

fn detail_content_revision(state: &ContainerInfoState) -> u64 {
    state.rows.iter().fold(state.rows.len() as u64, |acc, row| {
        acc.wrapping_mul(131)
            .wrapping_add(row.label().len() as u64)
            .wrapping_mul(131)
            .wrapping_add(row.value().len() as u64)
    })
}

fn detail_layout(
    area: Rect,
    state: &ContainerInfoState,
) -> (Vec<DetailRow<'_, usize>>, DetailTableState<usize>, Buffer) {
    let rows = detail_rows(state);
    let theme = termrock::Theme::default();
    let panel = Panel::new(&theme)
        .title(&state.title)
        .emphasis(PanelEmphasis::Focused);
    let table_area = detail_table_area(panel.inner(area));
    let table = DetailTable::new(&rows, &theme).content_revision(detail_content_revision(state));
    let mut table_state = detail_state(state);
    let mut buffer = Buffer::empty(area);
    (&table).render(table_area, &mut buffer, &mut table_state);
    (rows, table_state, buffer)
}

#[must_use]
pub fn copy_payload_at(
    area: Rect,
    state: &ContainerInfoState,
    col: u16,
    row: u16,
) -> Option<(usize, String)> {
    let (rows, mut table_state, _) = detail_layout(area, state);
    let hit = table_state.click(Position::new(col, row));
    let DetailTableOutcome::Copy(id) = hit else {
        return None;
    };
    rows.get(id).map(|row| (id, row.value.to_owned()))
}

#[must_use]
pub fn hyperlink_payload_at(
    area: Rect,
    state: &ContainerInfoState,
    col: u16,
    row: u16,
) -> Option<(usize, String)> {
    let (rows, mut table_state, _) = detail_layout(area, state);
    let hit = table_state.click_link(Position::new(col, row));
    let DetailTableOutcome::ActivateLink(id) = hit else {
        return None;
    };
    rows.get(id)
        .and_then(|row| row.href.map(|href| (id, href.to_owned())))
}

/// Visible hyperlink cells for the encoder's frame-layer OSC 8 emission:
/// one `(rect, uri)` per linked row slice currently on screen. The capsule's
/// cell encoder brackets exactly these cells during emission, replacing the
/// raw post-frame overlay (the host console still uses
/// [`hyperlink_overlay`]).
#[must_use]
pub fn hyperlink_regions(area: Rect, state: &ContainerInfoState) -> Vec<(Rect, String)> {
    let (rows, table_state, _) = detail_layout(area, state);
    let theme = termrock::Theme::default();
    DetailTable::new(&rows, &theme)
        .content_revision(detail_content_revision(state))
        .hyperlink_regions(&table_state)
        .into_iter()
        .map(|region| (region.area, region.url.to_owned()))
        .collect()
}

#[must_use]
pub fn hyperlink_overlay(area: Rect, state: &ContainerInfoState) -> Vec<u8> {
    let mut out = Vec::new();
    let (rows, table_state, buffer) = detail_layout(area, state);
    let theme = termrock::Theme::default();
    let table = DetailTable::new(&rows, &theme).content_revision(detail_content_revision(state));
    for region in table.hyperlink_regions(&table_state) {
        let role = if state.hovered_row == Some(region.id) {
            Role::LinkHover
        } else {
            Role::Link
        };
        let link = theme.style(role).fg.unwrap_or(Color::Reset);
        let Color::Rgb(red, green, blue) = link else {
            continue;
        };
        let visible = (region.area.x..region.area.right())
            .map(|x| buffer[(x, region.area.y)].symbol())
            .collect::<String>();
        if visible.is_empty() {
            continue;
        }
        out.extend_from_slice(
            format!(
                "\x1b[{};{}H",
                region.area.y.saturating_add(1),
                region.area.x.saturating_add(1)
            )
            .as_bytes(),
        );
        out.extend_from_slice(&termrock::osc::encode_hyperlink_open(None, region.url));
        out.extend_from_slice(format!("\x1b[38;2;{red};{green};{blue}m").as_bytes());
        out.extend_from_slice(b"\x1b[1;4m");
        out.extend_from_slice(visible.as_bytes());
        out.extend_from_slice(&termrock::osc::encode_hyperlink_close());
        out.extend_from_slice(b"\x1b[0m");
    }
    out
}

#[cfg(test)]
mod tests;
