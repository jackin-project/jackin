//! Tests for `workspace_label`.
use super::*;
use crate::WorkspaceName;

#[test]
fn empty_label_rejected() {
    assert!(matches!(
        WorkspaceLabel::parse(""),
        Err(WorkspaceLabelError::Empty)
    ));
}

#[test]
fn path_label_allowed_where_config_stem_is_not() {
    let path = "/home/op/projects/chainargos";
    assert!(
        WorkspaceName::parse(path).is_err(),
        "config stem must reject path separators"
    );
    let label = WorkspaceLabel::parse(path).expect("path label is legal");
    assert_eq!(label.as_str(), path);
}

#[test]
fn from_name_preserves_config_stem() {
    let name = WorkspaceName::parse("chainargos").unwrap();
    let label = WorkspaceLabel::from_name(&name);
    assert_eq!(label.as_str(), "chainargos");
    assert_eq!(label.as_str(), name.as_str());
    let from_owned = WorkspaceLabel::from(WorkspaceName::parse("chainargos").unwrap());
    assert_eq!(from_owned.as_str(), "chainargos");
}
