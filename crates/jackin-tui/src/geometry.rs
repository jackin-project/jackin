//! Text measurement, tab layout, and terminal-title helpers for jackin❯ TUI surfaces.

use ratatui::layout::Rect;

/// Per-tab descriptor consumed by both ratatui and ANSI tab
/// renderers. `cell_cols` is the number of display columns the cell
/// occupies including its left/right padding spaces.
#[derive(Debug, Clone)]
pub struct TabCell<'a> {
    pub label: &'a str,
    pub active: bool,
    /// 0-based column index where this cell's leftmost space starts.
    pub start_col: u16,
    /// Display column width of the cell (`label_cols + 2` padding).
    pub cell_cols: u16,
}

/// Single space between adjacent tab cells. Console TUI and
/// jackin-capsule both follow this spacing.
pub const TAB_GAP: u16 = 1;

/// One footer-hint span — the single hint vocabulary shared by every TUI
/// surface (host cockpit, workspace manager, in-container multiplexer). Each
/// backend has its own renderer over these spans, but the vocabulary and
/// styling rule are one: `Key` white + bold, `Text`/`Dyn` phosphor green /
/// dim, `Sep` a gray dot, `GroupSep` a wide gap.
///
/// Not `Copy`: `Dyn` owns a runtime `String` (e.g. "3 items selected"), which
/// `Key`/`Text` static spans cannot express. Static hint lists stay `const`
/// (`&[HintSpan]` of borrowed variants); only runtime-built lists allocate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintSpan<'a> {
    /// Hotkey glyph(s) — white + bold in rendered output.
    Key(&'a str),
    /// Runtime hotkey glyph(s) — white + bold in rendered output.
    DynKey(String),
    /// Action label following a key — phosphor green in rendered output.
    Text(&'a str),
    /// Runtime action label whose text is only known at render time —
    /// rendered dim to set it apart from the static `Text` labels.
    Dyn(String),
    /// Single dot separator (`" · "`) between two related items in
    /// the same group.
    Sep,
    /// Three-space gap between hint groups.
    GroupSep,
}

/// Center a fixed-size rectangle inside `area`, leaving a one-cell margin
/// where the terminal has room for it.
#[must_use]
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

impl HintSpan<'_> {
    /// Display-column width contribution of a single span. Mirrors
    /// the rendering rule that `Text` / `Dyn` spans render with a leading
    /// space and `Sep`/`GroupSep` each occupy three columns.
    #[must_use]
    pub fn display_cols(&self) -> usize {
        match self {
            Self::Key(k) => k.chars().count(),
            Self::DynKey(k) => k.chars().count(),
            Self::Text(t) => 1 + t.chars().count(),
            Self::Dyn(t) => 1 + t.chars().count(),
            Self::Sep | Self::GroupSep => 3,
        }
    }
}

/// Total display-column width of a hint-span sequence. Used by
/// renderers to compute centring and to decide whether the hint
/// fits inside the current terminal width.
#[must_use]
pub fn hint_row_cols(spans: &[HintSpan<'_>]) -> usize {
    spans.iter().map(HintSpan::display_cols).sum()
}

/// True for any C0 / C1 control byte or DEL (`0x7f`). These bytes
/// are what terminal-injection attacks (`\x1b[…m`, `\x9b…`, OSC,
/// BEL) build their sequences from; stripping them eliminates the
/// class.
#[must_use]
pub fn is_terminal_control_char(c: char) -> bool {
    let code = c as u32;
    code < 0x20 || c == '\x7f' || (0x80..0xa0).contains(&code)
}

/// Display-column width of `s` measured with `unicode-width`,
/// excluding C0 / C1 control bytes. Stripping controls here makes
/// the result safe to feed to width-budget callers regardless of
/// upstream input.
#[must_use]
pub fn display_cols(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    s.chars()
        .filter(|c| !is_terminal_control_char(*c))
        .map(|c| c.width().unwrap_or(0))
        .sum()
}

/// Take the longest prefix of `s` whose display width fits inside
/// `max_cols`, skipping control bytes.
#[must_use]
pub fn take_display_cols(s: &str, max_cols: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut out = String::new();
    let mut used = 0usize;
    for c in s.chars() {
        if is_terminal_control_char(c) {
            continue;
        }
        let width = c.width().unwrap_or(0);
        if used + width > max_cols {
            break;
        }
        out.push(c);
        used += width;
    }
    out
}

/// Substring of `s` covering display columns `[skip, skip + width)`,
/// skipping terminal control bytes and preserving only complete characters.
#[must_use]
pub fn display_cols_slice(s: &str, skip: usize, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut col = 0usize;
    let mut out = String::new();
    for ch in s.chars() {
        if is_terminal_control_char(ch) {
            continue;
        }
        let w = ch.width().unwrap_or(0);
        if col >= skip && col + w <= skip + width {
            out.push(ch);
        }
        col += w;
        if col >= skip + width {
            break;
        }
    }
    out
}

/// Leading ASCII-space count for text rows that need symmetric trailing
/// scroll padding. Controls are ignored so injected bytes cannot affect
/// width math.
#[must_use]
pub fn leading_space_cols<S>(parts: impl IntoIterator<Item = S>) -> usize
where
    S: AsRef<str>,
{
    let mut count = 0;
    for part in parts {
        for ch in part.as_ref().chars() {
            if is_terminal_control_char(ch) {
                continue;
            }
            if ch != ' ' {
                return count;
            }
            count += 1;
        }
    }
    count
}

/// Display-column width for a row plus the matching trailing padding used by
/// horizontally scrollable, indented content.
#[must_use]
pub fn padded_line_display_cols<I, S>(parts: I) -> usize
where
    I: IntoIterator<Item = S> + Clone,
    S: AsRef<str>,
{
    parts
        .clone()
        .into_iter()
        .map(|part| display_cols(part.as_ref()))
        .sum::<usize>()
        + leading_space_cols(parts)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedPrefixSegment {
    pub start_byte: usize,
    pub end_byte: usize,
    pub target_col: usize,
    pub display_cols: usize,
}

/// Visible byte ranges for a horizontally scrolled line whose prefix remains
/// fixed while the suffix scrolls by display columns.
#[must_use]
pub fn fixed_prefix_scroll_segments(
    text: &str,
    base_col: usize,
    fixed_prefix_cols: usize,
    scroll_cols: usize,
    viewport_cols: usize,
) -> Vec<FixedPrefixSegment> {
    use unicode_width::UnicodeWidthChar;

    let prefix_cols = fixed_prefix_cols.min(viewport_cols);
    let suffix_cols = viewport_cols.saturating_sub(prefix_cols);
    let suffix_start = fixed_prefix_cols.saturating_add(scroll_cols);
    let suffix_end = suffix_start.saturating_add(suffix_cols);
    let mut segments: Vec<FixedPrefixSegment> = Vec::new();
    let mut col = base_col;

    for (start_byte, ch) in text.char_indices() {
        if is_terminal_control_char(ch) {
            continue;
        }
        let end_byte = start_byte + ch.len_utf8();
        let width = ch.width().unwrap_or(0);
        if width == 0 {
            if let Some(last) = segments.last_mut()
                && last.end_byte == start_byte
            {
                last.end_byte = end_byte;
            }
            continue;
        }

        let target_col = if col < prefix_cols && col + width <= prefix_cols {
            col
        } else if col >= suffix_start && col + width <= suffix_end {
            prefix_cols + (col - suffix_start)
        } else {
            col += width;
            continue;
        };
        if target_col + width <= viewport_cols {
            segments.push(FixedPrefixSegment {
                start_byte,
                end_byte,
                target_col,
                display_cols: width,
            });
        }
        col += width;
    }

    segments
}

/// Collapse a terminal-window title to a single line of printable
/// characters: control bytes become spaces, runs of whitespace
/// collapse to one space, and leading / trailing whitespace is
/// trimmed. Safe to embed in an OSC 2 string regardless of source.
#[must_use]
pub fn sanitize_terminal_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_space = true;
    for ch in title.chars() {
        if ch.is_control() || ch == '\u{7f}' || ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Title-case display name for an agent slug. Mirrors the console
/// TUI's `agent_picker_label` so both surfaces use the same casing.
/// Returns `None` for unrecognised slugs so callers can fall back to
/// the raw slug rather than silently displaying a wrong label.
#[must_use]
pub fn agent_display_name(slug: &str) -> Option<&'static str> {
    match slug {
        "claude" => Some("Claude"),
        "codex" => Some("Codex"),
        "amp" => Some("Amp"),
        "kimi" => Some("Kimi"),
        "opencode" => Some("OpenCode"),
        _ => None,
    }
}

/// Build a row of `TabCell` descriptors from `(label, active)` pairs,
/// starting at `start_col`. Used by both consumers to compute
/// click-region bounds and to know where to paint the active-tab
/// underline.
///
/// Column width is measured with `unicode-width`. Plain `.chars().count()`
/// silently counts wide glyphs (CJK, emoji) as 1 column and combining
/// marks as N columns instead of the 2 / 0 cells they actually occupy
/// — every downstream click hit-test and underline placement then drifts
/// by however many wide/combining chars sit before the active tab.
#[must_use]
pub fn lay_out_tabs<'a>(labels: &[(&'a str, bool)], start_col: u16) -> Vec<TabCell<'a>> {
    use unicode_width::UnicodeWidthStr;
    let mut col = start_col;
    let mut out = Vec::with_capacity(labels.len());
    for &(label, active) in labels {
        let label_cols = u16::try_from(UnicodeWidthStr::width(label)).unwrap_or(u16::MAX);
        let cell_cols = label_cols.saturating_add(2); // " label "
        out.push(TabCell {
            label,
            active,
            start_col: col,
            cell_cols,
        });
        col = col.saturating_add(cell_cols).saturating_add(TAB_GAP);
    }
    out
}

/// Index of the tab cell whose column range contains `col`, if any. Shared
/// by every tab strip (console editor/settings, in-container status bar) so
/// click and hover hit-testing resolve the same tab as `lay_out_tabs`
/// painted — no surface re-derives the column maths. `col` and the cells'
/// `start_col` are in the same 0-based column space.
#[must_use]
pub fn tab_at_column(cells: &[TabCell<'_>], col: u16) -> Option<usize> {
    cells.iter().position(|cell| {
        col >= cell.start_col && col < cell.start_col.saturating_add(cell.cell_cols)
    })
}
