use super::*;

fn graph() -> WorkspaceGraph {
    WorkspaceGraph {
        names: BTreeMap::from([
            ("core-id".into(), "core".into()),
            ("runtime-id".into(), "runtime".into()),
            ("app-id".into(), "app".into()),
            ("tool-id".into(), "tool".into()),
        ]),
        roots: BTreeMap::from([
            ("core-id".into(), PathBuf::from("crates/core")),
            ("runtime-id".into(), PathBuf::from("crates/runtime")),
            ("app-id".into(), PathBuf::from("crates/app")),
            ("tool-id".into(), PathBuf::from("crates/tool")),
        ]),
        dependents: BTreeMap::from([
            ("core-id".into(), BTreeSet::from(["runtime-id".into()])),
            ("runtime-id".into(), BTreeSet::from(["app-id".into()])),
        ]),
    }
}

#[test]
fn selects_changed_crate_and_transitive_reverse_dependents() {
    assert_eq!(
        graph().affected(&[PathBuf::from("crates/core/src/lib.rs")]),
        ["app", "core", "runtime"]
    );
}

#[test]
fn keeps_unrelated_crates_out() {
    assert_eq!(
        graph().affected(&[PathBuf::from("crates/tool/src/main.rs")]),
        ["tool"]
    );
}

#[test]
fn selects_all_for_workspace_input_or_unknown_rust_path() {
    let expected = ["app", "core", "runtime", "tool"];
    assert_eq!(graph().affected(&[PathBuf::from("Cargo.lock")]), expected);
    assert_eq!(graph().affected(&[PathBuf::from("src/build.rs")]), expected);
}
