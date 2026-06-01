pub const LIST_HEADER_HEIGHT: u16 = 2;
pub const LIST_FOOTER_HEIGHT: u16 = 2;
pub const SCREEN_HEADER_HEIGHT: u16 = 3;
pub const TAB_STRIP_HEIGHT: u16 = 2;

/// Compute the seam column for a percentage split and total terminal width.
#[must_use]
pub const fn split_seam_column(pct: u16, width: u16) -> u16 {
    width.saturating_mul(pct) / 100
}

/// Return `(left_x, left_width, right_x, right_width)` using Ratatui's
/// percentage layout arithmetic.
#[must_use]
pub fn horizontal_split_pane_dims(pct: u16, total_width: u16) -> (u16, u16, u16, u16) {
    let right_pct = 100u16.saturating_sub(pct);
    let cols = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(pct),
            ratatui::layout::Constraint::Percentage(right_pct),
        ])
        .split(ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: total_width,
            height: 1,
        });
    (cols[0].x, cols[0].width, cols[1].x, cols[1].width)
}

/// Derive a new split percentage from a drag anchor and current mouse column.
#[must_use]
pub fn split_pct_from_drag(anchor_pct: u16, anchor_x: u16, mouse_col: u16, width: u16) -> u16 {
    let delta_cols = i32::from(mouse_col) - i32::from(anchor_x);
    let delta_pct = delta_cols * 100 / i32::from(width.max(1));
    let candidate = i32::from(anchor_pct) + delta_pct;
    let bounded = candidate.clamp(0, 100);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    {
        bounded as u16
    }
}

/// Like a centered percent-width rect, but takes a fixed row height.
#[must_use]
pub fn centered_rect_fixed(
    outer: ratatui::layout::Rect,
    pct_w: u16,
    rows: u16,
) -> ratatui::layout::Rect {
    let w = outer.width * pct_w / 100;
    let h = rows.min(outer.height);
    ratatui::layout::Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::{horizontal_split_pane_dims, split_pct_from_drag, split_seam_column};

    #[test]
    fn split_seam_column_uses_saturating_percent_math() {
        assert_eq!(split_seam_column(30, 100), 30);
        assert_eq!(split_seam_column(30, 0), 0);
    }

    #[test]
    fn horizontal_split_pane_dims_match_ratatui_percentage_layout() {
        assert_eq!(horizontal_split_pane_dims(30, 100), (0, 30, 30, 70));
        assert_eq!(horizontal_split_pane_dims(33, 101), (0, 33, 33, 68));
    }

    #[test]
    fn split_pct_from_drag_handles_signed_delta_and_bounds() {
        assert_eq!(split_pct_from_drag(30, 30, 50, 100), 50);
        assert_eq!(split_pct_from_drag(30, 30, 0, 100), 0);
        assert_eq!(split_pct_from_drag(80, 80, 200, 100), 100);
    }
}
