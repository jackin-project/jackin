use super::*;

#[test]
fn fixed_width_uses_stable_reference_columns() {
    assert_eq!(
        modal_rect(
            Rect::new(0, 0, 200, 50),
            ModalRectSpec::Fixed {
                width_pct: 60,
                height: 5,
            },
        ),
        Rect::new(52, 22, 96, 5)
    );
}

#[test]
fn fixed_width_shrinks_with_four_column_margin() {
    assert_eq!(
        modal_rect(
            Rect::new(0, 0, 80, 24),
            ModalRectSpec::Fixed {
                width_pct: 60,
                height: 5,
            },
        ),
        Rect::new(2, 9, 76, 5)
    );
}

#[test]
fn exact_width_preserves_capsule_small_terminal_behavior() {
    assert_eq!(
        modal_rect(
            Rect::new(0, 1, 20, 5),
            ModalRectSpec::Exact {
                width: 64,
                height: 5,
            },
        ),
        Rect::new(0, 1, 64, 5)
    );
}

#[test]
fn top_aligned_keeps_capsule_usage_dialog_below_status_bar() {
    assert_eq!(
        modal_rect(
            Rect::new(0, 2, 80, 20),
            ModalRectSpec::TopAligned {
                width: 64,
                height: 8,
            },
        ),
        Rect::new(8, 2, 64, 8)
    );
}

#[test]
fn top_aligned_capped_width_keeps_usage_dialog_at_top() {
    assert_eq!(
        modal_rect(
            Rect::new(0, 2, 100, 20),
            ModalRectSpec::TopAlignedMaxWidthMin {
                max_width: 86,
                min_width: 50,
                side_margin: 4,
                height: 8,
            },
        ),
        Rect::new(7, 2, 86, 8)
    );
}
