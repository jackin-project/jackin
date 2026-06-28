use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Smoke-test the forbidden-edge extraction: build a synthetic deps map and
/// assert the gate flags only the entries on the `FORBIDDEN_EDGES` list.
#[test]
fn synthetic_graph_flags_only_listed_forbidden_edges() {
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    deps.insert(
        "jackin-runtime".into(),
        BTreeSet::from(["jackin-core".into(), "jackin-tui".into()]),
    );
    deps.insert(
        "jackin-config".into(),
        BTreeSet::from(["jackin-core".into(), "jackin-diagnostics".into()]),
    );
    deps.insert(
        "jackin-manifest".into(),
        BTreeSet::from(["jackin-diagnostics".into()]),
    );

    let mut problems = Vec::new();
    for (from, to) in FORBIDDEN_EDGES {
        if let Some(actual) = deps.get(*from)
            && actual.contains(*to)
        {
            problems.push(format!("{from} → {to}"));
        }
    }
    problems.sort();
    assert_eq!(
        problems,
        vec![
            "jackin-config → jackin-diagnostics",
            "jackin-manifest → jackin-diagnostics",
            "jackin-runtime → jackin-tui",
        ]
    );
}

#[test]
fn synthetic_graph_passes_when_clean() {
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    deps.insert(
        "jackin-runtime".into(),
        BTreeSet::from(["jackin-core".into(), "jackin-config".into()]),
    );
    deps.insert(
        "jackin-config".into(),
        BTreeSet::from(["jackin-core".into()]),
    );
    deps.insert(
        "jackin-manifest".into(),
        BTreeSet::from(["jackin-core".into()]),
    );
    let mut problems = Vec::new();
    for (from, to) in FORBIDDEN_EDGES {
        if let Some(actual) = deps.get(*from)
            && actual.contains(*to)
        {
            problems.push(format!("{from} → {to}"));
        }
    }
    assert!(problems.is_empty());
}