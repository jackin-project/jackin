//! `DamageGrid` — the Phase 2 v0 terminal model implementation.
//!
//! Uses a ring-backed row store inspired by Alacritty's grid storage. Rows are
//! stable `Vec<Cell>` slices for render borrowing, while scroll and scrollback
//! rotation avoid shifting the whole backing vector.
//!
//! Key capability: `dirty_spans()` reports which rows were
//! mutated since the last call, recorded *as* `Perform` mutates the grid —
//! not recomputed by a full-grid diff.
//!
//! Implements `vte::Perform` directly so the capsule can swap in `DamageGrid`
//! wherever it needs to feed PTY output into a terminal model.
//!
//! # Attribution
//! Grid structure inspired by Alacritty `alacritty_terminal::grid::Grid`
//! (Apache-2.0/MIT) and Zellij `zellij-server::panes::grid::Grid` (MIT).
//! Neither crate is a dependency; only the design pattern is borrowed.

use std::{
    collections::{VecDeque, vec_deque},
    ops::{Index, IndexMut},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};

use crate::cell::{Attrs, Cell, Color, Hyperlink, UnderlineStyle};
use crate::damage::{DirtySpans, DirtyTracker};
use crate::passthrough::{PassthroughBuffer, PassthroughEvent};
use crate::width::VirtualTerminalProfile;

/// Provenance for one physical grid row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowWrap {
    /// Row starts a logical line or came from an explicit line break/edit op.
    #[default]
    Hard,
    /// Row continues the previous logical line because DECAWM soft-wrapped.
    Soft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOp {
    Up { top: u16, bottom: u16, rows: u16 },
    Down { top: u16, bottom: u16, rows: u16 },
}

/// Mouse protocol modes (DEC modes 1000/1002/1003).
///
/// Variants preserve the public names used by the capsule input layer:
/// - `Press` = mode 1000 (report button press only)
/// - `PressRelease` = mode 1002 (report press, release, and button motion)
/// - `ButtonMotion` = mode 1002 alias, identical to `PressRelease`
/// - `AnyEvent` = mode 1003 (report all motion)
/// - `AnyMotion` = mode 1003 alias, identical to `AnyEvent`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseProtocolMode {
    #[default]
    None,
    /// Mode 1000: report button press only.
    Press,
    /// Mode 1002: report press + release + motion while button held.
    /// Alias: `ButtonMotion`.
    PressRelease,
    /// Mode 1002: identical to `PressRelease`.
    ButtonMotion,
    /// Mode 1003: report all pointer motion.
    /// Alias: `AnyMotion`.
    AnyEvent,
    /// Mode 1003: identical to `AnyEvent`.
    AnyMotion,
}

/// Mouse protocol encodings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseProtocolEncoding {
    #[default]
    Default,
    Utf8,
    Sgr,
    Urxvt,
}

/// The Phase 2 v0 terminal model.
///
/// Call `process(bytes)` to feed raw PTY output.  The grid records which
/// spans changed via the dirty tracker.  Call `dirty_spans()` to retrieve
/// and clear the dirty set before rendering.
pub struct DamageGrid {
    // ── Parser — must persist across process() calls to handle split sequences ──
    // vte::Parser maintains internal state for multi-byte escape sequences.
    // Creating a new parser on each process() call would lose that state, causing
    // sequences split across PTY read() boundaries to be silently dropped.
    parser: vte::Parser,
    pending_utf8: Vec<u8>,
    profile: VirtualTerminalProfile,

    // ── Grid state ────────────────────────────────────────────────────────────
    rows: u16,
    cols: u16,
    /// Primary screen cells.
    primary: RowStore,
    /// Alternate screen cells (activated by `?1049h`).
    alternate: RowStore,
    /// True when the alternate screen is active.
    alt_screen: bool,
    /// Scrollback buffer (primary screen only). Newest entry = last item.
    scrollback: RowStore,
    /// Max scrollback rows kept.
    scrollback_limit: usize,
    /// Current scrollback view offset (0 = live tail).
    scrollback_offset: usize,

    // ── Cursor ────────────────────────────────────────────────────────────────
    cursor_row: u16,
    cursor_col: u16,
    saved_cursor_row: u16,
    saved_cursor_col: u16,
    /// DECAWM deferred wrap. A printable written in the last column does NOT
    /// move the cursor; it parks here with `pending_wrap` armed. The next
    /// printable performs the wrap first; any explicit cursor move (CR, LF,
    /// CUP, CHA, …) cancels it. Eager-wrapping instead drifts the cursor one
    /// row down per last-column write, which is how box-drawing TUIs (Claude
    /// Code, Amp) desynced and corrupted the screen.
    pending_wrap: bool,

    // ── Modes ─────────────────────────────────────────────────────────────────
    mouse_mode: MouseProtocolMode,
    mouse_encoding: MouseProtocolEncoding,
    hide_cursor: bool,
    /// DECSCUSR cursor style (`CSI {n} SP q`): 0 = default. Reconciled to the
    /// outer terminal per frame by the capsule encoder; never forwarded raw.
    cursor_style: u16,
    /// True when visible-screen content changed since the last ED2/ED0-home
    /// preserve; with the byte-equality check below this makes preserve-on-
    /// clear exactly-once (scrollback retention decision, capsule rendering
    /// plan §3.7 candidate (b)).
    mutated_since_preserve: bool,
    /// The exact rows the last preserve pushed, for byte-equality dedupe.
    last_preserved_block: Option<Vec<Vec<Cell>>>,
    bracketed_paste: bool,
    application_cursor: bool,
    focus_events: bool,

    // ── Reported default colors (OSC 10/11 query replies) ────────────────────
    /// Foreground/background RGB the grid reports when the program queries
    /// OSC 10/11. Agents gate their color theming on this answer — codex
    /// renders without any background styling until OSC 11 is answered — so
    /// the query must never go silent. The capsule overwrites these with the
    /// attach client's real terminal colors when the client could read them;
    /// the defaults assume the jackin' dark theme.
    reported_fg: (u8, u8, u8),
    reported_bg: (u8, u8, u8),

    // ── Current SGR attributes (applied to newly written cells) ───────────────
    current_attrs: Attrs,
    active_hyperlink: Option<Hyperlink>,

    // ── Scroll region ─────────────────────────────────────────────────────────
    scroll_top: u16,    // 0-based, inclusive
    scroll_bottom: u16, // 0-based, inclusive
    scroll_ops: Vec<ScrollOp>,

    /// Kitty keyboard protocol stack pushed by the foreground program.
    /// Each `\x1b[>{flags}u` pushes; each `\x1b[<{n}u` pops `n` levels.
    /// The capsule mirrors the top of this stack onto the outer terminal
    /// on focus swap and pops it on focus-out, so it must track depth
    /// exactly — a leaked push leaves the operator's terminal in kitty
    /// mode after focus moves to a plain shell.
    kitty_kb_stack: Vec<u32>,

    // ── Damage + passthrough ──────────────────────────────────────────────────
    pub dirty: DirtyTracker,
    pub passthrough: PassthroughBuffer,
}

impl std::fmt::Debug for DamageGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DamageGrid")
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .field("alt_screen", &self.alt_screen)
            .field("scrollback_len", &self.scrollback.len())
            .field("scrollback_limit", &self.scrollback_limit)
            .field("scrollback_offset", &self.scrollback_offset)
            .field("cursor_row", &self.cursor_row)
            .field("cursor_col", &self.cursor_col)
            .field("hide_cursor", &self.hide_cursor)
            .field("bracketed_paste", &self.bracketed_paste)
            .field("application_cursor", &self.application_cursor)
            .field("focus_events", &self.focus_events)
            .field("kitty_kb_stack_depth", &self.kitty_kb_stack.len())
            .field("dirty", &self.dirty)
            .finish_non_exhaustive()
    }
}

/// Per-pane cap on kitty-keyboard push depth. A buggy or hostile program
/// looping `\x1b[>1u` would otherwise grow `kitty_kb_stack` without bound;
/// 64 is well past any real terminal program's nested keymap-mode depth.
const KITTY_KB_STACK_CAP: usize = 64;

/// Ring-backed row storage.
///
/// This borrows the core idea from Alacritty's `Storage` grid
/// (Apache-2.0/MIT): keep rows in a ring so line scroll/evict operations are
/// row-rotation operations instead of shifting every visible row. Cells remain
/// in plain row vectors so the capsule can borrow contiguous row slices for
/// direct dirty-patch emission.
#[derive(Clone, Debug, Default)]
pub(crate) struct RowStore {
    rows: VecDeque<Vec<Cell>>,
    wraps: VecDeque<RowWrap>,
    arena: RowArena,
}

impl RowStore {
    fn blank(rows: u16, cols: u16, arena: RowArena) -> Self {
        Self {
            rows: (0..rows).map(|_| arena.blank_row(cols)).collect(),
            wraps: (0..rows).map(|_| RowWrap::Hard).collect(),
            arena,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn clear(&mut self) {
        for row in self.rows.drain(..) {
            self.arena.recycle(row);
        }
        self.wraps.clear();
    }

    pub(crate) fn get(&self, row: usize) -> Option<&Vec<Cell>> {
        self.rows.get(row)
    }

    pub(crate) fn wrap(&self, row: usize) -> Option<RowWrap> {
        self.wraps.get(row).copied()
    }

    pub(crate) fn iter(&self) -> vec_deque::Iter<'_, Vec<Cell>> {
        self.rows.iter()
    }

    fn iter_mut(&mut self) -> vec_deque::IterMut<'_, Vec<Cell>> {
        self.rows.iter_mut()
    }

    fn insert(&mut self, index: usize, row: Vec<Cell>) {
        self.insert_with_wrap(index, row, RowWrap::Hard);
    }

    fn insert_with_wrap(&mut self, index: usize, row: Vec<Cell>, wrap: RowWrap) {
        let index = index.min(self.rows.len());
        self.rows.insert(index, row);
        self.wraps.insert(index, wrap);
    }

    fn remove(&mut self, index: usize) -> Option<Vec<Cell>> {
        let row = self.rows.remove(index);
        if row.is_some() {
            let _ = self.wraps.remove(index);
        }
        row
    }

    fn push_back(&mut self, row: Vec<Cell>) {
        self.push_back_with_wrap(row, RowWrap::Hard);
    }

    fn push_back_with_wrap(&mut self, row: Vec<Cell>, wrap: RowWrap) {
        self.rows.push_back(row);
        self.wraps.push_back(wrap);
    }

    fn pop_front(&mut self) -> Option<Vec<Cell>> {
        self.pop_front_with_wrap().map(|(row, _wrap)| row)
    }

    fn pop_front_with_wrap(&mut self) -> Option<(Vec<Cell>, RowWrap)> {
        let row = self.rows.pop_front()?;
        let wrap = self.wraps.pop_front().unwrap_or_default();
        Some((row, wrap))
    }

    fn recycle_front(&mut self) {
        if let Some(row) = self.pop_front() {
            self.arena.recycle(row);
        }
    }
}

impl Drop for RowStore {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Shared row arena for all terminal grids owned by one daemon.
///
/// This is deliberately small and row-level rather than a full Ghostty-style
/// page allocator: prior headless benchmarks showed the hot path is already
/// CPU-cheap, so the first shared store should remove per-session row churn
/// without adding page pins or offset-addressed cell lifetimes.
#[derive(Clone, Debug)]
pub struct RowArena {
    inner: Arc<RowArenaInner>,
}

#[derive(Debug, Default)]
struct RowArenaInner {
    rows: Mutex<Vec<Vec<Cell>>>,
}

impl Default for RowArena {
    fn default() -> Self {
        Self {
            inner: Arc::new(RowArenaInner::default()),
        }
    }
}

impl RowArena {
    /// Shared process-wide arena used by `DamageGrid::new`.
    #[must_use]
    pub fn shared() -> Self {
        static SHARED: OnceLock<RowArena> = OnceLock::new();
        SHARED.get_or_init(Self::default).clone()
    }

    fn lock_rows(&self) -> MutexGuard<'_, Vec<Vec<Cell>>> {
        match self.inner.rows.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn blank_row(&self, cols: u16) -> Vec<Cell> {
        let cols = cols as usize;
        let mut rows = self.lock_rows();
        if let Some(index) = rows.iter().position(|row| row.len() == cols) {
            let mut row = rows.swap_remove(index);
            row.fill(Cell::default());
            row
        } else {
            vec![Cell::default(); cols]
        }
    }

    fn recycle(&self, mut row: Vec<Cell>) {
        const MAX_RECYCLED_ROWS: usize = 4096;
        row.fill(Cell::default());
        let mut rows = self.lock_rows();
        if rows.len() < MAX_RECYCLED_ROWS {
            rows.push(row);
        }
    }

    #[cfg(test)]
    fn recycled_rows(&self) -> usize {
        self.lock_rows().len()
    }
}

impl Index<usize> for RowStore {
    type Output = Vec<Cell>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.rows[index]
    }
}

impl IndexMut<usize> for RowStore {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.rows[index]
    }
}

impl DamageGrid {
    /// Create a new grid with the given dimensions and scrollback limit.
    pub fn new(rows: u16, cols: u16, scrollback_limit: usize) -> Self {
        Self::with_row_arena(rows, cols, scrollback_limit, RowArena::shared())
    }

    /// Create a new grid backed by a caller-provided shared row arena.
    pub fn with_row_arena(
        rows: u16,
        cols: u16,
        scrollback_limit: usize,
        row_arena: RowArena,
    ) -> Self {
        let blank = make_blank_grid(rows, cols, row_arena.clone());
        Self {
            parser: vte::Parser::new(),
            pending_utf8: Vec::new(),
            profile: VirtualTerminalProfile::default(),
            rows,
            cols,
            primary: blank.clone(),
            alternate: blank,
            alt_screen: false,
            scrollback: RowStore {
                rows: VecDeque::new(),
                wraps: VecDeque::new(),
                arena: row_arena,
            },
            scrollback_limit,
            scrollback_offset: 0,
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor_row: 0,
            saved_cursor_col: 0,
            pending_wrap: false,
            mouse_mode: MouseProtocolMode::None,
            mouse_encoding: MouseProtocolEncoding::Default,
            hide_cursor: false,
            cursor_style: 0,
            mutated_since_preserve: false,
            last_preserved_block: None,
            bracketed_paste: false,
            application_cursor: false,
            focus_events: false,
            reported_fg: VirtualTerminalProfile::default().default_reported_fg,
            reported_bg: VirtualTerminalProfile::default().default_reported_bg,
            current_attrs: Attrs::default(),
            active_hyperlink: None,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            scroll_ops: Vec::new(),
            kitty_kb_stack: Vec::new(),
            dirty: DirtyTracker::new(rows),
            passthrough: PassthroughBuffer::default(),
        }
    }

    /// Feed raw PTY bytes through the persistent vte parser, mutating the grid.
    ///
    /// The parser is persisted across calls so that multi-byte escape sequences
    /// split across PTY `read()` boundaries are handled correctly. Creating a new
    /// parser on each call would lose inter-call state and silently drop split
    /// sequences (bug caught by the conformance harness).
    pub fn process(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        let mut combined = Vec::new();
        let bytes = if self.pending_utf8.is_empty() {
            bytes
        } else {
            combined.reserve(self.pending_utf8.len() + bytes.len());
            combined.append(&mut self.pending_utf8);
            combined.extend_from_slice(bytes);
            &combined
        };

        let pending_len = incomplete_utf8_suffix_len(bytes);
        let feed_len = bytes.len() - pending_len;
        if feed_len > 0 {
            self.advance_parser(&bytes[..feed_len]);
        }
        if pending_len > 0 {
            self.pending_utf8.extend_from_slice(&bytes[feed_len..]);
        }
    }

    fn advance_parser(&mut self, bytes: &[u8]) {
        // SAFETY: we need a mutable reference to both self.parser and self (which
        // implements vte::Perform). The parser only reads `bytes`; it calls self
        // through &mut dyn Perform. Rust's borrow rules prevent this directly,
        // so we temporarily move the parser out, advance, then restore it.
        //
        // Alternative: store parser in a separate wrapper or use RefCell. The
        // move-out approach avoids any runtime cost. The parser is always restored
        // before the function returns, so the field is never left empty.
        let mut parser = std::mem::replace(&mut self.parser, vte::Parser::new());
        parser.advance(self, bytes);
        self.parser = parser;
    }

    /// Drain and return the dirty-row set, clearing it for the next frame.
    pub fn dirty_spans(&mut self) -> DirtySpans {
        self.dirty.take()
    }

    /// Drain and return all passthrough events produced during `process()`.
    pub fn drain_passthrough(&mut self) -> Vec<PassthroughEvent> {
        self.passthrough.drain()
    }

    /// Drain dirty spans and snapshot only the rows that changed.
    pub fn dump_dirty_patch(&mut self) -> crate::snapshot::GridPatch<'_> {
        let dirty = self.dirty_spans();
        self.dirty_patch_from(dirty)
    }

    /// Build a borrowed dirty patch from a previously drained dirty-span set.
    pub fn dirty_patch_from(&self, dirty: DirtySpans) -> crate::snapshot::GridPatch<'_> {
        let screen = if self.alt_screen {
            &self.alternate
        } else {
            &self.primary
        };
        crate::snapshot::GridPatch::new(
            self.rows,
            self.cols,
            (self.cursor_row, self.cursor_col),
            self.alt_screen,
            screen,
            dirty,
        )
    }

    /// Dump the current screen state as a `GridSnapshot`.
    ///
    /// The snapshot is a complete, owned copy of the active grid — primary or
    /// alternate — plus cursor position and screen-mode flags. Use it for:
    /// - Acceptance tests that assert exact screen state.
    /// - Terminal observation (feeds the [terminal observation roadmap item]).
    /// - Debugging: `snap.to_text()` gives the visual contents as a string.
    ///
    /// Concept borrowed from `avt` (MIT, Marcin Kulik / asciinema). Implementation
    /// is our own — `avt` is not a dependency. Attribution in `snapshot.rs`.
    pub fn dump(&self) -> crate::snapshot::GridSnapshot {
        let screen = if self.alt_screen {
            &self.alternate
        } else {
            &self.primary
        };
        let cells = screen
            .iter()
            .map(|row| row.iter().map(crate::snapshot::SnapCell::from).collect())
            .collect();
        let row_wraps = (0..screen.len())
            .map(|idx| screen.wrap(idx).unwrap_or_default())
            .collect();
        crate::snapshot::GridSnapshot {
            rows: self.rows,
            cols: self.cols,
            cursor: (self.cursor_row, self.cursor_col),
            alternate_screen: self.alt_screen,
            cells,
            row_wraps,
        }
    }

    // ── Coupling-surface accessors ────────────────────────────────────────────

    /// Grid dimensions `(rows, cols)`.
    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Scroll affordance metrics for rendering the scrollbar chrome.
    ///
    /// Returns `(occupied_rows, viewport_rows, viewport_cols, cursor_row, cursor_col)`.
    /// Used by `view.rs::screen_scroll_affordance_metrics` to compute the scroll
    /// thumb position and dimensions for the focused pane's scrollbar chrome.
    ///
    /// `occupied_rows` is the number of non-blank rows in the current visible screen
    /// (first row whose all cells are blank, counting from the bottom). `0` means the
    /// full viewport is occupied.
    #[must_use]
    pub fn scroll_affordance_metrics(&self) -> (u16, u16, u16, u16, u16) {
        let screen = if self.alt_screen {
            &self.alternate
        } else {
            &self.primary
        };
        // Count non-blank rows from the top (first empty row from bottom gives extent).
        let occupied = screen
            .iter()
            .rposition(|row| row.iter().any(|cell| !cell.contents.is_empty()))
            .map_or(0, |last| (last + 1) as u16);
        (
            occupied,
            self.rows,
            self.cols,
            self.cursor_row,
            self.cursor_col,
        )
    }

    /// Return scrollback rows starting at `offset` lines from the live tail.
    ///
    /// Used by the capsule to render the scrollback view: when `offset > 0`
    /// the operator has scrolled up `offset` lines into history. This function
    /// returns up to `max_rows` rows from the scrollback buffer at that offset.
    ///
    /// The rows are returned newest-first (index 0 = closest to the live tail),
    /// matching how the capsule overlays them above the visible screen.
    ///
    /// Returns an empty slice when `offset == 0` (live view) or when the
    /// scrollback buffer is empty. Clamps `offset` to the actual scrollback length.
    #[must_use]
    pub fn scrollback_rows_at_offset(&self, offset: usize, max_rows: usize) -> Vec<&[Cell]> {
        let len = self.scrollback.len();
        if offset == 0 || len == 0 || max_rows == 0 {
            return Vec::new();
        }
        // scrollback is oldest-first; we want the `offset` most recent rows.
        // start = the index of the row that is `offset` lines back from the tail.
        let clamped = offset.min(len);
        let start = len.saturating_sub(clamped);
        let end = (start + max_rows).min(len);
        (start..end)
            .filter_map(|idx| self.scrollback.get(idx).map(Vec::as_slice))
            .collect()
    }

    /// Dump a scrollback VIEW as a [`GridSnapshot`]: the scrollback rows at
    /// `offset` as a top prefix, then the live screen rows filling the rest of
    /// the `viewport_rows`. `offset == 0` (or empty scrollback) returns the
    /// live `dump()`. This is the snapshot the capsule's Ratatui pane-body
    /// widget renders when the operator has scrolled up — the Ratatui parallel
    /// of `render::pane_snapshot_from_damagegrid_with_scrollback`, kept here so
    /// it composes the same prefix/tail layout from one place.
    #[must_use]
    pub fn dump_scrollback_view(
        &self,
        offset: usize,
        viewport_rows: u16,
    ) -> crate::snapshot::GridSnapshot {
        let live = self.dump();
        if offset == 0 {
            return live;
        }
        let rows_to_draw = viewport_rows.min(self.rows);
        let sb = self.scrollback_rows_at_offset(offset, rows_to_draw as usize);
        if sb.is_empty() {
            return live;
        }
        let prefix = sb.len().min(rows_to_draw as usize);
        let mut cells: Vec<Vec<crate::snapshot::SnapCell>> =
            Vec::with_capacity(rows_to_draw as usize);
        for sb_row in sb.iter().take(prefix) {
            cells.push(sb_row.iter().map(crate::snapshot::SnapCell::from).collect());
        }
        for live_row in live.cells.iter().take(rows_to_draw as usize - prefix) {
            cells.push(live_row.clone());
        }
        let mut row_wraps = Vec::with_capacity(rows_to_draw as usize);
        let clamped = offset.min(self.scrollback.len());
        let start = self.scrollback.len().saturating_sub(clamped);
        for idx in start..start + prefix {
            row_wraps.push(self.scrollback.wrap(idx).unwrap_or_default());
        }
        row_wraps.extend(
            live.row_wraps
                .iter()
                .copied()
                .take(rows_to_draw as usize - prefix),
        );
        crate::snapshot::GridSnapshot {
            rows: rows_to_draw,
            cols: self.cols,
            cursor: live.cursor,
            alternate_screen: live.alternate_screen,
            cells,
            row_wraps,
        }
    }

    /// Borrow a scrollback VIEW without allocating row snapshots.
    ///
    /// The row layout matches [`DamageGrid::dump_scrollback_view`]: with
    /// `offset == 0` this is the live screen, otherwise the top prefix is taken
    /// from scrollback and the remaining rows are live screen rows.
    #[must_use]
    pub fn scrollback_view(
        &self,
        offset: usize,
        viewport_rows: u16,
    ) -> crate::snapshot::GridView<'_> {
        let screen = if self.alt_screen {
            &self.alternate
        } else {
            &self.primary
        };
        let cursor = (self.cursor_row, self.cursor_col);
        if offset == 0 || self.scrollback.is_empty() || viewport_rows == 0 {
            return crate::snapshot::GridView::new(crate::snapshot::GridViewParts {
                rows: self.rows,
                cols: self.cols,
                cursor,
                alternate_screen: self.alt_screen,
                screen,
                scrollback: &self.scrollback,
                scrollback_start: 0,
                scrollback_prefix: 0,
            });
        }

        let rows_to_draw = viewport_rows.min(self.rows);
        let clamped = offset.min(self.scrollback.len());
        let start = self.scrollback.len().saturating_sub(clamped);
        let prefix = ((start + usize::from(rows_to_draw)).min(self.scrollback.len()) - start)
            .min(usize::from(rows_to_draw));
        crate::snapshot::GridView::new(crate::snapshot::GridViewParts {
            rows: rows_to_draw,
            cols: self.cols,
            cursor,
            alternate_screen: self.alt_screen,
            screen,
            scrollback: &self.scrollback,
            scrollback_start: start,
            scrollback_prefix: prefix,
        })
    }

    /// Resize the grid. Marks all rows dirty.
    pub fn set_size(&mut self, rows: u16, cols: u16) {
        // The grid must always keep at least one addressable cell. The parser
        // indexes `active_grid()[cursor_row]` directly on every erase/write, so
        // a 0-row or 0-col grid turns the next PTY byte into an out-of-bounds
        // panic on the empty `VecDeque`. This is reachable in practice: a
        // capsule pane squeezed below its border height under an extreme
        // resize hands `set_size` a 0-row inner rect. Clamp to 1×1 so the
        // model stays well-formed regardless of the geometry the host computes.
        let rows = rows.max(1);
        let cols = cols.max(1);
        self.rows = rows;
        self.cols = cols;
        self.primary = resize_grid(&self.primary, rows, cols);
        self.alternate = resize_grid(&self.alternate, rows, cols);
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        // Resize is a cursor-moving path: cancel any deferred (DECAWM) wrap so a
        // pending wrap from a pre-resize last-column write does not fire on the
        // next printable and drop the glyph a row down. The clamp above already
        // pulled cursor_col into range, so clear_pending_wrap only un-arms the flag.
        self.clear_pending_wrap();
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.dirty.resize(rows);
    }

    /// Get a cell reference. Returns `None` if out of bounds.
    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        let screen = if self.alt_screen {
            &self.alternate
        } else {
            &self.primary
        };
        screen.get(row as usize).and_then(|r| r.get(col as usize))
    }

    /// Cursor position `(row, col)`.
    pub fn cursor_position(&self) -> (u16, u16) {
        (self.cursor_row, self.cursor_col)
    }

    /// Whether the alternate screen is active.
    pub fn alternate_screen(&self) -> bool {
        self.alt_screen
    }

    /// Set the scrollback view offset. 0 = live tail; `scrollback_limit` = oldest.
    pub fn set_scrollback(&mut self, offset: usize) {
        self.scrollback_offset = offset.min(self.scrollback.len());
    }

    /// Current scrollback view offset.
    pub fn scrollback(&self) -> usize {
        self.scrollback_offset
    }

    /// DECSCUSR cursor style requested by the program (`0` = default).
    pub fn cursor_style(&self) -> u16 {
        self.cursor_style
    }

    /// Number of scrollback rows filled.
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Clear the scrollback buffer. Used by capsule's `clear_scrollback`.
    pub fn clear_scrollback(&mut self) {
        self.scrollback.clear();
        self.scrollback_offset = 0;
    }

    pub fn drain_scroll_ops(&mut self) -> Vec<ScrollOp> {
        std::mem::take(&mut self.scroll_ops)
    }

    /// Mouse protocol mode.
    pub fn mouse_protocol_mode(&self) -> MouseProtocolMode {
        self.mouse_mode
    }

    /// Mouse protocol encoding.
    pub fn mouse_protocol_encoding(&self) -> MouseProtocolEncoding {
        self.mouse_encoding
    }

    pub fn hide_cursor(&self) -> bool {
        self.hide_cursor
    }

    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    pub fn application_cursor(&self) -> bool {
        self.application_cursor
    }

    /// Whether the terminal has enabled focus-event reporting (DEC 1004).
    ///
    /// Mirrors the DEC mode state tracked internally so callers do not need
    /// to maintain their own copy by draining `PassthroughEvent::FocusEvents`.
    pub fn focus_events(&self) -> bool {
        self.focus_events
    }

    /// Reset the poll-based terminal modes to their power-on defaults.
    ///
    /// RIS (`ESC c`) is a full power-on reset; without this the cursor could
    /// stay hidden or mouse reporting stay armed after a program emits RIS to
    /// recover. These fields are read back by the capsule each frame, so the
    /// reset is observed without a passthrough event.
    pub(crate) fn reset_modes(&mut self) {
        self.mouse_mode = MouseProtocolMode::None;
        self.mouse_encoding = MouseProtocolEncoding::Default;
        self.hide_cursor = false;
        self.bracketed_paste = false;
        self.application_cursor = false;
        self.focus_events = false;
        self.scrollback_offset = 0;
    }

    /// Top of the kitty-keyboard stack (`0` when empty). The capsule
    /// re-asserts this on the outer terminal when the pane gains focus.
    pub fn kitty_kb_flags(&self) -> u32 {
        self.kitty_kb_stack.last().copied().unwrap_or(0)
    }

    /// Clear the kitty-keyboard stack. Called by the capsule on
    /// alternate-screen exit so a full-screen program that pushed a
    /// kitty level cannot leave the following shell prompt in that mode.
    pub fn clear_kitty_kb_stack(&mut self) {
        self.kitty_kb_stack.clear();
    }

    // ── Internal grid helpers ─────────────────────────────────────────────────

    fn active_grid(&mut self) -> &mut RowStore {
        if self.alt_screen {
            &mut self.alternate
        } else {
            &mut self.primary
        }
    }

    /// Write a character at the current cursor position, advance cursor.
    fn write_char_at_cursor(&mut self, ch: char) {
        let width = self.profile.char_width(ch);
        self.mutated_since_preserve = true;

        // Grapheme clustering: zero-width input (combining marks, variation
        // selectors, ZWJ) joins the previously written cell instead of
        // overwriting it, and any character following a ZWJ continues that
        // cluster. Cluster width stays the base width — DECRQM declines mode
        // 2027, so the outer terminal advances by `unicode_width` too.
        if self.append_to_previous_cluster(ch, width) {
            return;
        }

        // DECAWM deferred wrap: a previous last-column write parked here. Now
        // that a new printable (not a cluster continuation) has arrived,
        // perform the wrap before writing.
        if self.pending_wrap {
            self.pending_wrap = false;
            self.cursor_col = 0;
            self.newline_action_with_wrap(RowWrap::Soft);
        }
        if self.cursor_row >= self.rows || self.cursor_col >= self.cols {
            return;
        }
        let row = self.cursor_row as usize;
        let col = self.cursor_col as usize;

        // Erase any prior wide char that would be partially overwritten —
        // in both directions: writing over a continuation blanks its lead,
        // and writing over a lead blanks its orphaned continuation.
        let mut dirty_start = self.cursor_col;
        let mut dirty_end = self.cursor_col.saturating_add(width);
        {
            let grid = self.active_grid();
            if col < grid[row].len() && grid[row][col].is_wide_continuation && col > 0 {
                grid[row][col - 1] = Cell::default();
                dirty_start = dirty_start.saturating_sub(1);
            }
            if col < grid[row].len()
                && grid[row][col].is_wide
                && width < 2
                && col + 1 < grid[row].len()
            {
                grid[row][col + 1] = Cell::default();
                dirty_end = dirty_end.max(self.cursor_col.saturating_add(2));
            }
        }

        let attrs = self.current_attrs.clone();
        let cols = self.cols;
        let cell = Cell {
            // Phase 4: CompactString stores ch inline (no heap alloc for ASCII + most Unicode).
            contents: compact_str::format_compact!("{ch}"),
            is_wide: width > 1,
            is_wide_continuation: false,
            attrs: attrs.clone(),
            hyperlink: self.active_hyperlink.clone(),
        };
        {
            let hyperlink = self.active_hyperlink.clone();
            let grid = self.active_grid();
            grid[row][col] = cell;
            if width > 1 && col + 1 < cols as usize && col + 1 < grid[row].len() {
                grid[row][col + 1] = Cell {
                    contents: compact_str::CompactString::new(""),
                    is_wide: false,
                    is_wide_continuation: true,
                    attrs: attrs.clone(),
                    hyperlink,
                };
            }
        }
        self.dirty
            .mark_range(self.cursor_row, dirty_start, dirty_end);

        self.cursor_col += width;
        if self.cursor_col >= self.cols {
            // Last-column write: defer the wrap (DECAWM). Park the cursor at
            // the phantom column (== cols, matching DEC autowrap semantics)
            // and arm pending_wrap; the next printable performs the wrap, and
            // any explicit cursor move cancels it. Eager wrapping here is what
            // drifted the cursor one row down per border line.
            self.cursor_col = self.cols;
            self.pending_wrap = true;
        }
    }

    /// Join `ch` to the cluster in the previously written cell when it is
    /// zero-width (combining mark, VS16, ZWJ) or continues a ZWJ sequence.
    /// Returns true when the character was absorbed.
    fn append_to_previous_cluster(&mut self, ch: char, width: u16) -> bool {
        let zero_width = width == 0;
        let target_col = if self.pending_wrap || self.cursor_col >= self.cols {
            self.cols.saturating_sub(1)
        } else if self.cursor_col > 0 {
            self.cursor_col - 1
        } else if zero_width {
            // Combining mark with nothing before it on this row: drop rather
            // than corrupt cell zero.
            return true;
        } else {
            return false;
        };
        let row = self.cursor_row as usize;
        let mut col = target_col as usize;
        // Resolve the lead cell and snapshot what we need from it under one
        // borrow, before the cursor/profile reads below re-borrow `self`.
        let (old_width, attrs, mut joined) = {
            let grid = self.active_grid();
            if col >= grid[row].len() {
                return zero_width;
            }
            // Step from a continuation cell back to its wide lead.
            if grid[row][col].is_wide_continuation && col > 0 {
                col -= 1;
            }
            let cell = &grid[row][col];
            if !zero_width && !cell.contents.ends_with('\u{200d}') {
                return false;
            }
            if cell.contents.is_empty() && zero_width {
                // Nothing to join — drop the orphan mark.
                return true;
            }
            (
                cell_width(cell),
                cell.attrs.clone(),
                compact_str::CompactString::new(&cell.contents),
            )
        };
        let old_cursor_end = col as u16 + old_width;
        joined.push(ch);
        let new_width = self.profile.cluster_width(joined.as_str()).min(2);
        {
            let cols = self.cols as usize;
            let hyperlink = self.active_hyperlink.clone();
            let grid = self.active_grid();
            grid[row][col].contents = joined;
            grid[row][col].hyperlink = hyperlink;
            set_cell_width(&mut grid[row], col, new_width, attrs, cols);
        }
        if self.cursor_row == row as u16
            && self.cursor_col == old_cursor_end
            && self.cursor_col < self.cols
        {
            self.cursor_col = (col as u16 + new_width).min(self.cols);
        }
        if self.cursor_row == row as u16
            && (self.pending_wrap || self.cursor_col >= self.cols)
            && col as u16 == self.cols.saturating_sub(1)
            && new_width > old_width
        {
            self.cursor_col = self.cols;
            self.pending_wrap = true;
        }
        let mark_start = col as u16;
        let mark_end = mark_start.saturating_add(old_width.max(new_width).max(1));
        self.dirty.mark_range(self.cursor_row, mark_start, mark_end);
        true
    }

    /// Scroll the active scroll region up by `n` rows, pushing content to scrollback.
    fn scroll_up(&mut self, n: u16, inserted_wrap: RowWrap) {
        self.mutated_since_preserve = true;
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;
        let cols = self.cols;
        if n > 0 {
            self.scroll_ops.push(ScrollOp::Up {
                top: self.scroll_top,
                bottom: self.scroll_bottom,
                rows: n,
            });
        }
        for _ in 0..n {
            let grid_len = self.active_grid().len();
            if bottom >= grid_len || top >= bottom {
                continue;
            }
            // top == 0 with no bottom margin is the common case (plain line feed
            // with no DECSTBM region); scrollback only collects primary rows.
            let to_scrollback = !self.alt_screen && top == 0 && self.scrollback_limit > 0;
            // Scroll-introduced blank lines use the DEFAULT background, not the
            // current one — xterm applies back-colour-erase to the explicit
            // erase ops (EL/ED/ECH), never to scroll/insert-line.
            if top == 0 && bottom + 1 == grid_len {
                // Full-region scroll: rotate the ring instead of cloning every
                // visible row. The evicted top row moves straight into
                // scrollback (no intermediate clone) or is recycled; the new
                // bottom row is drawn from the arena recycle pool.
                let blank = self.active_grid().arena.blank_row(cols);
                let evicted = self.active_grid().pop_front_with_wrap();
                if let Some((row, wrap)) = evicted {
                    if to_scrollback {
                        if self.scrollback.len() >= self.scrollback_limit {
                            self.scrollback.recycle_front();
                        }
                        self.scrollback.push_back_with_wrap(row, wrap);
                    } else {
                        self.active_grid().arena.recycle(row);
                    }
                }
                self.active_grid().push_back_with_wrap(blank, inserted_wrap);
            } else {
                // Partial scroll region (DECSTBM margins) or a non-zero top:
                // the ring cannot rotate without disturbing rows outside the
                // region, so shift within range.
                if to_scrollback {
                    let row = self.primary[0].clone();
                    let wrap = self.primary.wrap(0).unwrap_or_default();
                    if self.scrollback.len() >= self.scrollback_limit {
                        self.scrollback.recycle_front();
                    }
                    self.scrollback.push_back_with_wrap(row, wrap);
                }
                let grid = self.active_grid();
                for r in top..bottom {
                    grid[r] = grid[r + 1].clone();
                    if let Some(next_wrap) = grid.wrap(r + 1) {
                        grid.wraps[r] = next_wrap;
                    }
                }
                grid[bottom] = blank_row(cols);
                grid.wraps[bottom] = inserted_wrap;
            }
        }
        for r in top as u16..=bottom as u16 {
            self.dirty.mark_row(r);
        }
    }

    /// Push the visible primary-screen rows into scrollback before a
    /// full-screen clear (ED2, or ED0 at the home cursor). Only the
    /// non-blank span (first non-blank row through last non-blank row) is
    /// kept, matching how a shell or inline agent TUI redraws over its own
    /// transcript. No-op on the alternate screen — full-screen apps own
    /// their display and do not contribute scrollback.
    fn preserve_visible_rows_to_scrollback(&mut self) {
        if self.alt_screen || self.scrollback_limit == 0 {
            return;
        }
        // Retention decision (capsule rendering plan §3.7, candidate (b)):
        // preserve-on-clear with exact dedupe. A clear that arrives with no
        // content mutation since the previous preserve, or whose visible
        // block is byte-identical to the last preserved block, retains
        // nothing — repeated ED2 repaint cycles cannot duplicate the
        // transcript (D11) while cleared-but-never-scrolled screens stay
        // recoverable.
        if !self.mutated_since_preserve {
            return;
        }
        let Some(first) = self
            .primary
            .iter()
            .position(|row| row.iter().any(|cell| !cell.contents.is_empty()))
        else {
            return;
        };
        let Some(last) = self
            .primary
            .iter()
            .rposition(|row| row.iter().any(|cell| !cell.contents.is_empty()))
        else {
            return;
        };
        // Compare the visible span against the last preserved block in place:
        // the common repeated-ED2 repaint cycle hits this and bails without
        // cloning a single row. Only a genuine miss materializes the block.
        let unchanged = self.last_preserved_block.as_ref().is_some_and(|prev| {
            prev.len() == last - first + 1
                && prev
                    .iter()
                    .zip(first..=last)
                    .all(|(prev_row, idx)| prev_row.as_slice() == self.primary[idx].as_slice())
        });
        if unchanged {
            self.mutated_since_preserve = false;
            return;
        }
        let block: Vec<Vec<Cell>> = (first..=last)
            .map(|idx| self.primary[idx].clone())
            .collect();
        for row in &block {
            if self.scrollback.len() >= self.scrollback_limit {
                self.scrollback.recycle_front();
            }
            self.scrollback.push_back(row.clone());
        }
        self.last_preserved_block = Some(block);
        self.mutated_since_preserve = false;
    }

    /// Newline action: move down or scroll.
    fn newline_action(&mut self) {
        self.newline_action_with_wrap(RowWrap::Hard);
    }

    fn newline_action_with_wrap(&mut self, wrap: RowWrap) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1, wrap);
        } else {
            self.cursor_row =
                Self::add_cursor_offset(self.cursor_row, 1, self.rows.saturating_sub(1));
            self.set_active_row_wrap(self.cursor_row, wrap);
        }
    }

    fn set_active_row_wrap(&mut self, row: u16, wrap: RowWrap) {
        let idx = usize::from(row);
        let grid = self.active_grid();
        if let Some(row_wrap) = grid.wraps.get_mut(idx) {
            *row_wrap = wrap;
        }
    }

    /// Cancel a deferred (DECAWM) wrap. Any explicit cursor move clears the
    /// pending state, and un-parks the cursor from the phantom column (== cols)
    /// back into the addressable range so the subsequent move computes from a
    /// valid column.
    fn clear_pending_wrap(&mut self) {
        self.pending_wrap = false;
        if self.cursor_col >= self.cols {
            self.cursor_col = self.cols.saturating_sub(1);
        }
    }

    fn clamp_cursor(&mut self) {
        self.cursor_row = self.cursor_row.min(self.rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(self.cols.saturating_sub(1));
    }

    fn add_cursor_offset(position: u16, offset: u16, max: u16) -> u16 {
        position.saturating_add(offset).min(max)
    }

    /// A blank cell carrying the current background colour (back-colour-erase).
    /// Erase (EL/ED/ECH), scroll, and insert/delete-line introduce blanks that
    /// must inherit the active SGR background — xterm-with-`bce` does this.
    /// Without it, a region an agent clears while a background colour is
    /// selected loses that colour, and its cell-at-a-time redraws (CHA-
    /// positioned, cell-at-a-time) desync from the grid, corrupting the screen.
    fn blank_cell(&self) -> Cell {
        let mut cell = Cell::default();
        cell.attrs.background = self.current_attrs.background;
        cell
    }

    fn blank_row_bce(&self) -> Vec<Cell> {
        vec![self.blank_cell(); self.cols as usize]
    }

    fn erase_line(&mut self, mode: u16) {
        let row = self.cursor_row as usize;
        let col = self.cursor_col as usize;
        let cols_u16 = self.cols;
        let cols = cols_u16 as usize;
        let cursor_row = self.cursor_row;
        let blank = self.blank_cell();
        let blank_row = self.blank_row_bce();
        {
            let grid = self.active_grid();
            match mode {
                0 => {
                    grid[row][col..cols].fill(blank);
                    self.dirty.mark_range(cursor_row, self.cursor_col, cols_u16);
                }
                1 => {
                    grid[row][0..=col.min(cols - 1)].fill(blank);
                    self.dirty
                        .mark_range(cursor_row, 0, self.cursor_col.saturating_add(1));
                }
                2 => {
                    grid[row] = blank_row;
                    self.dirty.mark_row(cursor_row);
                }
                _ => {}
            }
        }
    }

    fn erase_display(&mut self, mode: u16) {
        let cursor_row = self.cursor_row as usize;
        let cursor_col = self.cursor_col as usize;
        let rows = self.rows as usize;
        let cols_usize = self.cols as usize;
        let blank = self.blank_cell();
        match mode {
            0 => {
                // ED0 at the home cursor is the normal-screen clear/redraw
                // shape shells and inline agent TUIs use; preserve the
                // visible rows as scrollback before blanking so the operator
                // can scroll back to read them.
                if self.cursor_row == 0 && self.cursor_col == 0 {
                    self.preserve_visible_rows_to_scrollback();
                }
                let blank_row = self.blank_row_bce();
                let grid = self.active_grid();
                grid[cursor_row][cursor_col..cols_usize].fill(blank);
                for row in grid.iter_mut().take(rows).skip(cursor_row + 1) {
                    *row = blank_row.clone();
                }
            }
            1 => {
                let blank_row = self.blank_row_bce();
                let grid = self.active_grid();
                for row in grid.iter_mut().take(cursor_row) {
                    *row = blank_row.clone();
                }
                grid[cursor_row][0..=cursor_col.min(cols_usize - 1)].fill(blank);
            }
            2 => {
                // ED2 clears the whole visible display; preserve those rows
                // as scrollback. ED3 below is the explicit saved-lines clear
                // and must NOT preserve them.
                self.preserve_visible_rows_to_scrollback();
                let blank_row = self.blank_row_bce();
                let grid = self.active_grid();
                for row in grid.iter_mut().take(rows) {
                    *row = blank_row.clone();
                }
            }
            3 => {
                self.scrollback.clear();
                self.scrollback_offset = 0;
                let blank_row = self.blank_row_bce();
                let grid = self.active_grid();
                for row in grid.iter_mut().take(rows) {
                    *row = blank_row.clone();
                }
                // Emit ScrollbackClear so the capsule can clear its retained history.
                self.passthrough.push(PassthroughEvent::ScrollbackClear);
            }
            _ => {}
        }
        self.dirty.mark_all();
    }

    /// Set the default colors reported to OSC 10/11 queries. The capsule
    /// calls this with the attach client's real terminal colors so agents
    /// theme against what the operator actually sees. `None` keeps the
    /// current value — the dark-theme default until any client reports, and
    /// the last reporting client's palette across a reattach from a
    /// terminal that could not answer (a better guess than resetting to the
    /// baked-in default).
    pub fn set_reported_colors(&mut self, fg: Option<(u8, u8, u8)>, bg: Option<(u8, u8, u8)>) {
        if let Some(fg) = fg {
            self.reported_fg = fg;
        }
        if let Some(bg) = bg {
            self.reported_bg = bg;
        }
    }

    /// Encode an OSC 10/11 color reply. xterm convention: 16-bit channels
    /// (`rgb:RRRR/GGGG/BBBB`, 8-bit value doubled into both bytes), reply
    /// terminator mirrors the query's (BEL stays BEL, ST stays ST).
    fn osc_color_reply(code: u8, (r, g, b): (u8, u8, u8), bell_terminated: bool) -> Vec<u8> {
        let terminator = if bell_terminated { "\x07" } else { "\x1b\\" };
        format!(
            "\x1b]{code};rgb:{:04x}/{:04x}/{:04x}{terminator}",
            u16::from(r) * 0x101,
            u16::from(g) * 0x101,
            u16::from(b) * 0x101,
        )
        .into_bytes()
    }

    fn handle_osc(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let Some(&code_bytes) = params.first() else {
            return;
        };
        let code = std::str::from_utf8(code_bytes)
            .ok()
            .and_then(|s| s.parse::<u8>().ok());
        let value = params.get(1).and_then(|b| std::str::from_utf8(b).ok());
        match (code, value) {
            (Some(0 | 2), Some(title)) => {
                self.passthrough
                    .push(PassthroughEvent::TitleChanged(title.to_owned()));
            }
            (Some(1), Some(name)) => {
                self.passthrough
                    .push(PassthroughEvent::IconNameChanged(name.to_owned()));
            }
            (Some(52), Some(_)) => {
                // OSC 52 format: 52;<sel>;<b64data>
                // params[1] = selection ("c" for clipboard), params[2] = base64 data.
                // Re-join all params after the code so the full "c;SGVsbG8=" payload
                // is preserved for re-encoding as "\x1b]52;c;SGVsbG8=\x07".
                let payload: String = params[1..]
                    .iter()
                    .filter_map(|b| std::str::from_utf8(b).ok())
                    .collect::<Vec<_>>()
                    .join(";");
                self.passthrough
                    .push(PassthroughEvent::ClipboardWrite(payload));
            }
            (Some(7), Some(uri)) => {
                self.passthrough
                    .push(PassthroughEvent::CwdChanged(uri.to_owned()));
            }
            (Some(9), Some(msg)) => {
                self.passthrough
                    .push(PassthroughEvent::Notification(msg.to_owned()));
            }
            // OSC 10/11 color queries — answer from the grid's stored
            // defaults, never the host (§3.6). Agents gate their theming on
            // this reply: codex paints no backgrounds at all until OSC 11 is
            // answered. Set forms (a color payload instead of `?`) are
            // dropped like any other unhandled OSC.
            (Some(code @ (10 | 11)), Some("?")) => {
                let rgb = if code == 10 {
                    self.reported_fg
                } else {
                    self.reported_bg
                };
                self.passthrough
                    .push(PassthroughEvent::Reply(Self::osc_color_reply(
                        code,
                        rgb,
                        bell_terminated,
                    )));
            }
            // OSC 8: hyperlink — model as cell metadata. The capsule applies
            // URI-scheme safety filtering when converting metadata to frame
            // OSC 8 spans.
            (Some(8), _) => {
                let id = params
                    .get(1)
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .unwrap_or("")
                    .trim_start_matches("id=")
                    .to_owned();
                let uri = params
                    .get(2)
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .unwrap_or("")
                    .to_owned();
                self.active_hyperlink = (!uri.is_empty()).then_some(Hyperlink { id, uri });
            }
            _ => {}
        }
    }
}

mod perform;

// ── SGR / DEC helpers ─────────────────────────────────────────────────────

impl DamageGrid {
    fn apply_sgr_params(&mut self, params: &vte::Params) {
        // Borrow each subparameter slice rather than cloning into owned Vecs —
        // SGR runs on the per-byte PTY parse hot path.
        let params = params.iter().collect::<Vec<&[u16]>>();
        self.apply_sgr(&params);
    }

    fn apply_sgr(&mut self, params: &[&[u16]]) {
        let mut i = 0;
        if params.is_empty() {
            self.current_attrs = Attrs::default();
            return;
        }
        while i < params.len() {
            let param = params[i];
            let code = param.first().copied().unwrap_or(0);
            match code {
                0 => {
                    self.current_attrs = Attrs::default();
                }
                1 => self.current_attrs.bold = true,
                2 => self.current_attrs.dim = true,
                3 => self.current_attrs.italic = true,
                4 => {
                    self.current_attrs.underline_style =
                        underline_style_from_sgr(param.get(1).copied().unwrap_or(1));
                }
                5 => self.current_attrs.slow_blink = true,
                6 => self.current_attrs.rapid_blink = true,
                7 => self.current_attrs.inverse = true,
                8 => self.current_attrs.conceal = true,
                9 => self.current_attrs.strikethrough = true,
                21 => self.current_attrs.underline_style = UnderlineStyle::Double,
                22 => {
                    self.current_attrs.bold = false;
                    self.current_attrs.dim = false;
                }
                23 => self.current_attrs.italic = false,
                24 => self.current_attrs.underline_style = UnderlineStyle::None,
                25 => {
                    self.current_attrs.slow_blink = false;
                    self.current_attrs.rapid_blink = false;
                }
                27 => self.current_attrs.inverse = false,
                28 => self.current_attrs.conceal = false,
                29 => self.current_attrs.strikethrough = false,
                // Standard 16 colors — foreground.
                30..=37 => {
                    self.current_attrs.foreground = Color::Idx(code as u8 - 30);
                }
                38 => {
                    if let Some(color) = parse_sgr_color(param, params, &mut i) {
                        self.current_attrs.foreground = color;
                    }
                }
                39 => self.current_attrs.foreground = Color::Default,
                // Standard 16 colors — background.
                40..=47 => {
                    self.current_attrs.background = Color::Idx(code as u8 - 40);
                }
                48 => {
                    if let Some(color) = parse_sgr_color(param, params, &mut i) {
                        self.current_attrs.background = color;
                    }
                }
                49 => self.current_attrs.background = Color::Default,
                53 => self.current_attrs.overline = true,
                55 => self.current_attrs.overline = false,
                58 => {
                    if let Some(color) = parse_sgr_color(param, params, &mut i) {
                        self.current_attrs.underline_color = color;
                    }
                }
                59 => self.current_attrs.underline_color = Color::Default,
                // Bright foreground (90-97).
                90..=97 => {
                    self.current_attrs.foreground = Color::Idx(code as u8 - 90 + 8);
                }
                // Bright background (100-107).
                100..=107 => {
                    self.current_attrs.background = Color::Idx(code as u8 - 100 + 8);
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn set_dec_mode(&mut self, mode: u16, enabled: bool) {
        match mode {
            // Show/hide cursor.
            25 => self.hide_cursor = !enabled,
            // Application/normal cursor keys — emit for passthrough.
            1 => {
                self.application_cursor = enabled;
                self.passthrough
                    .push(PassthroughEvent::ApplicationCursorKeys(enabled));
            }
            // Alternate screen (simple form, no cursor save).
            47 => self.set_alt_screen(enabled),
            // Focus events — track state and emit for passthrough.
            1004 => {
                self.focus_events = enabled;
                self.passthrough
                    .push(PassthroughEvent::FocusEvents(enabled));
            }
            // Bracketed paste — emit for passthrough.
            2004 => {
                self.bracketed_paste = enabled;
                self.passthrough
                    .push(PassthroughEvent::BracketedPaste(enabled));
            }
            // Alternate screen (save/restore cursor).
            // Mode 1047: switch only (no cursor save/restore).
            // Mode 1049: save cursor before entering alt screen, restore after leaving.
            1047 => self.set_alt_screen(enabled),
            1049 => {
                if enabled {
                    self.saved_cursor_row = self.cursor_row;
                    self.saved_cursor_col = self.cursor_col;
                    self.set_alt_screen(true);
                    self.cursor_row = 0;
                    self.cursor_col = 0;
                } else {
                    self.set_alt_screen(false);
                    self.cursor_row = self.saved_cursor_row;
                    self.cursor_col = self.saved_cursor_col;
                    self.clamp_cursor();
                    // Reset kitty keyboard stack on alt-screen exit: programs in
                    // the alt screen may not clean up their keyboard mode stack,
                    // so pop all pushed levels and emit one reset per level so
                    // the outer terminal's keyboard mode is restored correctly.
                    let depth = self.kitty_kb_stack.len();
                    self.kitty_kb_stack.clear();
                    for _ in 0..depth {
                        self.passthrough
                            .push(PassthroughEvent::UnhandledCsi(b"\x1b[<u".to_vec()));
                    }
                }
            }
            // Mouse modes.
            1000 => {
                self.mouse_mode = if enabled {
                    MouseProtocolMode::Press
                } else {
                    MouseProtocolMode::None
                };
            }
            // Mode 1002: button-press + release + motion while button held.
            // Use ButtonMotion so tui/input.rs can match without conversion.
            1002 => {
                self.mouse_mode = if enabled {
                    MouseProtocolMode::ButtonMotion
                } else {
                    MouseProtocolMode::None
                };
            }
            // Mode 1003: any motion.
            1003 => {
                self.mouse_mode = if enabled {
                    MouseProtocolMode::AnyMotion
                } else {
                    MouseProtocolMode::None
                };
            }
            1005 => {
                self.mouse_encoding = if enabled {
                    MouseProtocolEncoding::Utf8
                } else {
                    MouseProtocolEncoding::Default
                };
            }
            1006 => {
                self.mouse_encoding = if enabled {
                    MouseProtocolEncoding::Sgr
                } else {
                    MouseProtocolEncoding::Default
                };
            }
            1015 => {
                self.mouse_encoding = if enabled {
                    MouseProtocolEncoding::Urxvt
                } else {
                    MouseProtocolEncoding::Default
                };
            }
            // Synchronized output (?2026): absorbed. The capsule's own frame
            // brackets supersede the agent's — forwarding the agent's BSU/ESU
            // on its own schedule decoupled them from frame timing, and a
            // dropped ESU froze the outer terminal (D6).
            2026 => {}
            _ => {}
        }
    }

    fn set_alt_screen(&mut self, active: bool) {
        self.alt_screen = active;
        self.dirty.mark_all();
    }
}

// ── Parse helpers ─────────────────────────────────────────────────────────

/// Reconstruct the raw bytes of a CSI sequence from its parsed components,
/// for forwarding unhandled sequences verbatim to the outer terminal.
///
/// Sub-params (vte's `&[u16]` per top-level param) are joined with `:`,
/// top-level params with `;`. Example: `[[1, 2], [3]]` with final `m`
/// → `\x1b[1:2;3m`.
fn reconstruct_csi(params: &vte::Params, intermediates: &[u8], final_byte: u8) -> Vec<u8> {
    use std::io::Write as _;
    let mut buf = b"\x1b[".to_vec();
    buf.extend_from_slice(intermediates);
    for (idx, sub) in params.iter().enumerate() {
        if idx > 0 {
            buf.push(b';');
        }
        for (jdx, n) in sub.iter().enumerate() {
            if jdx > 0 {
                buf.push(b':');
            }
            let _unused = write!(buf, "{n}");
        }
    }
    buf.push(final_byte);
    buf
}

/// Parse extended color from SGR params starting at `i`.
fn underline_style_from_sgr(style: u16) -> UnderlineStyle {
    match style {
        0 => UnderlineStyle::None,
        1 => UnderlineStyle::Single,
        2 => UnderlineStyle::Double,
        3 => UnderlineStyle::Curly,
        4 => UnderlineStyle::Dotted,
        5 => UnderlineStyle::Dashed,
        _ => UnderlineStyle::Single,
    }
}

/// Parse extended color from either colon subparameters (`38:2:r:g:b`) or
/// semicolon parameters (`38;2;r;g;b`). Advances `i` for semicolon forms.
fn parse_sgr_color(current: &[u16], params: &[&[u16]], i: &mut usize) -> Option<Color> {
    if current.len() > 1 {
        return parse_sgr_color_values(&current[1..]);
    }
    if *i + 1 >= params.len() {
        return None;
    }
    let mode = params[*i + 1].first().copied().unwrap_or(0);
    match mode {
        5 => {
            if *i + 2 < params.len() {
                let idx = params[*i + 2].first().copied().unwrap_or(0).min(255) as u8;
                *i += 2;
                Some(Color::Idx(idx))
            } else {
                None
            }
        }
        2 => {
            if *i + 4 < params.len() {
                let r = params[*i + 2].first().copied().unwrap_or(0).min(255) as u8;
                let g = params[*i + 3].first().copied().unwrap_or(0).min(255) as u8;
                let b = params[*i + 4].first().copied().unwrap_or(0).min(255) as u8;
                *i += 4;
                Some(Color::Rgb(r, g, b))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_sgr_color_values(values: &[u16]) -> Option<Color> {
    match values.first().copied()? {
        5 => values.get(1).map(|idx| Color::Idx((*idx).min(255) as u8)),
        2 => {
            let start = if values.len() >= 5 && values[1] == 0 {
                2
            } else {
                1
            };
            let r = values.get(start).copied()?.min(255) as u8;
            let g = values.get(start + 1).copied()?.min(255) as u8;
            let b = values.get(start + 2).copied()?.min(255) as u8;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

fn cell_width(cell: &Cell) -> u16 {
    if cell.is_wide {
        2
    } else {
        u16::from(!(cell.is_wide_continuation || cell.contents.is_empty()))
    }
}

fn set_cell_width(row: &mut [Cell], col: usize, width: u16, attrs: Attrs, cols: usize) {
    row[col].is_wide = width > 1;
    row[col].is_wide_continuation = false;

    if col + 1 < cols && col + 1 < row.len() {
        if width > 1 {
            let hyperlink = row[col].hyperlink.clone();
            row[col + 1] = Cell {
                contents: compact_str::CompactString::new(""),
                is_wide: false,
                is_wide_continuation: true,
                attrs,
                hyperlink,
            };
        } else if row[col + 1].is_wide_continuation {
            row[col + 1] = Cell::default();
        }
    }
}

// ── Grid construction helpers ─────────────────────────────────────────────

fn blank_row(cols: u16) -> Vec<Cell> {
    vec![Cell::default(); cols as usize]
}

fn make_blank_grid(rows: u16, cols: u16, arena: RowArena) -> RowStore {
    RowStore::blank(rows, cols, arena)
}

fn resize_grid(grid: &RowStore, rows: u16, cols: u16) -> RowStore {
    let mut new = make_blank_grid(rows, cols, grid.arena.clone());
    for (r, row) in grid.iter().enumerate() {
        if r >= rows as usize {
            break;
        }
        new.wraps[r] = grid.wrap(r).unwrap_or_default();
        for (c, cell) in row.iter().enumerate() {
            if c < cols as usize {
                new[r][c] = cell.clone();
            }
        }
    }
    new
}

fn incomplete_utf8_suffix_len(bytes: &[u8]) -> usize {
    let Some(last) = bytes.last() else {
        return 0;
    };
    if last.is_ascii() {
        return 0;
    }

    let start = bytes
        .iter()
        .rposition(u8::is_ascii)
        .map_or(0, |idx| idx + 1);
    let suffix = &bytes[start..];
    match std::str::from_utf8(suffix) {
        Ok(_) => 0,
        Err(err) if err.valid_up_to() > 0 => suffix.len() - err.valid_up_to(),
        Err(_) => suffix.len(),
    }
}

#[cfg(test)]
mod tests;
