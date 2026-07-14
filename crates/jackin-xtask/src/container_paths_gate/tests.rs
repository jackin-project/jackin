use super::{cargo_declares_turso, is_forbidden_root_literal, turso_import_column};
use std::collections::BTreeMap;

#[test]
fn forbidden_root_predicate_matches_roots_and_tmp_jackin() {
    assert!(is_forbidden_root_literal("/run/x"));
    assert!(is_forbidden_root_literal("/var/tmp"));
    assert!(is_forbidden_root_literal("/opt/foo"));
    assert!(is_forbidden_root_literal("/etc/hostname"));
    assert!(is_forbidden_root_literal("/tmp/jackin-foo"));
    assert!(is_forbidden_root_literal("/tmp/jackin/jk-run.jsonl"));
    assert!(!is_forbidden_root_literal("/tmp/other"));
    assert!(!is_forbidden_root_literal("/jackin/run"));
}

#[test]
fn turso_import_detects_use_and_path_not_log_string() {
    assert!(turso_import_column("use turso::Builder;").is_some());
    assert!(turso_import_column("let db = turso::Builder::new_local(p);").is_some());
    // Log-string mention without path form must not trip the gate.
    assert!(turso_import_column(r#"clog!("turso store ready");"#).is_none());
}

#[test]
fn cargo_declares_turso_reads_dep_lines() {
    assert!(cargo_declares_turso("turso = { workspace = true }\n"));
    assert!(cargo_declares_turso("libsql = \"0.1\"\n"));
    assert!(!cargo_declares_turso("# turso = { workspace = true }\n"));
    assert!(!cargo_declares_turso("serde = \"1\"\n"));
}

#[test]
fn shrink_only_logic_growth_and_stale() {
    let measured: BTreeMap<String, usize> =
        BTreeMap::from([("crates/a.rs".into(), 3), ("crates/b.rs".into(), 1)]);
    let recorded: BTreeMap<String, usize> = BTreeMap::from([
        ("crates/a.rs".into(), 2), // growth
        ("crates/c.rs".into(), 5), // stale
    ]);
    let mut problems = Vec::new();
    for (path, budgeted) in &recorded {
        match measured.get(path) {
            None => problems.push(format!("{path}: stale")),
            Some(&n) if n < *budgeted => problems.push(format!("{path}: shrink")),
            Some(&n) if n > *budgeted => problems.push(format!("{path}: grew")),
            Some(_) => {}
        }
    }
    for path in measured.keys() {
        if !recorded.contains_key(path) {
            problems.push(format!("{path}: unallowlisted"));
        }
    }
    assert!(problems.iter().any(|p| p.contains("grew")));
    assert!(problems.iter().any(|p| p.contains("stale")));
    assert!(problems.iter().any(|p| p.contains("unallowlisted")));
}

#[test]
fn forbidden_root_in_production_literal_fails_predicate() {
    // Characterization: a production `/run/x` string is a forbidden-root hit
    // (inverts the old gate's ignore-assertion for /run).
    assert!(is_forbidden_root_literal("/run/x"));
}
