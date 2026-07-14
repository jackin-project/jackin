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
    let lib = dir.path().join("crates/jackin-env/src/lib.rs");
    fs::create_dir_all(lib.parent().expect("parent")).expect("mkdir");
    fs::write(&lib, "mod env_layer;\npub mod test_support;\n").expect("write lib");
    check_curated_pub_mods(dir.path()).expect("env pilot shape ok");
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
