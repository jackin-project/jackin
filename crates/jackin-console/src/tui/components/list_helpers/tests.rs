//! Tests for `list_helpers`.
use super::{
    clamp_selection, first_selection, list_state_for_count, matches_filter, selected_choice,
};

#[test]
fn first_selection_is_zero_only_when_nonempty() {
    assert_eq!(first_selection(0), None);
    assert_eq!(first_selection(3), Some(0));
}

#[test]
fn clamp_selection_handles_empty_missing_and_past_end() {
    assert_eq!(clamp_selection(Some(2), 0), None);
    assert_eq!(clamp_selection(None, 3), None);
    assert_eq!(clamp_selection(Some(4), 3), Some(2));
    assert_eq!(clamp_selection(Some(1), 3), Some(1));
}

#[test]
fn list_state_for_count_selects_first_nonempty_row() {
    assert_eq!(list_state_for_count(0).selected, None);
    assert_eq!(list_state_for_count(2).selected, Some(0));
}

#[test]
fn selected_choice_reads_only_valid_selection() {
    let choices = ["alpha", "beta"];

    assert_eq!(selected_choice(&choices, Some(1)), Some(&"beta"));
    assert_eq!(selected_choice(&choices, Some(2)), None);
    assert_eq!(selected_choice(&choices, None), None);
}

#[test]
fn matches_filter_accepts_empty_or_any_matching_value() {
    assert!(matches_filter("", ["anything"]));
    assert!(matches_filter("api", ["Stripe", "API token"]));
    assert!(matches_filter("VAULT", ["Personal Vault", "Secure Notes"]));
    assert!(!matches_filter("missing", ["one", "two"]));
}
