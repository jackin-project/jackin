// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared read-only container/session information dialog.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use termrock::ModalOutcome;
use termrock::components::dialog_layout::{
    DialogBodyScroll, DialogBorder, render_dialog_shell, render_scrollable_dialog_body,
};
use termrock::components::panel::{Panel, PanelFocus};
use termrock::components::scrollable_panel::effective_offset;
use termrock::style::{LINK_FG, LINK_FG_HOVER, PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};

/// Body line indent (matches the canonical dialog content padding).
const INDENT_COLS: usize = 2;
/// Width of the `" : "` label/value separator.
const SEP_COLS: usize = 3;
/// Visible copy affordance appended to every copyable value.
const COPY_AFFORDANCE: &str = "  ⧉";

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
    /// Opaque invocation id.
    pub invocation_id: Option<String>,
}

impl DebugInfo {
    /// Build the dialog state in canonical row order, omitting unknown fields.
    #[must_use]
    pub fn into_state(self) -> ContainerInfoState {
        let mut rows = Vec::new();
        if let Some(invocation_id) = self.invocation_id {
            rows.push(ContainerInfoRow::new("Invocation ID", invocation_id).copyable());
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
    /// with every other dialog through [`DialogBodyScroll`] so vertical and
    /// horizontal scroll behave identically everywhere.
    pub scroll: DialogBodyScroll,
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
            scroll: DialogBodyScroll::new(),
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
            let axes = termrock::components::dialog_scroll_axes(
                content_width,
                content_height,
                dialog_rect,
            );
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
            termrock::components::ScrollAxes {
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
        axes: termrock::components::ScrollAxes,
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
                INDENT_COLS
                    + label_width
                    + SEP_COLS
                    + termrock::display_cols(&row.value)
                    + copy_affordance_cols(row)
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
            .map(|row| termrock::display_cols(&row.label))
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
    scroll: &mut DialogBodyScroll,
    content_width: usize,
    content_height: usize,
    dialog_rect: Rect,
) {
    use termrock::components::scrollable_panel::effective_offset;
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
pub fn debug_info_hint_spans(
    axes: termrock::components::ScrollAxes,
) -> Vec<termrock::HintSpan<'static>> {
    let mut spans = termrock::components::scroll_hint_spans(axes);
    if axes.any() {
        spans.push(termrock::HintSpan::GroupSep);
    }
    // UNREGISTERABLE(container-info-copy): Enter copies the active row inline; no ContainerInfo keymap.
    spans.push(termrock::HintSpan::Key("↵"));
    spans.push(termrock::HintSpan::Text("copy value"));
    spans.push(termrock::HintSpan::GroupSep);
    // UNREGISTERABLE(container-info-reveal): R/O toggle reveals diagnostics inline; no ContainerInfo keymap.
    spans.push(termrock::HintSpan::Key("R/O"));
    spans.push(termrock::HintSpan::Text("reveal diagnostics"));
    spans.push(termrock::HintSpan::GroupSep);
    // UNREGISTERABLE(container-info-no-keymap): Esc dismisses inline.
    spans.push(termrock::HintSpan::Key("Esc"));
    spans.push(termrock::HintSpan::Text("dismiss"));
    spans.push(termrock::HintSpan::GroupSep);
    // UNREGISTERABLE(mouse): mouse click cannot be expressed as a KeyChord.
    spans.push(termrock::HintSpan::Key("click"));
    spans.push(termrock::HintSpan::Text("copy value"));
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
    let inner = render_dialog_shell(frame, area, Some(&state.title), DialogBorder::Default);
    let label_width = state.label_width();
    // The body is a single scrollable block: a leading spacer row then one
    // line per fact. render_scrollable_dialog_body applies both-axis scroll and
    // draws the scrollbars on the dialog border only when content overflows.
    let mut lines = Vec::with_capacity(state.rows.len().saturating_add(1));
    lines.push(Line::from(""));
    for (idx, row) in state.rows.iter().enumerate() {
        lines.push(container_info_line(
            row,
            label_width,
            state.copied_row == Some(idx),
            state.hovered_row == Some(idx),
        ));
    }
    let mut scroll = state.scroll.clone();
    render_scrollable_dialog_body(frame, area, inner, &lines, &mut scroll);
}

/// Hit-test the value placements at `(col, row)`, returning the first row whose
/// rendered value cell contains the cursor and satisfies `predicate`, mapped
/// through `extractor`. Both `copy_payload_at` and `hyperlink_payload_at` share
/// this geometry; they differ only in the row predicate and payload extractor.
fn payload_at(
    area: Rect,
    state: &ContainerInfoState,
    col: u16,
    row: u16,
    predicate: impl Fn(&ContainerInfoRow) -> bool,
    extractor: impl Fn(usize, &ContainerInfoRow) -> Option<(usize, String)>,
) -> Option<(usize, String)> {
    value_placements(area, state)
        .into_iter()
        .find(|p| {
            predicate(&state.rows[p.idx])
                && row == p.screen_y
                && col >= p.screen_x
                && col < p.screen_x.saturating_add(p.visible_target_cols)
        })
        .and_then(|p| extractor(p.idx, &state.rows[p.idx]))
}

#[must_use]
pub fn copy_payload_at(
    area: Rect,
    state: &ContainerInfoState,
    col: u16,
    row: u16,
) -> Option<(usize, String)> {
    payload_at(
        area,
        state,
        col,
        row,
        |r| r.copyable,
        |idx, r| Some((idx, r.value.clone())),
    )
}

#[must_use]
pub fn hyperlink_payload_at(
    area: Rect,
    state: &ContainerInfoState,
    col: u16,
    row: u16,
) -> Option<(usize, String)> {
    payload_at(
        area,
        state,
        col,
        row,
        |r| r.href.is_some(),
        |idx, r| r.href.clone().map(|href| (idx, href)),
    )
}

/// Visible hyperlink cells for the encoder's frame-layer OSC 8 emission:
/// one `(rect, uri)` per linked row slice currently on screen. The capsule's
/// cell encoder brackets exactly these cells during emission, replacing the
/// raw post-frame overlay (the host console still uses
/// [`hyperlink_overlay`]).
#[must_use]
pub fn hyperlink_regions(area: Rect, state: &ContainerInfoState) -> Vec<(Rect, String)> {
    value_placements(area, state)
        .into_iter()
        .filter_map(|p| {
            let row = &state.rows[p.idx];
            let href = row.href()?;
            let visible = termrock::display_cols_slice(
                row.value(),
                p.skip_cols,
                usize::from(p.visible_value_cols),
            );
            if visible.is_empty() {
                return None;
            }
            let width = termrock::display_cols(&visible) as u16;
            Some((
                Rect {
                    x: p.screen_x.saturating_sub(1),
                    y: p.screen_y.saturating_sub(1),
                    width,
                    height: 1,
                },
                href.to_owned(),
            ))
        })
        .collect()
}

#[must_use]
pub fn hyperlink_overlay(area: Rect, state: &ContainerInfoState) -> Vec<u8> {
    let mut out = Vec::new();
    for p in value_placements(area, state) {
        let row = &state.rows[p.idx];
        let Some(href) = row.href() else {
            continue;
        };
        let link = if state.hovered_row == Some(p.idx) {
            termrock::LINK_FG_HOVER
        } else {
            termrock::LINK_FG
        };
        // Render only the horizontally-visible slice of the value, matching what
        // the scrolled Paragraph already painted, so the OSC 8 link lands on the
        // exact visible cells.
        let visible = termrock::display_cols_slice(
            row.value(),
            p.skip_cols,
            usize::from(p.visible_value_cols),
        );
        if visible.is_empty() {
            continue;
        }
        out.extend_from_slice(
            format!(
                "\x1b[{};{}H",
                p.screen_y.saturating_add(1),
                p.screen_x.saturating_add(1)
            )
            .as_bytes(),
        );
        out.extend_from_slice(&termrock::osc::encode_hyperlink_open(None, href));
        out.extend_from_slice(format!("\x1b[38;2;{};{};{}m", link.r, link.g, link.b).as_bytes());
        out.extend_from_slice(b"\x1b[1;4m");
        out.extend_from_slice(visible.as_bytes());
        out.extend_from_slice(&termrock::osc::encode_hyperlink_close());
        out.extend_from_slice(b"\x1b[0m");
    }
    out
}

/// On-screen placement of one row's value text under the current scroll.
struct ValuePlacement {
    idx: usize,
    screen_x: u16,
    screen_y: u16,
    /// Leading value columns scrolled off the left edge.
    skip_cols: usize,
    /// Visible value columns remaining after the skip + right clip.
    visible_value_cols: u16,
    /// Visible clickable columns, including the copy affordance for copyable rows.
    visible_target_cols: u16,
}

/// Visible value placements for every row, accounting for both scroll axes.
/// Used by the copy hit-test and the OSC 8 hyperlink overlay so both follow the
/// content as it scrolls. Rows fully scrolled out of view are omitted.
fn value_placements(area: Rect, state: &ContainerInfoState) -> Vec<ValuePlacement> {
    if area.width < 20 || area.height < 5 {
        return Vec::new();
    }
    let inner = Panel::new().focus(PanelFocus::Focused).block().inner(area);
    let label_width = state.label_width();
    let value_col = INDENT_COLS + label_width + SEP_COLS;
    let content_height = state.rows.len().saturating_add(1);
    let content_width = state.content_width();
    let eff_x = usize::from(effective_offset(
        content_width,
        usize::from(inner.width),
        state.scroll.scroll_x,
    ));
    let eff_y = usize::from(effective_offset(
        content_height,
        usize::from(inner.height),
        state.scroll.scroll_y,
    ));
    let vp_right = eff_x + usize::from(inner.width);
    let vp_bottom = eff_y + usize::from(inner.height);
    state
        .rows
        .iter()
        .enumerate()
        .filter_map(|(idx, row)| {
            let line_index = idx + 1; // line 0 is the leading spacer
            if line_index < eff_y || line_index >= vp_bottom {
                return None;
            }
            let value_cols = termrock::display_cols(row.value());
            let target_cols = value_cols + copy_affordance_cols(row);
            let target_start = value_col.max(eff_x);
            let target_end = (value_col + target_cols).min(vp_right);
            if target_start >= target_end {
                return None;
            }
            let value_start = target_start.min(value_col + value_cols);
            let value_end = target_end.min(value_col + value_cols);
            Some(ValuePlacement {
                idx,
                screen_x: inner
                    .x
                    .saturating_add(u16::try_from(target_start - eff_x).ok()?),
                screen_y: inner
                    .y
                    .saturating_add(u16::try_from(line_index - eff_y).ok()?),
                skip_cols: value_start - value_col,
                visible_value_cols: u16::try_from(value_end.saturating_sub(value_start))
                    .unwrap_or(u16::MAX),
                visible_target_cols: u16::try_from(target_end - target_start).unwrap_or(u16::MAX),
            })
        })
        .collect()
}

fn copy_affordance_cols(row: &ContainerInfoRow) -> usize {
    if row.copyable {
        termrock::display_cols(COPY_AFFORDANCE)
    } else {
        0
    }
}

fn container_info_line(
    row: &ContainerInfoRow,
    label_width: usize,
    copied: bool,
    hovered: bool,
) -> Line<'static> {
    let label_style = termrock::style::DIM;
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    // A copyable value (with or without an href) is a clickable link: it reads
    // in LINK_FG cyan and underlined, brightening to LINK_FG_HOVER on hover.
    // Non-copyable emphasised values stay brand-green; plain values stay white.
    let clickable = row.copyable || row.href.is_some();
    let mut value_style = Style::default().fg(if clickable {
        if hovered { LINK_FG_HOVER } else { LINK_FG }
    } else if row.emphasised {
        PHOSPHOR_GREEN
    } else {
        WHITE
    });
    if row.emphasised || clickable {
        value_style = value_style.add_modifier(Modifier::BOLD);
    }
    if clickable {
        value_style = value_style.add_modifier(Modifier::UNDERLINED);
    }
    let copy_style = if hovered {
        Style::default()
            .fg(LINK_FG_HOVER)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(LINK_FG).add_modifier(Modifier::BOLD)
    };
    let mut spans = vec![
        // 2-space indent (INDENT_COLS) baked in so the body renders flush in the
        // dialog inner area and value_placements' column math stays in sync.
        Span::raw("  "),
        Span::styled(format!("{:<label_width$}", row.label), label_style),
        Span::styled(" : ", sep_style),
        Span::styled(row.value.clone(), value_style),
    ];
    if row.copyable {
        spans.push(Span::styled(COPY_AFFORDANCE, copy_style));
    }
    if copied {
        spans.push(Span::styled(
            "  Copied!",
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}
