use super::*;

#[test]
fn click_outside_returns_dismiss() {
    let rect = Rect::new(10, 5, 20, 10);
    assert_eq!(classify_click(rect, 5, 5), ModalClickResult::OutsideDismiss);
    assert_eq!(
        classify_click(rect, 35, 5),
        ModalClickResult::OutsideDismiss
    );
    assert_eq!(
        classify_click(rect, 15, 20),
        ModalClickResult::OutsideDismiss
    );
}

#[test]
fn click_inside_returns_hit() {
    let rect = Rect::new(10, 5, 20, 10);
    assert_eq!(classify_click(rect, 15, 8), ModalClickResult::InsideHit);
    assert_eq!(classify_click(rect, 10, 5), ModalClickResult::InsideHit);
    assert_eq!(classify_click(rect, 29, 14), ModalClickResult::InsideHit);
}

#[test]
fn modal_stack_opens_root_without_parents() {
    let mut stack = ModalStack::new();

    stack.open("root");

    assert_eq!(stack.current(), Some(&"root"));
    assert!(stack.parents().is_empty());
    assert_eq!(stack.depth(), 1);
}

#[test]
fn modal_stack_pushes_current_when_opening_sub_modal() {
    let mut stack = ModalStack::from_current("root");

    stack.open_sub("child");

    assert_eq!(stack.current(), Some(&"child"));
    assert_eq!(stack.parents(), &["root"]);
    assert_eq!(stack.depth(), 2);
}

#[test]
fn modal_stack_pop_restores_one_parent_at_a_time() {
    let mut stack = ModalStack::from_current("root");
    stack.open_sub("child");
    stack.open_sub("grandchild");

    stack.pop();

    assert_eq!(stack.current(), Some(&"child"));
    assert_eq!(stack.parents(), &["root"]);

    stack.pop();

    assert_eq!(stack.current(), Some(&"root"));
    assert!(stack.parents().is_empty());

    stack.pop();

    assert_eq!(stack.current(), None);
    assert!(stack.parents().is_empty());
}

#[test]
fn modal_stack_clear_chain_drops_current_and_parents() {
    let mut stack = ModalStack::from_current("root");
    stack.open_sub("child");
    stack.open_sub("grandchild");

    stack.clear_chain();

    assert_eq!(stack.current(), None);
    assert!(stack.parents().is_empty());
    assert_eq!(stack.depth(), 0);
}

#[test]
fn modal_stack_open_replaces_existing_chain() {
    let mut stack = ModalStack::from_current("root");
    stack.open_sub("child");

    stack.open("replacement");

    assert_eq!(stack.current(), Some(&"replacement"));
    assert!(stack.parents().is_empty());
}
