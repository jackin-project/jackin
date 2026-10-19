// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::*;

#[test]
fn cyclic_prefix_aliases_stop_and_retain_finite_sibling_branches() {
    let imports = TelemetryImports {
        aliases: BTreeMap::from([
            (
                String::from("a"),
                BTreeSet::from([String::from("b"), String::from("tracing")]),
            ),
            (
                String::from("b"),
                BTreeSet::from([String::from("a::nested")]),
            ),
        ]),
        globs: BTreeSet::new(),
        modules: BTreeSet::new(),
    };

    let paths = imports.paths("a::info");

    assert!(paths.unresolved_cycle);
    assert!(paths.candidates.contains("tracing::info"));
    assert!(paths.candidates.contains("a::nested::info"));
    assert!(paths.candidates.len() <= 4, "{:#?}", paths.candidates);
    assert_eq!(
        imports.is_tracing_macro("a::info"),
        TelemetryImportMatch::Matched
    );
}

#[test]
fn finite_transitive_and_ambiguous_aliases_keep_all_resolutions() {
    let imports = TelemetryImports {
        aliases: BTreeMap::from([
            (
                String::from("emit"),
                BTreeSet::from([String::from("local"), String::from("tracing")]),
            ),
            (
                String::from("local"),
                BTreeSet::from([String::from("tracing::event")]),
            ),
        ]),
        globs: BTreeSet::new(),
        modules: BTreeSet::new(),
    };

    let paths = imports.paths("emit");

    assert!(!paths.unresolved_cycle);
    assert!(paths.candidates.contains("tracing"));
    assert!(paths.candidates.contains("tracing::event"));
    assert_eq!(
        imports.is_tracing_macro("emit"),
        TelemetryImportMatch::Matched
    );
}
