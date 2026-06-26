use super::{
    Attrs, Cell, DamageGrid, KITTY_KB_STACK_CAP, PassthroughEvent, RowWrap, ScrollOp, blank_row,
    make_blank_grid, reconstruct_csi,
};
use smallvec::SmallVec;
// ── vte::Perform implementation ────────────────────────────────────────────

impl vte::Perform for DamageGrid {
    fn print(&mut self, ch: char) {
        self.write_char_at_cursor(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // LF / VT / FF — newline. A plain cursor move dirties nothing;
            // when the newline scrolls, `scroll_up` marks the moved rows
            // itself (D16 — no spurious damage per line feed).
            0x0a..=0x0c => {
                self.clear_pending_wrap();
                self.newline_action();
            }
            // CR — carriage return.
            0x0d => {
                self.clear_pending_wrap();
                self.cursor_col = 0;
            }
            // BS — backspace.
            0x08 => {
                self.clear_pending_wrap();
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            // HT — horizontal tab (move to next tab stop, 8-col aligned).
            0x09 => {
                self.clear_pending_wrap();
                let next_tab = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next_tab.min(self.cols.saturating_sub(1));
            }
            // BEL — ignore.
            0x07 => {}
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // Collect param values (0 if absent/empty, as per VT semantics).
        let p: SmallVec<[u16; 8]> = params
            .iter()
            .map(|sub| sub.first().copied().unwrap_or(0))
            .collect();
        let p0 = p.first().copied().unwrap_or(0);
        let p1 = p.get(1).copied().unwrap_or(0);

        // Any explicit cursor positioning cancels a deferred (DECAWM) wrap.
        // SGR, mode toggles, erases, and edit ops do NOT move the cursor and
        // must preserve a pending wrap (e.g. "write last column, change color,
        // write" still wraps), so this is gated to the cursor-moving finals.
        if matches!(
            action,
            'A' | 'B' | 'C' | 'D' | 'E' | 'F' | 'G' | 'H' | 'f' | 'd' | 'r'
        ) {
            self.clear_pending_wrap();
        }

        match action {
            // Insert Characters (ICH) — insert n blank chars at cursor, shift right.
            '@' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let cols = self.cols as usize;
                let grid = self.active_grid();
                let row_cells = &mut grid[row];
                // Shift existing chars right, dropping any that fall off the end.
                let end = cols.min(row_cells.len());
                for c in (col..end.saturating_sub(n)).rev() {
                    row_cells[c + n] = row_cells[c].clone();
                }
                // Inserted cells use the DEFAULT background (ICH is not a BCE op).
                for cell in row_cells.iter_mut().take((col + n).min(end)).skip(col) {
                    *cell = Cell::default();
                }
                self.dirty
                    .mark_range(self.cursor_row, self.cursor_col, self.cols);
            }
            // Cursor Up.
            'A' => {
                let n = p0.max(1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.clamp_cursor();
            }
            // Cursor Down.
            'B' => {
                let n = p0.max(1);
                self.cursor_row =
                    Self::add_cursor_offset(self.cursor_row, n, self.rows.saturating_sub(1));
                self.clamp_cursor();
            }
            // Cursor Forward.
            'C' => {
                let n = p0.max(1);
                self.cursor_col =
                    Self::add_cursor_offset(self.cursor_col, n, self.cols.saturating_sub(1));
                self.clamp_cursor();
            }
            // Cursor Back.
            'D' => {
                let n = p0.max(1);
                self.cursor_col = self.cursor_col.saturating_sub(n);
                self.clamp_cursor();
            }
            // Cursor Next Line.
            'E' => {
                let n = p0.max(1);
                self.cursor_row =
                    Self::add_cursor_offset(self.cursor_row, n, self.rows.saturating_sub(1));
                self.cursor_col = 0;
            }
            // Cursor Previous Line.
            'F' => {
                let n = p0.max(1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.cursor_col = 0;
            }
            // Cursor Horizontal Absolute.
            'G' => {
                let col = p0.saturating_sub(1);
                self.cursor_col = col.min(self.cols.saturating_sub(1));
            }
            // Cursor Position (row, col, 1-based).
            'H' | 'f' => {
                let row = p0.saturating_sub(1);
                let col = p1.saturating_sub(1);
                self.cursor_row = row.min(self.rows.saturating_sub(1));
                self.cursor_col = col.min(self.cols.saturating_sub(1));
            }
            // Erase in Display.
            'J' => {
                self.erase_display(p0);
            }
            // Erase in Line.
            'K' => {
                self.erase_line(p0);
            }
            // Insert Lines. Inserted blanks use the DEFAULT background (not BCE).
            'L' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let bottom = self.scroll_bottom as usize;
                let cols = self.cols;
                if row <= bottom {
                    self.scroll_ops.push(ScrollOp::Down {
                        top: self.cursor_row,
                        bottom: self.scroll_bottom,
                        rows: p0.max(1),
                    });
                }
                let grid = self.active_grid();
                for _ in 0..n {
                    if bottom < grid.len() {
                        grid.remove(bottom);
                    }
                    grid.insert(row, blank_row(cols));
                }
                self.dirty.mark_all();
            }
            // Delete Lines. Inserted blanks use the DEFAULT background (not BCE).
            'M' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let bottom = self.scroll_bottom as usize;
                let cols = self.cols;
                if row <= bottom {
                    self.scroll_ops.push(ScrollOp::Up {
                        top: self.cursor_row,
                        bottom: self.scroll_bottom,
                        rows: p0.max(1),
                    });
                }
                let grid = self.active_grid();
                for _ in 0..n {
                    if row < grid.len() {
                        grid.remove(row);
                    }
                    if bottom < grid.len() + 1 {
                        grid.insert(bottom, blank_row(cols));
                    }
                }
                self.dirty.mark_all();
            }
            // Delete Characters. Tail fill uses the DEFAULT background (not BCE).
            'P' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let cols = self.cols as usize;
                let grid = self.active_grid();
                let row_cells = &mut grid[row];
                for c in col..cols.saturating_sub(n) {
                    row_cells[c] = row_cells.get(c + n).cloned().unwrap_or_default();
                }
                let tail_start = cols.saturating_sub(n);
                row_cells[tail_start..cols].fill(Cell::default());
                self.dirty
                    .mark_range(self.cursor_row, self.cursor_col, self.cols);
            }
            // Scroll Up.
            'S' => {
                let n = p0.max(1);
                self.scroll_up(n, RowWrap::Hard);
            }
            // Scroll Down. Inserted blanks use the DEFAULT background (not BCE).
            'T' => {
                let n = p0.max(1) as usize;
                let top = self.scroll_top as usize;
                let bottom = self.scroll_bottom as usize;
                let cols = self.cols;
                // Record the region shift like `S`/`L`/`M` do, so the deferred
                // scroll-region optimizer sees both scroll directions.
                self.scroll_ops.push(ScrollOp::Down {
                    top: self.scroll_top,
                    bottom: self.scroll_bottom,
                    rows: p0.max(1),
                });
                let grid = self.active_grid();
                for _ in 0..n {
                    if bottom < grid.len() {
                        grid.remove(bottom);
                    }
                    grid.insert(top, blank_row(cols));
                }
                self.dirty.mark_all();
            }
            // Erase Characters.
            'X' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let blank = self.blank_cell();
                let grid = self.active_grid();
                let end = (col + n).min(grid[row].len());
                grid[row][col..end].fill(blank);
                self.dirty
                    .mark_range(self.cursor_row, self.cursor_col, end as u16);
            }
            // Cursor Vertical Absolute.
            'd' => {
                let row = p0.saturating_sub(1);
                self.cursor_row = row.min(self.rows.saturating_sub(1));
            }
            // SGR — Select Graphic Rendition (no intermediates). The
            // `>`-intermediate form (`CSI > 4 ; n m`, xterm modifyOtherKeys)
            // is not SGR and must fall through to the passthrough arm.
            'm' if intermediates.is_empty() => {
                self.apply_sgr_params(params);
            }
            // DEC Private Mode Set.
            'h' if intermediates == b"?" => {
                for &mode in &p {
                    self.set_dec_mode(mode, true);
                }
            }
            // DEC Private Mode Reset.
            'l' if intermediates == b"?" => {
                for &mode in &p {
                    self.set_dec_mode(mode, false);
                }
            }
            // Set Scrolling Region.
            // DECSTBM: Set Top and Bottom Margins (scroll region).
            // After setting the scroll region, cursor is homed to (0, 0).
            'r' => {
                let top = p0.saturating_sub(1);
                let bottom = if p1 == 0 {
                    self.rows.saturating_sub(1)
                } else {
                    p1.saturating_sub(1).min(self.rows.saturating_sub(1))
                };
                if top < bottom {
                    self.scroll_top = top;
                    self.scroll_bottom = bottom;
                } else {
                    // Invalid region: reset to full screen.
                    self.scroll_top = 0;
                    self.scroll_bottom = self.rows.saturating_sub(1);
                }
                // DECSTBM positions the cursor at the upper-left after setting margins.
                self.cursor_row = 0;
                self.cursor_col = 0;
            }
            // Save Cursor.
            's' => {
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
            }
            // `u` splits by intermediate: bare = DECRC (restore cursor);
            // `>`/`<`/`?` = kitty keyboard protocol, tracked and forwarded.
            'u' if intermediates.is_empty() => {
                self.cursor_row = self.saved_cursor_row;
                self.cursor_col = self.saved_cursor_col;
                self.clamp_cursor();
            }
            // Kitty keyboard push (`\x1b[>{flags}u`): track depth so the
            // capsule's focus-swap restore stays balanced, and forward raw.
            'u' if intermediates == b">" => {
                let flags = u32::from(p0.max(1));
                if self.kitty_kb_stack.len() < KITTY_KB_STACK_CAP {
                    self.kitty_kb_stack.push(flags);
                }
                self.passthrough.push(PassthroughEvent::UnhandledCsi(
                    format!("\x1b[>{flags}u").into_bytes(),
                ));
            }
            // Kitty keyboard pop (`\x1b[<{n}u`): pop `n` levels (default 1).
            'u' if intermediates == b"<" => {
                let count = usize::from(p0.max(1));
                for _ in 0..count.min(self.kitty_kb_stack.len()) {
                    self.kitty_kb_stack.pop();
                }
                self.passthrough.push(PassthroughEvent::UnhandledCsi(
                    format!("\x1b[<{count}u").into_bytes(),
                ));
            }
            // Kitty keyboard query (`\x1b[?u`). The agent is asking the
            // capsule's emulator which kitty-keyboard flags it supports.
            // Answer 0 (no enhancement) so the agent uses the legacy key
            // encoding the capsule's input layer handles — do NOT forward to
            // the host, whose richer kitty support the grid does not emulate.
            'u' if intermediates == b"?" => {
                self.passthrough
                    .push(PassthroughEvent::Reply(b"\x1b[?0u".to_vec()));
            }
            // Primary / secondary Device Attributes query (DA1 `\x1b[c`,
            // DA2 `\x1b[>c`). Answer as the emulator itself with a conservative
            // VT220 identity and no optional features, so the agent does not
            // light up capabilities (sixel, etc.) the grid cannot render.
            'c' => match intermediates {
                b"" => self
                    .passthrough
                    .push(PassthroughEvent::Reply(b"\x1b[?62c".to_vec())),
                b">" => self
                    .passthrough
                    .push(PassthroughEvent::Reply(b"\x1b[>0;0;0c".to_vec())),
                _ => {}
            },
            // Device Status Report. `5n` → terminal-OK; `6n` → cursor position.
            // The cursor reply MUST use the grid's own cursor, not the host's:
            // forwarding `6n` to the host returned the outer terminal's cursor,
            // which the agent then used for layout math against the wrong
            // origin. DEC form (`?6n`, DECXCPR) carries a trailing page param.
            'n' if intermediates == b"" || intermediates == b"?" => {
                let reply = match p0 {
                    5 => b"\x1b[0n".to_vec(),
                    6 => {
                        let row = self.cursor_row.saturating_add(1);
                        // Clamp the DECAWM phantom column (== cols while a
                        // wrap is pending) to the last real column — agents
                        // do layout math with this reply, and `cols + 1` is
                        // not an addressable position (D13).
                        let col = self
                            .cursor_col
                            .min(self.cols.saturating_sub(1))
                            .saturating_add(1);
                        if intermediates == b"?" {
                            format!("\x1b[?{row};{col};1R").into_bytes()
                        } else {
                            format!("\x1b[{row};{col}R").into_bytes()
                        }
                    }
                    _ => Vec::new(),
                };
                if !reply.is_empty() {
                    self.passthrough.push(PassthroughEvent::Reply(reply));
                }
            }
            // DECRQM — request mode (`\x1b[?{mode}$p` / `\x1b[{mode}$p`). Answer
            // 0 ("mode not recognized") for every mode so the agent renders in
            // the capsule's baseline: critically this declines mode 2027
            // (grapheme-cluster width), whose enable would make the agent
            // advance columns by grapheme while the grid advances by
            // `unicode_width`, desyncing every wide/combining glyph. `$` marks
            // DECRQM; `!p` (DECSTR soft reset) has no `$` and falls through.
            'p' if intermediates.contains(&b'$') => {
                let dec = intermediates.contains(&b'?');
                let status = self.profile.decrqm_status(p0);
                let reply = if dec {
                    format!("\x1b[?{p0};{status}$y")
                } else {
                    format!("\x1b[{p0};{status}$y")
                };
                self.passthrough
                    .push(PassthroughEvent::Reply(reply.into_bytes()));
            }
            // DECSCUSR — set cursor style (`CSI {n} SP q`). Tracked per pane
            // and reconciled to the outer terminal by the capsule encoder per
            // frame; forwarding it raw leaked one pane's cursor shape into
            // every other pane (D5).
            'q' if intermediates == b" " => {
                self.cursor_style = p0;
            }
            // DECSTR — soft reset (`CSI ! p`). Handled in-grid and never
            // forwarded: on the outer terminal it would soft-reset the host.
            'p' if intermediates == b"!" => {
                self.current_attrs = Attrs::default();
                self.scroll_top = 0;
                self.scroll_bottom = self.rows.saturating_sub(1);
                self.pending_wrap = false;
                self.hide_cursor = false;
                self.application_cursor = false;
                self.bracketed_paste = false;
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
            }
            // xterm modifyOtherKeys (`CSI > 4 ; n m`) — on the forward
            // allowlist: the capsule's input layer relies on the outer
            // terminal honoring it, and the session tracks the level so
            // alternate-screen exit can reset it.
            'm' if intermediates == b">" && p0 == 4 => {
                let bytes = reconstruct_csi(params, intermediates, action as u8);
                if !bytes.is_empty() {
                    self.passthrough.push(PassthroughEvent::UnhandledCsi(bytes));
                }
            }
            // XTVERSION query (`\x1b[>q`). Suppress: the grid has no meaningful
            // terminal-version identity to advertise, and forwarding it let the
            // host answer with a real terminal's name + version, which steered
            // the agent into host-specific rendering paths. No reply needed —
            // agents fall back when the query goes unanswered.
            'q' if intermediates == b">" => {}
            _ => {
                // Default-deny (§3.6 of the capsule rendering plan): an
                // unhandled CSI never reaches the client. The bytes are
                // carried out as DroppedCsi so the capsule can `cdebug!`-log
                // the exact sequence; allowlist additions (kitty keyboard
                // push/pop above, modifyOtherKeys) require a documented
                // sequence + reason in multiplexer-design-rules.
                let bytes = reconstruct_csi(params, intermediates, action as u8);
                if !bytes.is_empty() {
                    self.passthrough.push(PassthroughEvent::DroppedCsi(bytes));
                }
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            // ESC M — Reverse Index (RI): move cursor up one row.
            // If cursor is at the top margin, scroll content DOWN one row instead.
            b'M' => {
                self.clear_pending_wrap();
                if self.cursor_row == self.scroll_top {
                    // Scroll down: insert blank row at scroll_top, remove from
                    // scroll_bottom. The new blank uses the DEFAULT background —
                    // RI scroll is not a back-colour-erase op.
                    let top = self.scroll_top as usize;
                    let bottom = self.scroll_bottom as usize;
                    let cols = self.cols;
                    self.scroll_ops.push(ScrollOp::Down {
                        top: self.scroll_top,
                        bottom: self.scroll_bottom,
                        rows: 1,
                    });
                    let grid = self.active_grid();
                    if bottom < grid.len() {
                        grid.remove(bottom);
                    }
                    grid.insert(top, blank_row(cols));
                    self.dirty.mark_all();
                } else {
                    self.cursor_row = self.cursor_row.saturating_sub(1);
                }
            }
            // DECSC — save cursor.
            b'7' => {
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
            }
            // DECRC — restore cursor.
            b'8' => {
                self.clear_pending_wrap();
                self.cursor_row = self.saved_cursor_row;
                self.cursor_col = self.saved_cursor_col;
                self.clamp_cursor();
            }
            // RIS — full reset.
            b'c' => {
                let blank = make_blank_grid(self.rows, self.cols, self.primary.arena.clone());
                self.primary = blank.clone();
                self.alternate = blank;
                self.alt_screen = false;
                self.pending_wrap = false;
                self.cursor_row = 0;
                self.cursor_col = 0;
                self.current_attrs = Attrs::default();
                self.active_hyperlink = None;
                self.scroll_top = 0;
                self.scroll_bottom = self.rows.saturating_sub(1);
                self.reset_modes();
                self.dirty.mark_all();
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.handle_osc(params, bell_terminated);
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}
