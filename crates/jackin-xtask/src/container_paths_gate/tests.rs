use std::collections::BTreeMap;

#[test]
fn measure_pattern_counts_quoted_jackin() {
    // Pure string exercise of the match used by measure_literals.
    let sample = r#"
        const A: &str = "/jackin/runtime";
        const B: &str = "/jackin/state";
        let x = "not-jackin";
        let y = "/run/x";
    "#;
    assert_eq!(sample.matches("\"/jackin").count(), 2);
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
