// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{WorkspaceSpawnTypesBuilder, spawn_receiver_type};
use std::sync::Arc;

#[test]
fn per_file_resolvers_share_the_workspace_alias_index() {
    let aliases_source =
        std::iter::once("pub type BaseExecutor = tokio::runtime::Handle;\n".to_owned())
            .chain((0..128).map(|index| format!("pub type Executor{index} = BaseExecutor;")))
            .chain([
                "pub type CycleA = CycleB;".to_owned(),
                "pub type CycleB = CycleA;".to_owned(),
            ])
            .collect::<Vec<_>>()
            .join("\n");
    let aliases = syn::parse_file(&aliases_source).expect("alias fixture parses");
    let consumer = syn::parse_file(
        "type LocalExecutor = String; fn run(_: foo::Executor0, _: LocalExecutor) {}",
    )
    .expect("consumer fixture parses");
    let files = [
        ("crates/services/foo/src/lib.rs", &aliases),
        ("crates/services/bar/src/lib.rs", &consumer),
    ];
    let mut builder = WorkspaceSpawnTypesBuilder::new(files.iter().map(|(path, _)| *path));
    for &(path, syntax) in &files {
        builder.add_file(path, syntax);
    }
    let workspace = builder.finish();
    let resolvers = (0..128)
        .map(|index| workspace.resolver(&format!("crates/services/bar/src/file{index}.rs")))
        .collect::<Vec<_>>();

    assert_eq!(workspace.aliases.len(), 132);
    assert_eq!(Arc::strong_count(&workspace.aliases), 129);
    assert_eq!(Arc::strong_count(&workspace.crate_names), 129);
    assert!(resolvers.iter().all(|resolver| {
        Arc::ptr_eq(&workspace.aliases, &resolver.aliases)
            && Arc::ptr_eq(&workspace.crate_names, &resolver.crate_names)
    }));
    let transitive = syn::parse_str("foo::Executor0").expect("type fixture parses");
    assert!(spawn_receiver_type(&transitive, &resolvers[0]));
    let cycle = syn::parse_str("foo::CycleA").expect("cycle fixture parses");
    assert!(!spawn_receiver_type(&cycle, &resolvers[0]));
    let local_shadow = syn::parse_str("LocalExecutor").expect("local type fixture parses");
    assert!(!spawn_receiver_type(&local_shadow, &resolvers[0]));
}

#[test]
fn streaming_index_handles_many_cross_file_aliases_and_cycles() {
    const ALIAS_COUNT: usize = 256;

    let mut sources = vec![(
        "crates/services/foo/src/lib.rs".to_owned(),
        "pub type BaseExecutor = tokio::runtime::Handle;".to_owned(),
    )];
    for index in 0..ALIAS_COUNT {
        let target = if index == 0 {
            "crate::BaseExecutor".to_owned()
        } else {
            format!("crate::layer{}::Executor{}", index - 1, index - 1)
        };
        sources.push((
            format!("crates/services/foo/src/layer{index}.rs"),
            format!("pub type Executor{index} = {target};"),
        ));
    }
    sources.extend([
        (
            "crates/services/foo/src/cycle_a.rs".to_owned(),
            "pub type CycleA = crate::cycle_b::CycleB;".to_owned(),
        ),
        (
            "crates/services/foo/src/cycle_b.rs".to_owned(),
            "pub type CycleB = crate::cycle_a::CycleA;".to_owned(),
        ),
        (
            "crates/services/bar/src/lib.rs".to_owned(),
            "type LocalExecutor = String;".to_owned(),
        ),
    ]);

    let mut builder =
        WorkspaceSpawnTypesBuilder::new(sources.iter().map(|(path, _)| path.as_str()));
    for (path, source) in &sources {
        let syntax = syn::parse_file(source).expect("streaming source fixture parses");
        builder.add_file(path, &syntax);
    }
    let workspace = builder.finish();
    assert_eq!(workspace.aliases.len(), ALIAS_COUNT + 4);

    let resolver = workspace.resolver("crates/services/bar/src/lib.rs");
    let transitive = syn::parse_str(&format!(
        "foo::layer{}::Executor{}",
        ALIAS_COUNT - 1,
        ALIAS_COUNT - 1
    ))
    .expect("cross-file alias type parses");
    assert!(spawn_receiver_type(&transitive, &resolver));
    let cycle = syn::parse_str("foo::cycle_a::CycleA").expect("cycle type parses");
    assert!(!spawn_receiver_type(&cycle, &resolver));
    let local_shadow = syn::parse_str("LocalExecutor").expect("local type parses");
    assert!(!spawn_receiver_type(&local_shadow, &resolver));
}
