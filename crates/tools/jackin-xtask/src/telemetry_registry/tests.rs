// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::namespace::NamespaceScanner;
use super::ownership_census;
use super::source_policy::WorkspaceSpawnTypes;
use super::{
    SourcePolicyScanner, event_runtime_severity, generate_rust_sources, repo_root, rust_pascal,
    validate_registry_matches_rust,
};
use syn::visit::Visit as _;

fn source_policy_violations(path: &str, source: &str) -> Vec<&'static str> {
    source_policy_violations_for_files(&[(path, source)])
}

fn source_policy_violations_for_files(files: &[(&str, &str)]) -> Vec<&'static str> {
    let parsed = files
        .iter()
        .map(|(path, source)| {
            (
                (*path).to_owned(),
                syn::parse_file(source).expect("source-policy fixture must parse"),
            )
        })
        .collect::<Vec<_>>();
    let indexed = parsed
        .iter()
        .map(|(path, syntax)| (path.as_str(), syntax))
        .collect::<Vec<_>>();
    let workspace = WorkspaceSpawnTypes::collect(&indexed);
    let mut violations = Vec::new();
    for (path, syntax) in &parsed {
        let mut scanner = SourcePolicyScanner::new(path, syntax, &workspace);
        scanner.visit_file(syntax);
        violations.extend(scanner.violations.iter().map(|(_, violation)| *violation));
    }
    violations.sort_unstable();
    violations
}

fn contains_legacy_telemetry_name(path: &str, source: &str) -> bool {
    let fixture = format!("fn namespace_fixture() {{ {source} }}");
    let Ok(syntax) = syn::parse_file(&fixture) else {
        return false;
    };
    let mut scanner = NamespaceScanner::new(path);
    scanner.visit_file(&syntax);
    !scanner.violations.is_empty()
}

mod case_01;
mod case_02;
