// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub const PAGE_STEP: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffScroll {
    key_offset: usize,
    render_offset: usize,
}

impl DiffScroll {
    #[must_use]
    pub fn new() -> Self {
        Self {
            key_offset: 0,
            render_offset: 0,
        }
    }

    #[must_use]
    pub fn offset_for_render(&self, line_count: usize) -> usize {
        self.render_offset.min(line_count.saturating_sub(1))
    }

    pub fn record_rendered(&mut self, offset: usize) {
        self.render_offset = offset;
    }

    pub fn line_up(&mut self) {
        self.key_offset = self.key_offset.saturating_sub(1);
        self.render_offset = self.key_offset;
    }

    pub fn line_down(&mut self, line_count: usize) {
        self.key_offset = self
            .key_offset
            .saturating_add(1)
            .min(line_count.saturating_sub(1));
        self.render_offset = self.key_offset;
    }

    pub fn page_up(&mut self) {
        self.key_offset = self.key_offset.saturating_sub(PAGE_STEP);
        self.render_offset = self.key_offset;
    }

    pub fn page_down(&mut self, line_count: usize) {
        self.key_offset = self
            .key_offset
            .saturating_add(PAGE_STEP)
            .min(line_count.saturating_sub(1));
        self.render_offset = self.key_offset;
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests;
