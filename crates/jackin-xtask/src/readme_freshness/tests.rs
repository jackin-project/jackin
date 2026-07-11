use super::{NameStatusEntry, evaluate, is_structural_status, parse_name_status};

fn entry(status: &str, path: &str) -> NameStatusEntry {
    NameStatusEntry {
        status: status.to_owned(),
        path: path.to_owned(),
        new_path: None,
    }
}

fn rename(status: &str, old: &str, new: &str) -> NameStatusEntry {
    NameStatusEntry {
        status: status.to_owned(),
        path: old.to_owned(),
        new_path: Some(new.to_owned()),
    }
}

#[test]
fn structural_add_triggers() {
    let report = evaluate(
        &[entry("A", "crates/jackin-foo/src/new_mod.rs")],
        &[],
    );
    assert_eq!(report.violations.len(), 1);
    assert_eq!(report.violations[0].crate_name, "jackin-foo");
}

#[test]
fn content_modify_does_not_trigger() {
    let report = evaluate(
        &[entry("M", "crates/jackin-foo/src/existing.rs")],
        &[],
    );
    assert!(report.violations.is_empty());
    assert!(report.structural_crates.is_empty());
}

#[test]
fn rename_triggers() {
    let report = evaluate(
        &[rename(
            "R100",
            "crates/jackin-foo/src/old.rs",
            "crates/jackin-foo/src/new.rs",
        )],
        &[],
    );
    assert_eq!(report.violations.len(), 1);
}

#[test]
fn delete_triggers() {
    let report = evaluate(
        &[entry("D", "crates/jackin-foo/src/gone.rs")],
        &[],
    );
    assert_eq!(report.violations.len(), 1);
}

#[test]
fn readme_touch_clears() {
    let report = evaluate(
        &[
            entry("A", "crates/jackin-foo/src/new_mod.rs"),
            entry("M", "crates/jackin-foo/README.md"),
        ],
        &[],
    );
    assert!(report.violations.is_empty());
    assert!(report.structural_crates.contains("jackin-foo"));
    assert!(report.readme_touched.contains("jackin-foo"));
}

#[test]
fn tests_dir_outside_src_does_not_trigger() {
    let report = evaluate(
        &[entry("A", "crates/jackin-foo/tests/integration.rs")],
        &[],
    );
    assert!(report.violations.is_empty());
}

#[test]
fn allowlist_skips_crate() {
    let report = evaluate(
        &[entry("A", "crates/jackin-foo/src/new_mod.rs")],
        &["jackin-foo"],
    );
    assert!(report.violations.is_empty());
}

#[test]
fn parse_name_status_handles_rename_tabs() {
    let entries = parse_name_status("R095\tcrates/a/src/old.rs\tcrates/a/src/new.rs\n");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, "R095");
    assert_eq!(entries[0].new_path.as_deref(), Some("crates/a/src/new.rs"));
    assert!(is_structural_status("R095"));
    assert!(!is_structural_status("M"));
}
