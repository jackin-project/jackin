//! Tests for `branch`.
use super::*;

#[test]
fn no_suffix_no_namespace() {
    assert_eq!(
        branch_name("the-architect", None),
        "jackin/scratch/the-architect"
    );
}

#[test]
fn no_suffix_with_namespace() {
    assert_eq!(
        branch_name("chainargos/the-architect", None),
        "jackin/scratch/chainargos/the-architect"
    );
}

#[test]
fn suffix_appends_to_final_segment_with_namespace() {
    assert_eq!(
        branch_name("chainargos/the-architect", Some("repo-1")),
        "jackin/scratch/chainargos/the-architect-repo-1"
    );
}

#[test]
fn suffix_appends_without_namespace() {
    assert_eq!(
        branch_name("the-architect", Some("repo-2")),
        "jackin/scratch/the-architect-repo-2"
    );
}
