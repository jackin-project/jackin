use super::*;
use std::collections::BTreeMap;

#[test]
fn rejects_empty() {
    assert!(matches!(
        WorkspaceName::parse(""),
        Err(WorkspaceNameError::Empty)
    ));
    assert_eq!(
        WorkspaceName::parse("").unwrap_err().to_string(),
        "workspace name cannot be empty"
    );
}

#[test]
fn rejects_reserved_dots() {
    for name in [".", ".."] {
        let err = WorkspaceName::parse(name).unwrap_err();
        assert!(matches!(err, WorkspaceNameError::Reserved(_)));
        assert_eq!(
            err.to_string(),
            format!("workspace name {name:?} is reserved")
        );
    }
}

#[test]
fn rejects_path_separators() {
    for name in ["a/b", "a\\b"] {
        let err = WorkspaceName::parse(name).unwrap_err();
        assert!(matches!(err, WorkspaceNameError::PathSeparator(_)));
        assert_eq!(
            err.to_string(),
            format!("workspace name {name:?} cannot contain path separators")
        );
    }
}

#[test]
fn accepts_valid_names() {
    for name in ["prod", "my-workspace", "ws_1", "a"] {
        let wn = WorkspaceName::parse(name).unwrap();
        assert_eq!(wn.as_str(), name);
        assert_eq!(wn.to_string(), name);
        assert_eq!(wn.clone().into_inner(), name);
        assert_eq!(WorkspaceName::try_from(name).unwrap(), wn);
    }
}

#[test]
fn display_round_trip() {
    let wn = WorkspaceName::parse("chainargos").unwrap();
    assert_eq!(format!("{wn}"), "chainargos");
}

#[test]
fn borrow_str_map_lookup() {
    let wn = WorkspaceName::parse("prod").unwrap();
    let mut map: BTreeMap<String, i32> = BTreeMap::new();
    map.insert(wn.as_str().to_owned(), 7);
    assert_eq!(map.get(wn.as_str()), Some(&7));
    let mut typed: BTreeMap<WorkspaceName, i32> = BTreeMap::new();
    typed.insert(wn.clone(), 9);
    assert_eq!(typed.get("prod"), Some(&9));
}
