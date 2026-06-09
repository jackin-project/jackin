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

#[test]
fn fuzz_split_utf8_c1_control_matches_one_shot() {
    assert_one_shot_matches_byte_split(&[0xc2, 0x8a, 0x1b, 0x1f]);
}

#[test]
fn fuzz_split_adjacent_utf8_leads_matches_one_shot() {
    assert_one_shot_matches_byte_split(&[b'k', 0xd6, 0xd6]);
}

#[test]
fn fuzz_valid_utf8_prefix_before_incomplete_suffix_matches_one_shot() {
    assert_one_shot_matches_byte_split(&[0xd6, 0x8c, 0xf0, 0xb9]);
}

#[test]
fn split_utf8_printable_matches_one_shot() {
    assert_one_shot_matches_byte_split("a¢b".as_bytes());
}

#[test]
fn split_incomplete_utf8_then_escape_matches_one_shot() {
    assert_one_shot_matches_byte_split(&[0xc2, 0x1b, b'[', b'2', b'C']);
}

fn assert_one_shot_matches_byte_split(bytes: &[u8]) {
    let mut one_shot = DamageGrid::new(24, 80, 1_000);
    let mut split = DamageGrid::new(24, 80, 1_000);

    one_shot.process(bytes);
    for byte in bytes {
        split.process(std::slice::from_ref(byte));
    }

    assert_eq!(one_shot.cursor_position(), split.cursor_position());
    assert_eq!(one_shot.alternate_screen(), split.alternate_screen());

    let (rows, cols) = one_shot.size();
    for row in 0..rows {
        for col in 0..cols {
            assert_eq!(one_shot.cell(row, col), split.cell(row, col));
        }
    }
}
