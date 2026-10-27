// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io;

use ratatui::{
    backend::{Backend, ClearType},
    buffer::Cell,
    layout::{Position, Size},
    style::{Color, Modifier},
};

#[derive(Debug)]
pub struct SocketBackend {
    size: (u16, u16),
    output: Vec<u8>,
    current_style: CellStyle,
    current_metadata: SgrMetadata,
    hyperlink_regions: Vec<(ratatui::layout::Rect, String)>,
    sgr_regions: Vec<(ratatui::layout::Rect, SgrMetadata)>,
    suppress_clear_escapes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CellStyle {
    fg: Color,
    bg: Color,
    modifiers: Modifier,
}

impl CellStyle {
    fn from_cell(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            modifiers: cell.modifier,
        }
    }
}

impl SocketBackend {
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            size: (cols, rows),
            output: Vec::with_capacity(65536),
            current_style: CellStyle::default(),
            current_metadata: SgrMetadata::default(),
            hyperlink_regions: Vec::new(),
            sgr_regions: Vec::new(),
            suppress_clear_escapes: false,
        }
    }

    pub fn set_hyperlink_regions(&mut self, regions: Vec<(ratatui::layout::Rect, String)>) {
        self.hyperlink_regions = regions;
    }

    pub(crate) fn set_sgr_regions(&mut self, regions: Vec<(ratatui::layout::Rect, SgrMetadata)>) {
        self.sgr_regions = regions;
    }

    pub fn begin_clear_suppression(&mut self) {
        self.suppress_clear_escapes = true;
    }

    pub fn end_clear_suppression(&mut self) {
        self.suppress_clear_escapes = false;
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size = (cols, rows);
        self.current_style = CellStyle::default();
        self.current_metadata = SgrMetadata::default();
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    pub fn drain_output_into(&mut self, target: &mut Vec<u8>) {
        target.extend_from_slice(&self.output);
        self.output.clear();
    }

    fn apply_style(&mut self, style: CellStyle, metadata: SgrMetadata) {
        if style == self.current_style && metadata == self.current_metadata {
            return;
        }
        self.output.extend_from_slice(b"\x1b[0m");

        if style.modifiers.contains(Modifier::BOLD) {
            self.output.extend_from_slice(b"\x1b[1m");
        }
        if style.modifiers.contains(Modifier::DIM) {
            self.output.extend_from_slice(b"\x1b[2m");
        }
        if style.modifiers.contains(Modifier::ITALIC) {
            self.output.extend_from_slice(b"\x1b[3m");
        }
        if style.modifiers.contains(Modifier::UNDERLINED) {
            self.output.extend_from_slice(b"\x1b[4m");
        }
        if style.modifiers.contains(Modifier::SLOW_BLINK) {
            self.output.extend_from_slice(b"\x1b[5m");
        }
        if style.modifiers.contains(Modifier::RAPID_BLINK) {
            self.output.extend_from_slice(b"\x1b[6m");
        }
        if style.modifiers.contains(Modifier::REVERSED) {
            self.output.extend_from_slice(b"\x1b[7m");
        }
        if style.modifiers.contains(Modifier::HIDDEN) {
            self.output.extend_from_slice(b"\x1b[8m");
        }
        if style.modifiers.contains(Modifier::CROSSED_OUT) {
            self.output.extend_from_slice(b"\x1b[9m");
        }

        write_color_sgr(&mut self.output, style.fg, false);

        write_color_sgr(&mut self.output, style.bg, true);
        write_sgr_metadata(&mut self.output, metadata);

        self.current_style = style;
        self.current_metadata = metadata;
    }
}

impl Backend for SocketBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut cursor_row: Option<u16> = None;
        let mut cursor_col: Option<u16> = None;
        let regions = std::mem::take(&mut self.hyperlink_regions);
        let sgr_regions = std::mem::take(&mut self.sgr_regions);
        let mut open_link: Option<usize> = None;

        for (x, y, cell) in content {
            let row = y + 1;
            let col = x + 1;

            let desired_link = regions
                .iter()
                .position(|(rect, _)| rect.contains(Position { x, y }));
            if desired_link != open_link {
                if open_link.is_some() {
                    crate::tui::ansi::emit_osc8_close(&mut self.output);
                }
                if let Some(idx) = desired_link {
                    crate::tui::ansi::emit_osc8_open(&mut self.output, &regions[idx].1);
                }
                open_link = desired_link;
            }
            let metadata = sgr_regions
                .iter()
                .find_map(|(rect, metadata)| rect.contains(Position { x, y }).then_some(*metadata))
                .unwrap_or_default();

            let already_positioned = cursor_row == Some(row) && cursor_col == Some(col);
            if !already_positioned {
                self.output.extend_from_slice(b"\x1b[");
                push_number(&mut self.output, u32::from(row));
                self.output.push(b';');
                push_number(&mut self.output, u32::from(col));
                self.output.push(b'H');
                cursor_row = Some(row);
            }

            self.apply_style(CellStyle::from_cell(cell), metadata);
            let sym = cell.symbol();
            self.output.extend_from_slice(sym.as_bytes());

            let is_single_ascii_printable =
                sym.len() == 1 && matches!(sym.as_bytes()[0], 0x20..=0x7e);
            if is_single_ascii_printable {
                cursor_col = Some(col + 1);
            } else {
                cursor_col = None;
            }
        }
        if open_link.is_some() {
            crate::tui::ansi::emit_osc8_close(&mut self.output);
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.output.extend_from_slice(b"\x1b[?25l");
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.output.extend_from_slice(b"\x1b[?25h");
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(Position { x: 0, y: 0 })
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        let pos = position.into();
        let row = pos.y + 1;
        let col = pos.x + 1;
        self.output.extend_from_slice(b"\x1b[");
        push_number(&mut self.output, u32::from(row));
        self.output.push(b';');
        push_number(&mut self.output, u32::from(col));
        self.output.push(b'H');
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.current_style = CellStyle::default();
        self.current_metadata = SgrMetadata::default();
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let seq: &[u8] = match clear_type {
            ClearType::All => {
                self.current_style = CellStyle::default();
                self.current_metadata = SgrMetadata::default();
                if self.suppress_clear_escapes {
                    return Ok(());
                }
                b"\x1b[0m\x1b[2J\x1b[H"
            }
            ClearType::AfterCursor => b"\x1b[0J",
            ClearType::BeforeCursor => b"\x1b[1J",
            ClearType::CurrentLine => b"\x1b[2K",
            ClearType::UntilNewLine => b"\x1b[0K",
        };
        self.output.extend_from_slice(seq);
        Ok(())
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(Size {
            width: self.size.0,
            height: self.size.1,
        })
    }

    fn window_size(&mut self) -> Result<ratatui::backend::WindowSize, Self::Error> {
        Ok(ratatui::backend::WindowSize {
            columns_rows: Size {
                width: self.size.0,
                height: self.size.1,
            },
            pixels: Size {
                width: 0,
                height: 0,
            },
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

mod sgr;
pub(crate) use sgr::{SgrMetadata, push_number, term_color, write_color_sgr, write_sgr_metadata};
