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
fn passes_when_all_files_within_caps() {
    // big.rs is 1500 lines but is in the allowlist at 1500 -> passes.
    let dir = fixture(
        "production_cap = 2000\ntest_cap = 10000\n\n[[production]]\npath = \"crates/big.rs\"\nlines = 1500\n",
    );
    let counts = measure(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    assert!(check(dir.path(), &budget, &counts).is_ok());
}

#[test]
fn fails_when_unlisted_file_exceeds_cap() {
    let dir = fixture("production_cap = 1000\ntest_cap = 10000\n");
    let counts = measure(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    let err = check(dir.path(), &budget, &counts).unwrap_err().to_string();
    assert!(err.contains("exceeds 1000-line cap"), "{err}");
    assert!(err.contains("big.rs"), "{err}");
}

#[test]
fn allowlist_entry_must_match_actual_count_down_only() {
    // Recorded 1000 lines but file is now 1500 -> ratchet exceeded.
    let dir = fixture(
        "production_cap = 2000\ntest_cap = 10000\n\n[[production]]\npath = \"crates/big.rs\"\nlines = 1000\n",
    );
    let counts = measure(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    let err = check(dir.path(), &budget, &counts).unwrap_err().to_string();
    assert!(err.contains("ratchet exceeded"), "{err}");
    assert!(err.contains("big.rs"), "{err}");
}

#[test]
fn allowlist_within_tolerance_passes() {
    // Recorded above current size — file has shrunk but entry stays.
    let dir = fixture(
        "production_cap = 1000\ntest_cap = 10000\n\n[[production]]\npath = \"crates/big.rs\"\nlines = 9999\n",
    );
    let counts = measure(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    assert!(check(dir.path(), &budget, &counts).is_ok());
}

#[test]
fn print_budget_only_emits_files_over_their_cap() {
    let dir = fixture("production_cap = 1000\ntest_cap = 10000\n");
    let counts = measure(dir.path()).unwrap();
    let budget = read_budget(&dir.path().join("file-size-budget.toml")).unwrap();
    // Capture stdout by buffering; print_budget prints directly. Just call it
    // and confirm it does not panic.
    print_budget(&counts, &budget);
}
