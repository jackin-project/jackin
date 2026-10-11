use super::*;
use std::fs;

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn fixture(budget_text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let crates = dir.path().join("crates");
    fs::create_dir_all(&crates).unwrap();
    // One production file under cap, one production file over cap, one test file.
    write(&crates.join("small.rs"), "fn tiny() {}\n");
    write(&crates.join("big.rs"), &"// padding line\n".repeat(1500));
    fs::create_dir_all(crates.join("pkg")).unwrap();
    write(&crates.join("pkg/tests.rs"), &"fn t() {}\n".repeat(4000));
    fs::write(dir.path().join("file-size-budget.toml"), budget_text).unwrap();
    dir
}

#[test]
fn passes_when_budgeted_file_matches_recorded_over_cap_count() {
    // big.rs is 1500 lines, over the 1000-line cap, recorded at exactly 1500
    // -> steady state, passes.
    let dir = fixture(
        "production_cap = 1000\ntest_cap = 10000\n\n[[production]]\npath = \"crates/big.rs\"\nlines = 1500\n",
    );
    let counts = measure_lines(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    check(dir.path(), &budget, &counts).unwrap();
}

#[test]
fn fails_when_unlisted_file_exceeds_cap() {
    let dir = fixture("production_cap = 1000\ntest_cap = 10000\n");
    let counts = measure_lines(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    let err = check(dir.path(), &budget, &counts).unwrap_err().to_string();
    assert!(err.contains("exceeds 1000-line cap"), "{err}");
    assert!(err.contains("big.rs"), "{err}");
}

#[test]
fn allowlist_entry_must_match_actual_count_down_only() {
    // Recorded 1000 lines but file is now 1500 -> ratchet exceeded (growth).
    let dir = fixture(
        "production_cap = 2000\ntest_cap = 10000\n\n[[production]]\npath = \"crates/big.rs\"\nlines = 1000\n",
    );
    let counts = measure_lines(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    let err = check(dir.path(), &budget, &counts).unwrap_err().to_string();
    assert!(err.contains("ratchet exceeded"), "{err}");
    assert!(err.contains("big.rs"), "{err}");
}

#[test]
fn rejects_budget_row_for_missing_file() {
    // The budget points at a file that does not exist on disk -> stale row.
    let dir = fixture(
        "production_cap = 1000\ntest_cap = 10000\n\n[[production]]\npath = \"crates/ghost.rs\"\nlines = 1500\n",
    );
    let counts = measure_lines(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    let err = check(dir.path(), &budget, &counts).unwrap_err().to_string();
    assert!(err.contains("crates/ghost.rs"), "{err}");
    assert!(
        err.contains("no longer exists") && err.contains("delete the stale budget row"),
        "{err}"
    );
}

#[test]
fn rejects_budget_row_when_file_drops_under_cap() {
    // big.rs is 1500 lines and under the 2000-line cap, yet it carries a
    // budget row -> the row is no longer needed and must be deleted.
    let dir = fixture(
        "production_cap = 2000\ntest_cap = 10000\n\n[[production]]\npath = \"crates/big.rs\"\nlines = 1500\n",
    );
    let counts = measure_lines(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    let err = check(dir.path(), &budget, &counts).unwrap_err().to_string();
    assert!(err.contains("crates/big.rs"), "{err}");
    assert!(
        err.contains("at or under the 2000-line cap")
            && err.contains("delete the stale budget row"),
        "{err}"
    );
}

#[test]
fn rejects_budget_row_when_recorded_count_higher_than_measured() {
    // big.rs is 1500 lines (over the 1000-line cap) but recorded at 9999 -> the
    // row must shrink to the measured count. Previously this passed silently.
    let dir = fixture(
        "production_cap = 1000\ntest_cap = 10000\n\n[[production]]\npath = \"crates/big.rs\"\nlines = 9999\n",
    );
    let counts = measure_lines(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    let err = check(dir.path(), &budget, &counts).unwrap_err().to_string();
    assert!(err.contains("crates/big.rs"), "{err}");
    assert!(err.contains("shrink the budget row to 1500"), "{err}");
}

#[test]
fn print_budget_only_emits_files_over_their_cap() {
    let dir = fixture("production_cap = 1000\ntest_cap = 10000\n");
    let counts = measure_lines(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    let report = budget_report(dir.path(), &counts, &budget);

    assert!(report.contains("path = \"crates/big.rs\""), "{report}");
    assert!(
        !report.contains(dir.path().to_string_lossy().as_ref()),
        "{report}"
    );
    assert!(!report.contains("crates/small.rs"), "{report}");
    assert!(!report.contains("crates/pkg/tests.rs"), "{report}");
}

#[test]
fn canonical_case_files_use_test_budget_without_matching_test_substrings_or_ancestors() {
    let dir = tempfile::tempdir().unwrap();
    let crates = dir.path().join("crates/pkg");
    write(&crates.join("src/tests.rs"), &"// test entry\n".repeat(5));
    write(
        &crates.join("src/tests/case_02.rs"),
        &"// test case\n".repeat(1500),
    );
    write(
        &crates.join("src/test_support/production.rs"),
        &"// production\n".repeat(1500),
    );

    let counts = measure_lines(dir.path()).unwrap();
    let budget = Budget {
        production_cap: 1000,
        test_cap: 2000,
        production: Vec::new(),
        test: Vec::new(),
    };
    let violations = collect_violations(dir.path(), &budget, &counts);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(
        violations[0].file,
        "crates/pkg/src/test_support/production.rs"
    );

    let test_budget = Budget {
        production_cap: 2000,
        test_cap: 1000,
        production: Vec::new(),
        test: Vec::new(),
    };
    let report = budget_report(dir.path(), &counts, &test_budget);
    let test_rows = report.split("[[test]]").nth(1).expect("test rows");
    assert!(
        test_rows.contains("crates/pkg/src/tests/case_02.rs"),
        "{report}"
    );
    assert!(
        !test_rows.contains("crates/pkg/src/test_support/production.rs"),
        "{report}"
    );
}

#[test]
fn test_source_classifier_requires_repository_relative_canonical_paths() {
    assert!(is_test_source_path(Path::new("crates/pkg/src/tests.rs")));
    assert!(is_test_source_path(Path::new(
        "crates/pkg/src/tests/case_01.rs"
    )));
    assert!(is_test_source_path(Path::new("crates/pkg/tests/main.rs")));
    assert!(!is_test_source_path(Path::new(
        "crates/pkg/src/test_support/case_01.rs"
    )));
    assert!(!is_test_source_path(Path::new("crates/pkg/src/lib.rs")));
    assert!(!is_test_source_path(Path::new(
        "/private/tmp/tests/repo/crates/pkg/src/lib.rs"
    )));
    assert!(!is_test_source_path(Path::new(
        "../tests/repo/crates/pkg/src/lib.rs"
    )));
}
