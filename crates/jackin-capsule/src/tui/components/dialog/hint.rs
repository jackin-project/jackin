//! Footer hint rows for capsule dialogs.

use jackin_tui::{
    HintSpan, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE,
    ansi::{BG_DARK, BOLD, RESET, rgb_fg},
    hint_row_cols,
};

const FG_GREEN: &str = rgb_fg(PHOSPHOR_GREEN);
const FG_DIM: &str = rgb_fg(PHOSPHOR_DIM);
const FG_BORDER: &str = rgb_fg(PHOSPHOR_DARK);
const FG_WHITE: &str = rgb_fg(WHITE);

/// Hint shown in the main pane view when no dialog is open.
const MAIN_VIEW_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Ctrl+\\"),
    HintSpan::Text("menu"),
    HintSpan::GroupSep,
    HintSpan::Key("↑↓"),
    HintSpan::Text("scroll"),
    HintSpan::GroupSep,
    HintSpan::Key("click"),
    HintSpan::Text("focus pane"),
];

/// Hint shown when the operator is in scrollback mode.
const SCROLLBACK_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("scroll"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("exit scrollback"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl+\\"),
    HintSpan::Text("menu"),
];

const SELECTION_COPIED_HINT: &[HintSpan<'static>] = &[
    HintSpan::Text("selection copied"),
    HintSpan::GroupSep,
    HintSpan::Key("click"),
    HintSpan::Text("clear"),
    HintSpan::GroupSep,
    HintSpan::Key("type"),
    HintSpan::Text("clear"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl+\\"),
    HintSpan::Text("menu"),
];

/// Return the appropriate hint spans for the main view (no dialog open).
pub(crate) fn main_view_hint(
    scrollback_active: bool,
    selection_copied: bool,
) -> &'static [HintSpan<'static>] {
    if selection_copied {
        SELECTION_COPIED_HINT
    } else if scrollback_active {
        SCROLLBACK_HINT
    } else {
        MAIN_VIEW_HINT
    }
}

pub(super) const PALETTE_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type filter"),
    HintSpan::GroupSep,
    HintSpan::Key("↵"),
    HintSpan::Text("select"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
];

pub(super) const PICKER_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type filter"),
    HintSpan::GroupSep,
    HintSpan::Key("↵"),
    HintSpan::Text("launch"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
];

pub(super) const RENAME_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↵"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("cancel"),
    HintSpan::GroupSep,
    HintSpan::Text("empty = auto name"),
];

/// Read-only info-dialog hint: copy key, the *available* scroll axes (per
/// `axes`, omitted when the body fits), then dismiss — built from the shared
/// `scroll_hint_spans` primitive so it never advertises a scroll direction the
/// body cannot move. Used by both `ContainerInfo` (Debug info) and a loaded
/// `GitHubContext`, which differ only in their copy label.
pub(super) fn info_dialog_hint(
    copy_label: &'static str,
    axes: jackin_tui::components::ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let mut spans = vec![HintSpan::Key("↵"), HintSpan::Text(copy_label)];
    let scroll = jackin_tui::components::scroll_hint_spans(axes);
    if !scroll.is_empty() {
        spans.push(HintSpan::GroupSep);
        spans.extend(scroll);
    }
    spans.push(HintSpan::GroupSep);
    spans.push(HintSpan::Key("Esc"));
    spans.push(HintSpan::Text("dismiss"));
    spans
}

pub(super) const READ_ONLY_HINT: &[HintSpan<'static>] =
    &[HintSpan::Key("Esc"), HintSpan::Text("dismiss")];

pub(super) const CONFIRM_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Y"),
    HintSpan::Text("confirm"),
    HintSpan::GroupSep,
    HintSpan::Key("N"),
    HintSpan::Text("cancel"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("back"),
];

/// Compute the visual column width of a hint span row. Matches the
/// formatting in `render_hint_row` so centring is exact.
pub(crate) fn render_hint_row(buf: &mut Vec<u8>, row: u16, term_cols: u16, spans: &[HintSpan<'_>]) {
    let total = hint_row_cols(spans);
    let padded_total = total.saturating_add(4);
    if padded_total > term_cols as usize {
        crate::cdebug!(
            "hint-row: SKIP row={} term_cols={} content_cols={} padded={} (too wide)",
            row,
            term_cols,
            total,
            padded_total,
        );
        return;
    }
    let start_col = ((term_cols as usize).saturating_sub(padded_total) / 2) as u16;
    crate::cdebug!(
        "hint-row: row={} term_cols={} content_cols={} padded={} start_col={}",
        row,
        term_cols,
        total,
        padded_total,
        start_col,
    );
    move_to(buf, row, 0);
    buf.extend_from_slice(BG_DARK.as_bytes());
    for _ in 0..term_cols {
        buf.push(b' ');
    }
    move_to(buf, row, start_col);
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_BORDER.as_bytes());
    buf.extend_from_slice("  ".as_bytes());
    for span in spans {
        match span {
            HintSpan::Key(k) => {
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_WHITE.as_bytes());
                buf.extend_from_slice(BOLD.as_bytes());
                buf.extend_from_slice(k.as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
            HintSpan::Text(t) => {
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_GREEN.as_bytes());
                buf.push(b' ');
                buf.extend_from_slice(t.as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
            HintSpan::Dyn(t) => {
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_DIM.as_bytes());
                buf.push(b' ');
                buf.extend_from_slice(t.as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
            HintSpan::Sep => {
                buf.extend_from_slice(BG_DARK.as_bytes());
                buf.extend_from_slice(FG_BORDER.as_bytes());
                buf.extend_from_slice(" · ".as_bytes());
                buf.extend_from_slice(RESET.as_bytes());
            }
            HintSpan::GroupSep => {
                buf.extend_from_slice("   ".as_bytes());
            }
        }
    }
    buf.extend_from_slice(BG_DARK.as_bytes());
    buf.extend_from_slice(FG_BORDER.as_bytes());
    buf.extend_from_slice("  ".as_bytes());
    buf.extend_from_slice(RESET.as_bytes());
}

fn move_to(buf: &mut Vec<u8>, row: u16, col: u16) {
    buf.extend_from_slice(b"\x1b[");
    write_dec(buf, row + 1);
    buf.push(b';');
    write_dec(buf, col + 1);
    buf.push(b'H');
}

fn write_dec(buf: &mut Vec<u8>, n: u16) {
    if n == 0 {
        buf.push(b'0');
        return;
    }
    let mut tmp = [0_u8; 5];
    let mut i = 5;
    let mut v = n;
    while v > 0 {
        i -= 1;
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    buf.extend_from_slice(&tmp[i..]);
}
