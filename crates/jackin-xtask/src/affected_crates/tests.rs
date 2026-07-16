use super::*;

fn graph() -> WorkspaceGraph {
    WorkspaceGraph {
        names: BTreeMap::from([
            ("core-id".into(), "core".into()),
            ("runtime-id".into(), "runtime".into()),
            ("app-id".into(), "jackin".into()),
            ("tool-id".into(), "tool".into()),
        ]),
        roots: BTreeMap::from([
            ("core-id".into(), PathBuf::from("crates/core")),
            ("runtime-id".into(), PathBuf::from("crates/runtime")),
            ("app-id".into(), PathBuf::from("crates/app")),
            ("tool-id".into(), PathBuf::from("crates/tool")),
        ]),
        dependencies: BTreeMap::from([
            ("runtime-id".into(), BTreeSet::from(["core-id".into()])),
            ("app-id".into(), BTreeSet::from(["runtime-id".into()])),
        ]),
        dependents: BTreeMap::from([
            ("core-id".into(), BTreeSet::from(["runtime-id".into()])),
            ("runtime-id".into(), BTreeSet::from(["app-id".into()])),
        ]),
    }
}

#[test]
fn metadata_snapshot_keeps_only_relocatable_crate_roots() {
    let graph = WorkspaceGraph::from_metadata(Metadata {
        packages: vec![
            Package {
                id: "core-id".into(),
                name: "core".into(),
                manifest_path: "/producer/repo/crates/core/Cargo.toml".into(),
            },
            Package {
                id: "app-id".into(),
                name: "jackin".into(),
                manifest_path: "/producer/repo/crates/jackin/Cargo.toml".into(),
            },
        ],
        workspace_members: BTreeSet::from(["core-id".into(), "app-id".into()]),
        resolve: Some(Resolve {
            nodes: vec![
                Node {
                    id: "core-id".into(),
                    deps: vec![],
                },
                Node {
                    id: "app-id".into(),
                    deps: vec![Dependency {
                        pkg: "core-id".into(),
                    }],
                },
            ],
        }),
    })
    .unwrap();

    assert_eq!(graph.roots["core-id"], PathBuf::from("crates/core"));
    assert_eq!(graph.roots["app-id"], PathBuf::from("crates/jackin"));
}

#[test]
fn cache_closure_follows_workspace_dependencies() {
    assert_eq!(
        graph().forward_closure("app-id"),
        BTreeSet::from(["app-id".into(), "core-id".into(), "runtime-id".into()])
    );
}

#[test]
fn selects_changed_crate_and_transitive_reverse_dependents() {
    assert_eq!(
        graph().affected(&[PathBuf::from("crates/core/src/lib.rs")]),
        ["core", "jackin", "runtime"]
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
fn ignores_documentation_alongside_changed_crates() {
    assert_eq!(
        graph().affected(&[
            PathBuf::from("crates/core/src/lib.rs"),
            PathBuf::from("crates/core/README.md"),
            PathBuf::from("docs/design.mdx"),
        ]),
        ["core", "jackin", "runtime"]
    );
    assert!(
        graph()
            .affected(&[PathBuf::from("docs/design.mdx")])
            .is_empty()
    );
}

#[test]
fn selects_all_for_workspace_input_or_unknown_rust_path() {
    let expected = ["core", "jackin", "runtime", "tool"];
    assert_eq!(graph().affected(&[PathBuf::from("Cargo.lock")]), expected);
    assert_eq!(graph().affected(&[PathBuf::from("src/build.rs")]), expected);
}

#[test]
fn routes_docker_inputs_only_to_the_jackin_e2e_owner() {
    assert_eq!(
        graph().affected(&[
            PathBuf::from("docker/construct/Dockerfile"),
            PathBuf::from("docker-bake.hcl"),
        ]),
        ["jackin"]
    );
}
