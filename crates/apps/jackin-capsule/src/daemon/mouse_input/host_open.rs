// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::attach::ServerFrame;

use crate::tui::pane_snapshot::RowSnapshot;

use super::super::Multiplexer;
use super::PanePress;
use crate::tui::selection::word_bounds_in_row;

impl Multiplexer {
    pub(crate) fn open_visible_url_at(&mut self, row: u16, col: u16) -> bool {
        if !host_url_opening_allowed() {
            return false;
        }
        match self.resolve_host_open_target_at_mouse_cell(row, col) {
            Some(HostOpenTarget::Allowed(url)) => self.send_host_open_url(url),
            Some(HostOpenTarget::Rejected) => {
                self.reject_host_open_url();
                true
            }
            None => false,
        }
    }

    pub(crate) fn open_visible_url_under_cursor(&mut self) -> bool {
        let Some(session_id) = self.active_focused_id() else {
            return false;
        };
        let Some(inner) = self.active_focused_inner_rect() else {
            return false;
        };
        let Some(session) = self.session_supervisor.sessions.get(session_id) else {
            return false;
        };
        if session.scrollback_offset() != 0 {
            return false;
        }
        let (cursor_row, cursor_col) = session.shadow_grid.cursor_position();
        let content_row = session
            .shadow_grid
            .scrollback_len()
            .saturating_add(usize::from(cursor_row));
        let rows = session
            .render_content_snapshot_range(inner.cols, content_row..content_row.saturating_add(1));
        if rows.is_empty() {
            return false;
        }
        let Some(target) = self.resolve_host_open_target_at_content_cell(
            session_id,
            &rows,
            content_row,
            content_row,
            cursor_col,
        ) else {
            return false;
        };
        match target {
            HostOpenTarget::Allowed(url) => self.send_host_open_url(url),
            HostOpenTarget::Rejected => {
                self.reject_host_open_url();
                true
            }
        }
    }

    fn resolve_host_open_target_at_mouse_cell(&self, row: u16, col: u16) -> Option<HostOpenTarget> {
        let candidate = self.detect_selection_start(row, col)?;
        let session = self.session_supervisor.sessions.get(candidate.session_id)?;
        let content_row = candidate.anchor_row;
        let range_start = content_row;
        let rows = session.render_content_snapshot_range(
            candidate.inner.cols,
            content_row..content_row.saturating_add(1),
        );
        self.resolve_host_open_target_at_content_cell(
            candidate.session_id,
            &rows,
            range_start,
            content_row,
            candidate.anchor_col,
        )
    }

    fn resolve_host_open_target_at_content_cell(
        &self,
        session_id: u64,
        rows: &[RowSnapshot],
        rows_base: usize,
        row_idx: usize,
        anchor_col: u16,
    ) -> Option<HostOpenTarget> {
        let session = self.session_supervisor.sessions.get(session_id)?;

        if let Some(osc8_target) = session.hyperlink_target_at_content_row(row_idx, anchor_col) {
            if crate::tui::url_text::is_host_open_url(osc8_target) {
                return Some(HostOpenTarget::Allowed(osc8_target.to_owned()));
            }
            if !crate::tui::url_text::has_url_scheme(osc8_target) {
                return None;
            }
            return Some(HostOpenTarget::Rejected);
        }

        let local_row = row_idx.saturating_sub(rows_base);
        let row = rows.get(local_row)?;
        let (start_col, end_col) = word_bounds_in_row(row, anchor_col)?;
        let url = row.text_range(start_col, end_col);
        if !crate::tui::url_text::is_host_open_url(&url) {
            if !crate::tui::url_text::has_url_scheme(&url) {
                return None;
            }
            return Some(HostOpenTarget::Rejected);
        }
        Some(HostOpenTarget::Allowed(url))
    }

    pub(crate) fn resolve_http_url_at_mouse_cell(&self, row: u16, col: u16) -> Option<String> {
        match self.resolve_host_open_target_at_mouse_cell(row, col) {
            Some(HostOpenTarget::Allowed(url)) => Some(url),
            Some(HostOpenTarget::Rejected) | None => None,
        }
    }

    fn send_host_open_url(&mut self, url: String) -> bool {
        jackin_telemetry::ui::record_action(
            jackin_telemetry::schema::enums::UiActionName::LinkOpen,
            jackin_telemetry::schema::enums::ScreenId::Capsule,
            None,
        );
        self.send_protocol_frame(ServerFrame::HostOpenUrl(url));
        true
    }

    fn reject_host_open_url(&mut self) {
        self.set_clipboard_image_notice("Host link rejected: unsupported URL scheme".to_owned());
    }
}

const DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

enum HostOpenTarget {
    Allowed(String),
    Rejected,
}

pub(crate) fn host_url_opening_allowed() -> bool {
    static ALLOWED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ALLOWED.get_or_init(|| {
        let value = std::env::var(jackin_core::JACKIN_OPEN_LINKS_ENV_NAME).ok();
        host_url_opening_allowed_for(value.as_deref())
    })
}

pub(crate) fn host_url_opening_allowed_for(value: Option<&str>) -> bool {
    jackin_core::open_links_allowed(value)
}

pub(crate) fn is_double_click(previous: &PanePress, press: &PanePress) -> bool {
    previous.session_id == press.session_id
        && previous.content_row == press.content_row
        && previous.col == press.col
        && press.at.duration_since(previous.at) <= DOUBLE_CLICK_WINDOW
}
