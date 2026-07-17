use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn members(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

fn edges(pairs: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
    pairs
        .iter()
        .map(|(from, tos)| {
            (
                (*from).to_owned(),
                tos.iter().map(|t| (*t).to_owned()).collect(),
            )
        })
        .collect()
}

fn tiers<'a>(pairs: &'a [(&'a str, u8)]) -> BTreeMap<&'a str, u8> {
    pairs.iter().copied().collect()
}

#[test]
fn clean_graph_produces_no_problems() {
    let t = tiers(&[("a", 0), ("b", 1), ("c", 2)]);
    let prod = edges(&[("c", &["b", "a"]), ("b", &["a"]), ("a", &[])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "b", "c"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn missing_tier_crate_fails_with_fix() {
    let t = tiers(&[("a", 0)]);
    let prod = edges(&[("a", &[]), ("orphan", &["a"])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "orphan"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("orphan") && p.contains("add it to TIERS")),
        "{problems:?}"
    );
}

#[test]
fn upward_production_edge_fails_with_strictly_lower_tier() {
    let t = tiers(&[("low", 0), ("high", 2)]);
    let prod = edges(&[("low", &["high"]), ("high", &[])]);
    let dev = BTreeMap::new();
    let m = members(&["low", "high"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("strictly lower tier") && p.contains("low") && p.contains("high")),
        "{problems:?}"
    );
}

#[test]
fn same_tier_production_edge_fails() {
    let t = tiers(&[("a", 1), ("b", 1)]);
    let prod = edges(&[("a", &["b"]), ("b", &[])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "b"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems.iter().any(|p| p.contains("strictly lower tier")),
        "{problems:?}"
    );
}

#[test]
fn production_cycle_is_reported() {
    let t = tiers(&[("a", 0), ("b", 1)]);
    let prod = edges(&[("a", &["b"]), ("b", &["a"])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "b"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("production dependency cycle")),
        "{problems:?}"
    );
}

#[test]
fn dev_cycle_not_in_allowlist_fails() {
    let t = tiers(&[("a", 0), ("b", 1)]);
    let prod = edges(&[("b", &["a"]), ("a", &[])]);
    let dev = edges(&[("a", &["b"])]);
    let m = members(&["a", "b"]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("--dev-->") && p.contains("not in DEV_CYCLE_ALLOWLIST")),
        "{problems:?}"
    );
}

#[test]
fn allowlisted_dev_cycle_is_quiet() {
    let t = tiers(&[("a", 0), ("b", 1)]);
    let prod = edges(&[("b", &["a"]), ("a", &[])]);
    let dev = edges(&[("a", &["b"])]);
    let m = members(&["a", "b"]);
    let allow = &[("a", "b")];
    let problems = evaluate(&t, &prod, &dev, &m, allow);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn stale_allowlist_row_fails() {
    let t = tiers(&[("a", 0), ("b", 1)]);
    let prod = edges(&[("b", &["a"]), ("a", &[])]);
    let dev = BTreeMap::new();
    let m = members(&["a", "b"]);
    let allow = &[("a", "b")];
    let problems = evaluate(&t, &prod, &dev, &m, allow);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("remove the stale") || p.contains("stale allowlist")),
        "{problems:?}"
    );
}

#[test]
fn real_tiers_table_covers_every_expected_member() {
    let expected: BTreeSet<&str> = [
        "jackin",
        "jackin-agent-status",
        "jackin-build-meta",
        "jackin-brand",
        "jackin-capsule",
        "jackin-config",
        "jackin-console",
        "jackin-oppicker",
        "jackin-core",
        "jackin-dev",
        "jackin-diagnostics",
        "jackin-docker",
        "jackin-env",
        "jackin-host",
        "jackin-image",
        "jackin-instance",
        "jackin-isolation",
        "jackin-launch",
        "jackin-manifest",
        "jackin-otlp-testbed",
        "jackin-pr-trailers",
        "jackin-process",
        "jackin-protocol",
        "jackin-runtime",
        "jackin-telemetry",
        "jackin-term",
        "jackin-tui",
        "jackin-test-support",
        "jackin-usage",
        "jackin-xtask",
    ]
    .into_iter()
    .collect();
    let declared: BTreeSet<&str> = TIERS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        declared, expected,
        "TIERS drifted from the pinned member set — update both if a crate was added/removed"
    );
    assert_eq!(TIERS.len(), 30);
}

#[test]
fn evaluate_accepts_live_tier_shape_on_synthetic_clean_graph() {
    let t: BTreeMap<&str, u8> = TIERS.iter().copied().collect();
    let prod = edges(&[
        ("jackin-core", &[]),
        ("jackin-config", &["jackin-core"]),
        ("jackin-manifest", &["jackin-config", "jackin-core"]),
        ("jackin-test-support", &["jackin-core", "jackin-manifest"]),
    ]);
    let dev = BTreeMap::new();
    let m = members(&[
        "jackin-core",
        "jackin-config",
        "jackin-manifest",
        "jackin-test-support",
    ]);
    let problems = evaluate(&t, &prod, &dev, &m, &[]);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn tui_ownership_gate_accepts_clean_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "crates/jackin-core/src",
        "crates/jackin-runtime/src",
        "crates/jackin-tui/src",
    ] {
        std::fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    std::fs::write(
        temp.path().join("crates/jackin-core/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("crates/jackin-runtime/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("crates/jackin-tui/src/lib.rs"),
        "pub mod tokens;\n",
    )
    .unwrap();

    check_tui_ownership(temp.path()).unwrap();
}

#[test]
fn tui_ownership_gate_rejects_core_presentation_and_shared_run_loop() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "crates/jackin-core/src",
        "crates/jackin-runtime/src",
        "crates/jackin-tui/src",
    ] {
        std::fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    std::fs::write(
        temp.path().join("crates/jackin-core/Cargo.toml"),
        "[dependencies]\nratatui = \"*\"\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("crates/jackin-runtime/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("crates/jackin-core/src/lib.rs"),
        "use termrock::Theme;\n",
    )
    .unwrap();
    std::fs::write(temp.path().join("crates/jackin-tui/src/run.rs"), "").unwrap();

    let error = check_tui_ownership(temp.path()).unwrap_err().to_string();
    assert!(error.contains("jackin-core/Cargo.toml"), "{error}");
    assert!(error.contains("jackin-core/src/lib.rs"), "{error}");
    assert!(error.contains("jackin-tui/src/run.rs"), "{error}");
}

#[test]
fn tui_ownership_gate_rejects_surface_local_host_color_handshake() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "crates/jackin-core/src",
        "crates/jackin-runtime/src/runtime",
        "crates/jackin-tui/src",
        "crates/jackin-capsule/src/tui",
    ] {
        std::fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    std::fs::write(
        temp.path().join("crates/jackin-core/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("crates/jackin-runtime/Cargo.toml"),
        "[package]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path()
            .join("crates/jackin-capsule/src/tui/host_colors.rs"),
        "",
    )
    .unwrap();

    let error = check_tui_ownership(temp.path()).unwrap_err().to_string();
    assert!(error.contains("host_colors.rs"), "{error}");
    assert!(error.contains("jackin-protocol"), "{error}");
}
