use super::{
    Rect, SplitDirectionGeometry, available_content_rows, content_rect, local_mouse_position,
    split_spawn_inner_size,
};

#[test]
fn shrink_inside_normal_rect() {
    let r = Rect::new(5, 10, 20, 30);
    let s = r.shrink(1);
    assert_eq!((s.row, s.col, s.rows, s.cols), (6, 11, 18, 28));
}

#[test]
fn shrink_clamps_to_zero_when_too_narrow() {
    let r = Rect::new(5, 10, 1, 1);
    let s = r.shrink(1);
    // Width and height drop to zero; row/col stay put so callers
    // get a valid (if empty) rectangle.
    assert_eq!((s.rows, s.cols), (0, 0));
    assert_eq!((s.row, s.col), (5, 10));
}

#[test]
fn shrink_by_zero_is_noop() {
    let r = Rect::new(2, 3, 7, 11);
    let s = r.shrink(0);
    assert_eq!((s.row, s.col, s.rows, s.cols), (2, 3, 7, 11));
}

#[test]
fn split_spawn_inner_size_uses_future_half_pane() {
    let rect = Rect::new(0, 0, 24, 80);
    assert_eq!(
        split_spawn_inner_size(SplitDirectionGeometry::LeftRight, rect),
        (22, 38)
    );
    assert_eq!(
        split_spawn_inner_size(SplitDirectionGeometry::TopBottom, rect),
        (10, 78)
    );
}

#[test]
fn split_spawn_inner_size_saturates_tiny_panes() {
    let rect = Rect::new(0, 0, 1, 3);
    assert_eq!(
        split_spawn_inner_size(SplitDirectionGeometry::LeftRight, rect),
        (0, 0)
    );
    assert_eq!(
        split_spawn_inner_size(SplitDirectionGeometry::TopBottom, rect),
        (0, 1)
    );
}

#[test]
fn local_mouse_position_requires_inner_rect_hit() {
    let inner = Rect::new(2, 4, 10, 20);
    assert_eq!(local_mouse_position(inner, 2, 4), Some((0, 0)));
    assert_eq!(local_mouse_position(inner, 11, 23), Some((9, 19)));
    assert_eq!(local_mouse_position(inner, 1, 4), None);
    assert_eq!(local_mouse_position(inner, 12, 4), None);
    assert_eq!(local_mouse_position(inner, 2, 3), None);
    assert_eq!(local_mouse_position(inner, 2, 24), None);
}

#[test]
fn content_area_reserves_chrome_rows() {
    assert_eq!(available_content_rows(24), 19);
    assert_eq!(content_rect(19, 80), Rect::new(2, 0, 19, 80));
}
