use crate::statusbar::STATUS_BAR_ROWS;

pub const DEFAULT_ROWS: u16 = 24;
pub const DEFAULT_COLS: u16 = 80;

const MIN_ROWS: u16 = STATUS_BAR_ROWS + 3;
const MIN_COLS: u16 = 3;

pub fn normalize_size(rows: u16, cols: u16) -> (u16, u16) {
    let rows = if rows == 0 { DEFAULT_ROWS } else { rows }.max(MIN_ROWS);
    let cols = if cols == 0 { DEFAULT_COLS } else { cols }.max(MIN_COLS);
    (rows, cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_size_replaces_zero_dimensions_with_defaults() {
        assert_eq!(normalize_size(0, 0), (DEFAULT_ROWS, DEFAULT_COLS));
    }

    #[test]
    fn normalize_size_clamps_tiny_dimensions_to_pty_safe_floor() {
        assert_eq!(normalize_size(1, 1), (MIN_ROWS, MIN_COLS));
    }
}
