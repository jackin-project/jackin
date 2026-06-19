use super::{Direction, PaneTree, Rect, SplitOrient, SplitPosition};

#[test]
fn border_at_horizontal_split_returns_path_and_orient() {
    let mut tree = PaneTree::Leaf(1);
    tree.split_h(1, 2, SplitPosition::After);
    let rect = Rect::new(0, 0, 10, 20);
    // Boundary cols sit either side of col=10 (left=9, right=10).
    let hit = tree.border_at(rect, 5, 10).expect("boundary hit");
    let (path, orient, _) = hit;
    assert!(path.is_empty(), "boundary at the root split");
    assert_eq!(orient, SplitOrient::Horizontal);
}

#[test]
fn border_at_vertical_split_returns_correct_orient() {
    let mut tree = PaneTree::Leaf(1);
    tree.split_v(1, 2, SplitPosition::After);
    let rect = Rect::new(0, 0, 10, 20);
    // Boundary row at row=5.
    let hit = tree.border_at(rect, 5, 4).expect("boundary hit");
    assert_eq!(hit.1, SplitOrient::Vertical);
}

#[test]
fn border_at_returns_none_for_pane_interior() {
    let mut tree = PaneTree::Leaf(1);
    tree.split_h(1, 2, SplitPosition::After);
    let rect = Rect::new(0, 0, 10, 20);
    // Click at col 3 is inside the left pane, not on the
    // boundary.
    assert!(tree.border_at(rect, 5, 3).is_none());
}

#[test]
fn set_ratio_at_clamps_to_safe_range() {
    let mut tree = PaneTree::Leaf(1);
    tree.split_h(1, 2, SplitPosition::After);
    assert!(tree.set_ratio_at(&[], 0.001));
    if let PaneTree::HSplit { ratio, .. } = tree {
        assert!(ratio >= 0.05);
    } else {
        panic!("expected HSplit");
    }
}

#[test]
fn set_ratio_at_rejects_nan_and_infinity() {
    let mut tree = PaneTree::Leaf(1);
    tree.split_h(1, 2, SplitPosition::After);
    // `is_finite()` covers NaN AND +/-infinity. Both would survive
    // `f32::clamp` poorly and pollute intermediate arithmetic.
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!tree.set_ratio_at(&[], bad), "{bad} must be rejected");
        if let PaneTree::HSplit { ratio, .. } = tree {
            assert!(ratio.is_finite());
        } else {
            panic!("expected HSplit");
        }
    }
}

#[test]
fn resize_rejects_non_finite_delta() {
    let mut tree = PaneTree::Leaf(1);
    tree.split_h(1, 2, SplitPosition::After);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!tree.resize(1, Direction::Right, bad));
        if let PaneTree::HSplit { ratio, .. } = tree {
            assert!(ratio.is_finite());
        } else {
            panic!("expected HSplit");
        }
    }
}

#[test]
fn remove_3_deep_collapses_correctly() {
    // Build: HSplit{ Leaf(1), VSplit{ HSplit{ Leaf(2), Leaf(3) }, Leaf(4) } }
    let mut tree = PaneTree::Leaf(1);
    assert!(tree.split_h(1, 2, SplitPosition::After));
    assert!(tree.split_v(2, 4, SplitPosition::After));
    assert!(tree.split_h(2, 3, SplitPosition::After));
    // Removing leaf 3 should collapse its parent HSplit to Leaf(2).
    assert!(tree.remove(3));
    assert!(tree.all_ids().contains(&1));
    assert!(tree.all_ids().contains(&2));
    assert!(tree.all_ids().contains(&4));
    assert!(!tree.all_ids().contains(&3));
    // Removing leaf 4 collapses VSplit to its remaining child.
    assert!(tree.remove(4));
    assert!(tree.all_ids().contains(&1));
    assert!(tree.all_ids().contains(&2));
    assert!(!tree.all_ids().contains(&4));
    // Removing leaf 2 collapses root HSplit to Leaf(1).
    assert!(tree.remove(2));
    assert_eq!(tree.all_ids(), vec![1]);
}

// Direction is only referenced via the test alias to keep this
// module's `use` block tidy; no runtime assertion needs it.
fn _direction_referenced(_: Direction) {}
