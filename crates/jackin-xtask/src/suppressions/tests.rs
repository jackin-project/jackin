use super::*;

fn measured_bare(pairs: &[(&str, usize)]) -> Measured {
    let mut m = Measured::default();
    for &(name, n) in pairs {
        m.bare_by_crate.insert(name.to_owned(), n);
    }
    m
}

fn measured_full(
    bare: &[(&str, usize)],
    expects: &[(&str, &str, usize)],
) -> Measured {
    let mut m = measured_bare(bare);
    for &(lint, crate_name, n) in expects {
        m.expect_by_lint_crate
            .insert((lint.to_owned(), crate_name.to_owned()), n);
    }
    m
}

fn budget_from(
    crates: &[(&str, usize)],
    expects: &[(&str, &str, usize)],
) -> Budget {
    Budget {
        crates: crates
            .iter()
            .map(|&(name, bare_allow)| CrateBudget {
                name: name.to_owned(),
                bare_allow,
            })
            .collect(),
        expects: expects
            .iter()
            .map(|&(lint, crate_name, count)| ExpectBudget {
                lint: lint.to_owned(),
                crate_name: crate_name.to_owned(),
                count,
            })
            .collect(),
    }
}

#[test]
fn passes_when_budget_matches_measured() {
    let budget = budget_from(
        &[("jackin-console", 10), ("jackin-runtime", 5)],
        &[("clippy::too_many_lines", "jackin-runtime", 3)],
    );
    let measured = measured_full(
        &[("jackin-console", 10), ("jackin-runtime", 5)],
        &[("clippy::too_many_lines", "jackin-runtime", 3)],
    );
    assert!(check(&budget, &measured).is_ok());
}

#[test]
fn fails_when_bare_allow_grows() {
    let budget = budget_from(&[("jackin-console", 10)], &[]);
    let measured = measured_bare(&[("jackin-console", 12)]);
    let err = check(&budget, &measured).unwrap_err().to_string();
    assert!(err.contains("jackin-console"), "{err}");
    assert!(err.contains("grew from 10 to 12"), "{err}");
    assert!(err.contains("--print-budget"), "{err}");
}

#[test]
fn fails_when_bare_allow_shrinks_without_budget_update() {
    let budget = budget_from(&[("jackin-console", 10)], &[]);
    let measured = measured_bare(&[("jackin-console", 8)]);
    let err = check(&budget, &measured).unwrap_err().to_string();
    assert!(err.contains("jackin-console"), "{err}");
    assert!(err.contains("shrunk from 10 to 8"), "{err}");
    assert!(err.contains("shrink-only"), "{err}");
}

#[test]
fn fails_when_crate_at_zero_still_listed() {
    let budget = budget_from(&[("jackin-console", 10)], &[]);
    let measured = Measured::default();
    let err = check(&budget, &measured).unwrap_err().to_string();
    assert!(err.contains("jackin-console"), "{err}");
    assert!(err.contains("now 0"), "{err}");
    assert!(err.contains("delete the stale"), "{err}");
}

#[test]
fn fails_when_unbudgeted_bare_crate_appears() {
    let budget = budget_from(&[], &[]);
    let measured = measured_bare(&[("jackin-new", 2)]);
    let err = check(&budget, &measured).unwrap_err().to_string();
    assert!(err.contains("jackin-new"), "{err}");
    assert!(err.contains("not in"), "{err}");
}

#[test]
fn fails_when_expect_grows() {
    let budget = budget_from(&[], &[("clippy::panic", "jackin-tui", 1)]);
    let measured = measured_full(&[], &[("clippy::panic", "jackin-tui", 2)]);
    let err = check(&budget, &measured).unwrap_err().to_string();
    assert!(err.contains("clippy::panic"), "{err}");
    assert!(err.contains("jackin-tui"), "{err}");
    assert!(err.contains("grew from 1 to 2"), "{err}");
}

#[test]
fn fails_when_expect_shrinks_without_budget_update() {
    let budget = budget_from(&[], &[("clippy::panic", "jackin-tui", 3)]);
    let measured = measured_full(&[], &[("clippy::panic", "jackin-tui", 1)]);
    let err = check(&budget, &measured).unwrap_err().to_string();
    assert!(err.contains("shrunk from 3 to 1"), "{err}");
}

#[test]
fn fails_when_expect_row_at_zero_still_listed() {
    let budget = budget_from(&[], &[("clippy::panic", "jackin-tui", 1)]);
    let measured = Measured::default();
    let err = check(&budget, &measured).unwrap_err().to_string();
    assert!(err.contains("now 0"), "{err}");
    assert!(err.contains("delete the stale"), "{err}");
}

#[test]
fn print_budget_round_trips_through_toml() {
    let measured = measured_full(
        &[("jackin-console", 4), ("jackin-runtime", 2)],
        &[
            ("clippy::too_many_lines", "jackin-runtime", 3),
            ("missing_debug_implementations", "jackin-capsule", 1),
        ],
    );
    let text = print_budget(&measured);
    let parsed: Budget = toml::from_str(&text).expect("budget TOML parses");
    assert_eq!(parsed.crates.len(), 2);
    assert_eq!(parsed.expects.len(), 2);
    assert!(check(&parsed, &measured).is_ok());
}

#[test]
fn parse_budget_toml_shape() {
    let text = r#"
[[crate]]
name = "jackin-console"
bare_allow = 120

[[expect]]
lint = "clippy::too_many_lines"
crate = "jackin-runtime"
count = 5
"#;
    let budget: Budget = toml::from_str(text).unwrap();
    assert_eq!(budget.crates[0].name, "jackin-console");
    assert_eq!(budget.crates[0].bare_allow, 120);
    assert_eq!(budget.expects[0].lint, "clippy::too_many_lines");
    assert_eq!(budget.expects[0].crate_name, "jackin-runtime");
    assert_eq!(budget.expects[0].count, 5);
}
