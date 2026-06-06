use crate::DamageGrid;

#[test]
fn fuzz_csi_cursor_down_count_does_not_overflow() {
    let mut grid = DamageGrid::new(24, 80, 1_000);
    let bytes = [
        0, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 56, 66, 56, 0, 0, 26,
        27, 255, 253, 91, 253, 52, 56, 56, 56, 253, 52, 56, 56, 56, 66, 56, 0, 0, 26, 27, 152, 152,
        10, 3, 3,
    ];

    grid.process(&bytes);

    let (row, col) = grid.cursor_position();
    assert_eq!(row, 23);
    assert!(col < 80);
}
