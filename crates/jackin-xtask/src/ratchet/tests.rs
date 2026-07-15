//! Unit tests for pure ratchet semantics.

use super::{
    NumericVerdict, PresenceVerdict, check_curated_pub_mods, check_numeric_entry,
    check_numeric_unlisted, check_presence,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[test]
fn numeric_growth_fails() {
    assert_eq!(
        check_numeric_entry(Some(2000), 1938, 1850),
        NumericVerdict::Growth {
            measured: 2000,
            budgeted: 1938
        }
    );
}

#[test]
fn numeric_shrink_force_fails() {
    assert_eq!(
        check_numeric_entry(Some(1900), 1938, 1850),
        NumericVerdict::Shrink {
            measured: 1900,
            budgeted: 1938
        }
    );
}

#[test]
fn numeric_stale_under_cap_fails() {
    assert_eq!(
        check_numeric_entry(Some(1000), 1938, 1850),
        NumericVerdict::StaleUnderCap { measured: 1000 }
    );
}

#[test]
fn numeric_missing_stale_fails() {
    assert_eq!(
        check_numeric_entry(None, 1938, 1850),
        NumericVerdict::StaleMissing
    );
}

#[test]
fn numeric_steady_state_ok() {
    assert_eq!(
        check_numeric_entry(Some(1938), 1938, 1850),
        NumericVerdict::Ok
    );
}

#[test]
fn numeric_unlisted_over_cap_fails() {
    assert_eq!(
        check_numeric_unlisted(2000, 1850),
        NumericVerdict::UnlistedOverCap {
            measured: 2000,
            cap: 1850
        }
    );
}

#[test]
fn presence_stale_and_new() {
    let mut violations = BTreeMap::new();
    violations.insert("a.rs".into(), "bad".into());
    let mut allowed = BTreeSet::new();
    allowed.insert("b.rs".into());
    let v = check_presence(&violations, &allowed);
    assert!(
        v.iter()
            .any(|(k, ver)| k == "b.rs" && *ver == PresenceVerdict::Stale)
    );
    assert!(v.iter().any(|(k, ver)| {
        k == "a.rs" && matches!(ver, PresenceVerdict::New { reason } if reason == "bad")
    }));
}

#[test]
fn curated_pub_mods_rejects_extra_root_mod() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Valid curated siblings so the only failure is the intentional leak.
    for (crate_name, body) in [
        ("jackin-config", "mod private;\npub mod test_support;\n"),
        (
            "jackin-core",
            "mod private;\npub mod container_paths;\npub mod debug_log;\n",
        ),
    ] {
        let lib = dir
            .path()
            .join("crates")
            .join(crate_name)
            .join("src/lib.rs");
        fs::create_dir_all(lib.parent().expect("parent")).expect("mkdir");
        fs::write(&lib, body).expect("write lib");
    }
    let lib = dir.path().join("crates/jackin-env/src/lib.rs");
    fs::create_dir_all(lib.parent().expect("parent")).expect("mkdir");
    fs::write(
        &lib,
        "mod env_layer;\npub mod test_support;\npub mod leaked;\n",
    )
    .expect("write lib");
    let err = check_curated_pub_mods(dir.path()).expect_err("extra pub mod must fail");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("leaked") && msg.contains("jackin-env"),
        "unexpected message: {msg}"
    );
}

#[test]
fn curated_pub_mods_accepts_env_pilot_shape() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shapes = [
        ("jackin-env", "mod env_layer;\npub mod test_support;\n"),
        ("jackin-config", "mod private;\npub mod test_support;\n"),
        (
            "jackin-core",
            "mod private;\npub mod container_paths;\npub mod debug_log;\n",
        ),
    ];
    for (crate_name, body) in shapes {
        let lib = dir
            .path()
            .join("crates")
            .join(crate_name)
            .join("src/lib.rs");
        fs::create_dir_all(lib.parent().expect("parent")).expect("mkdir");
        fs::write(&lib, body).expect("write lib");
    }
    check_curated_pub_mods(dir.path()).expect("curated pilot shapes ok");
}

#[test]
fn real_tree_curated_pub_mods_green() {
    // Workspace root is two levels up from this crate when run via cargo test.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    check_curated_pub_mods(&root).expect("real tree curated surfaces green");
}

#[test]
fn suite_time_parses_fixture_junit_ms() {
    use super::measure_suite_time;
    let dir = tempfile::tempdir().expect("tempdir");
    let junit_dir = dir.path().join("target/nextest/ci");
    fs::create_dir_all(&junit_dir).expect("mkdir");
    // Two time attributes: 1.5s + 2s → 3500ms sum (attributes on suite + case).
    fs::write(
        junit_dir.join("junit.xml"),
        r#"<?xml version="1.0"?><testsuites time="1.5"><testsuite time="2.0"></testsuite></testsuites>"#,
    )
    .expect("write junit");
    let measured = measure_suite_time(dir.path()).expect("measure");
    assert_eq!(measured.get("junit_total_ms").copied(), Some(3500));
}

#[test]
fn suite_time_absent_junit_is_empty() {
    use super::measure_suite_time;
    let dir = tempfile::tempdir().expect("tempdir");
    let measured = measure_suite_time(dir.path()).expect("measure");
    assert!(measured.is_empty());
}

#[test]
fn function_complexity_reports_per_crate_max_and_ignores_tests() {
    use super::measure_rust_function_complexity;
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("crates/example/src");
    fs::create_dir_all(&src).expect("mkdir");
    fs::write(
        src.join("lib.rs"),
        "fn small(v: bool) { if v {} }\nfn larger(v: bool) { if v {} else if !v {} }\n",
    )
    .expect("write source");
    fs::write(
        src.join("tests.rs"),
        "fn ignored() { if true {} else if false {} else if true {} }\n",
    )
    .expect("write tests");

    let measured = measure_rust_function_complexity(dir.path()).expect("measure");
    assert_eq!(measured.get("example"), Some(&2));
    assert_eq!(measured.len(), 1);
}
