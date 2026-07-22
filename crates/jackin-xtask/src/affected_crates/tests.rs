use super::*;

fn graph() -> WorkspaceGraph {
    WorkspaceGraph {
        names: BTreeMap::from([
            ("core-id".into(), "core".into()),
            ("runtime-id".into(), "runtime".into()),
            ("app-id".into(), "jackin".into()),
            ("tool-id".into(), "tool".into()),
        ]),
        package_names: BTreeMap::from([
            ("core-id".into(), "core".into()),
            ("runtime-id".into(), "runtime".into()),
            ("app-id".into(), "jackin".into()),
            ("tool-id".into(), "tool".into()),
            ("serde 1.0.0".into(), "serde".into()),
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
        resolved_dependencies: BTreeMap::from([
            ("core-id".into(), BTreeSet::from(["serde 1.0.0".into()])),
            ("runtime-id".into(), BTreeSet::from(["core-id".into()])),
            ("app-id".into(), BTreeSet::from(["runtime-id".into()])),
        ]),
        resolved_features: BTreeMap::from([(
            "serde 1.0.0".into(),
            BTreeSet::from(["derive".into()]),
        )]),
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
                    features: BTreeSet::new(),
                },
                Node {
                    id: "app-id".into(),
                    deps: vec![Dependency {
                        pkg: "core-id".into(),
                    }],
                    features: BTreeSet::new(),
                },
            ],
        }),
    })
    .unwrap();

    assert_eq!(graph.roots["core-id"], PathBuf::from("crates/core"));
    assert_eq!(graph.roots["app-id"], PathBuf::from("crates/jackin"));
}

#[test]
fn resolved_cache_closure_includes_external_dependencies() {
    assert_eq!(
        graph().resolved_forward_closure("app-id"),
        BTreeSet::from([
            "app-id".into(),
            "core-id".into(),
            "runtime-id".into(),
            "serde 1.0.0".into(),
        ])
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
fn ignores_ci_orchestration_for_crate_selection() {
    assert!(
        graph()
            .affected(&[
                PathBuf::from(".github/workflows/ci.yml"),
                PathBuf::from(".github/actions/cache-cargo-registry/action.yml"),
            ])
            .is_empty()
    );
}

#[test]
fn selects_all_for_workspace_input_or_unknown_rust_path() {
    let expected = ["core", "jackin", "runtime", "tool"];
    assert_eq!(graph().affected(&[PathBuf::from("Cargo.toml")]), expected);
    assert_eq!(graph().affected(&[PathBuf::from("src/build.rs")]), expected);
}

#[test]
fn lock_changes_select_only_resolved_consumers_and_dependents() {
    assert_eq!(
        graph().affected_with_dependencies(
            &[PathBuf::from("Cargo.lock")],
            Some(&BTreeSet::from(["serde".into()])),
            None,
        ),
        ["core", "jackin", "runtime"]
    );
}

#[test]
fn unknown_removed_lock_package_fails_safe_to_every_crate() {
    assert_eq!(
        graph().affected_with_dependencies(
            &[PathBuf::from("Cargo.lock")],
            Some(&BTreeSet::from(["removed".into()])),
            None,
        ),
        ["core", "jackin", "runtime", "tool"]
    );
}

#[test]
fn lock_parser_detects_package_record_changes() {
    let base = br#"version = 4
[[package]]
name = "serde"
version = "1.0.0"
checksum = "old"
"#;
    let head = br#"version = 4
[[package]]
name = "serde"
version = "1.0.0"
checksum = "new"
"#;
    let base = lock_packages(base).unwrap();
    let head = lock_packages(head).unwrap();
    assert_ne!(base, head);
}

#[test]
fn workspace_dependency_only_change_is_not_global() {
    let base = br#"[workspace]
members = ["crates/*"]

[workspace.dependencies]
serde = "1"
"#;
    let head = br#"[workspace]
members = ["crates/*"]

[workspace.dependencies]
serde = "2"
"#;
    let mut base = manifest_value(base).unwrap();
    let mut head = manifest_value(head).unwrap();
    let base_dependencies = take_workspace_dependencies(&mut base);
    let head_dependencies = take_workspace_dependencies(&mut head);
    assert_eq!(base, head);
    assert_ne!(base_dependencies, head_dependencies);
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
