use super::{
    MIN_OS, XunitTotals, minos_matches_target, normalize_generated_text, parse_dwarf_uuid,
    parse_xctest_summary, parse_xunit_totals, tree_differences, validate_build, validate_version,
};

#[test]
fn version_accepts_dotted_numeric() {
    validate_version("0.6.0").unwrap();
    validate_version("1").unwrap();
    validate_version("10.20.30").unwrap();
}

#[test]
fn version_rejects_semver_prerelease_and_empty() {
    assert!(validate_version("").is_err());
    assert!(validate_version("0.6.0-dev").is_err());
    assert!(validate_version("v0.6.0").is_err());
    assert!(validate_version("0..1").is_err());
}

#[test]
fn build_accepts_numeric_only() {
    validate_build("1").unwrap();
    validate_build("42").unwrap();
    assert!(validate_build("").is_err());
    assert!(validate_build("1a").is_err());
}

#[test]
fn minos_must_match_current_baseline() {
    assert!(minos_matches_target("26.0", MIN_OS));
    assert!(minos_matches_target("26.0.0", MIN_OS));
    assert!(!minos_matches_target("25.0", MIN_OS));
    assert!(!minos_matches_target("26.1", MIN_OS));
    assert!(!minos_matches_target("27.0", MIN_OS));
}

#[test]
fn generated_bindings_have_stable_whitespace() {
    assert_eq!(
        normalize_generated_text("one  \n  two\t\n\n"),
        "one\n  two\n"
    );
    assert_eq!(normalize_generated_text("one"), "one\n");
    assert_eq!(normalize_generated_text(" \t\n"), "");
}

fn write_tree(root: &std::path::Path, files: &[(&str, &[u8])]) {
    for (relative, bytes) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

#[test]
fn tree_differences_clean_when_identical() {
    let temp = std::env::temp_dir().join(format!("jackin-bindings-clean-{}", std::process::id()));
    let expected = temp.join("expected");
    let actual = temp.join("actual");
    drop(std::fs::remove_dir_all(&temp));
    write_tree(&expected, &[("a.bin", b"one"), ("nested/b.bin", b"two")]);
    write_tree(&actual, &[("a.bin", b"one"), ("nested/b.bin", b"two")]);
    assert!(
        tree_differences(&expected, &actual, "label")
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(&temp).unwrap();
}

#[test]
fn tree_differences_flags_stale_missing_and_extra() {
    let temp = std::env::temp_dir().join(format!("jackin-bindings-drift-{}", std::process::id()));
    let expected = temp.join("expected");
    let actual = temp.join("actual");
    drop(std::fs::remove_dir_all(&temp));
    write_tree(
        &expected,
        &[("stale.bin", b"old"), ("missing.bin", b"gone")],
    );
    write_tree(&actual, &[("stale.bin", b"new"), ("extra.bin", b"added")]);
    let differences = tree_differences(&expected, &actual, "label").unwrap();
    assert_eq!(
        differences,
        vec![
            "label/missing.bin: missing after regeneration".to_owned(),
            "label/extra.bin: not committed".to_owned(),
            "label/stale.bin: content drift".to_owned(),
        ]
    );
    std::fs::remove_dir_all(&temp).unwrap();
}

#[test]
fn xunit_totals_sum_every_testsuite() {
    let source = concat!(
        "<?xml version=\"1.0\"?>\n",
        "<testsuites>\n",
        "<testsuite name=\"a\" tests=\"3\" failures=\"0\" errors=\"0\"></testsuite>\n",
        "<testsuite name=\"b\" tests=\"4\" failures=\"1\" errors=\"2\"></testsuite>\n",
        "</testsuites>\n"
    );
    assert_eq!(
        parse_xunit_totals(source).unwrap(),
        XunitTotals {
            tests: 7,
            failures: 1,
            errors: 2,
        }
    );
}

#[test]
fn xunit_totals_reject_corrupt_reports() {
    parse_xunit_totals("").unwrap_err();
    parse_xunit_totals("<testsuites></testsuites>").unwrap_err();
    parse_xunit_totals("<testsuite name=\"a\" tests=\"1\">").unwrap_err();
    parse_xunit_totals("<testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\"")
        .unwrap_err();
}

#[test]
fn xctest_summary_reads_last_all_tests_block() {
    let log = concat!(
        "Test Suite 'PlatformLaneTests' started\n",
        "\t Executed 3 tests, with 0 failures (0 unexpected) in 0.012 (0.013) seconds\n",
        "Test Suite 'All tests' passed at 2026-08-20 10:00:00.000\n",
        "\t Executed 71 tests, with 0 failures (0 unexpected) in 2.733 (2.738) seconds\n"
    );
    assert_eq!(
        parse_xctest_summary(log).unwrap(),
        XunitTotals {
            tests: 71,
            failures: 0,
            errors: 0,
        }
    );
}

#[test]
fn xctest_summary_rejects_missing_or_truncated_block() {
    parse_xctest_summary("nothing here").unwrap_err();
    // 'All tests' header without the following Executed line = crashed runner.
    parse_xctest_summary("Test Suite 'All tests' started\n").unwrap_err();
    // Executed line without parseable numbers is corruption, never zero.
    let log = concat!(
        "Test Suite 'All tests' passed\n",
        "\t Executed many tests, with no failures\n"
    );
    parse_xctest_summary(log).unwrap_err();
}

fn repo_text(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

fn task_block<'a>(mise: &'a str, name: &str) -> &'a str {
    let marker = format!("[tasks.{name}]\n");
    let start = mise
        .find(&marker)
        .unwrap_or_else(|| panic!("mise.toml missing {marker}"));
    let rest = &mise[start + marker.len()..];
    let end = rest.find("\n[").map_or(rest.len(), |index| index + 1);
    &rest[..end]
}

fn assert_subsequence(haystack: &str, needles: &[&str], label: &str) {
    let mut cursor = 0;
    for needle in needles {
        let found = haystack[cursor..]
            .find(needle)
            .unwrap_or_else(|| panic!("{label}: `{needle}` missing or out of order"));
        cursor += found + needle.len();
    }
}

#[test]
fn dwarf_uuid_reads_the_arm64_slice() {
    let output = "UUID: 11111111-2222-3333-4444-555555555555 (x86_64) /tmp/app\n\
                  UUID: AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE (arm64) /tmp/app\n";
    assert_eq!(
        parse_dwarf_uuid(output).as_deref(),
        Some("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE")
    );
    assert!(parse_dwarf_uuid("no uuid here").is_none());
    assert!(parse_dwarf_uuid("UUID: 1111 (x86_64) /tmp/app\n").is_none());
}

#[test]
fn cadence_tasks_define_the_canonical_graph() {
    let mise = repo_text("mise.toml");
    assert_subsequence(
        task_block(&mise, "desktop-ci"),
        &[
            "desktop-bindings-check",
            "desktop-generate",
            "desktop-format-check",
            "desktop-lint",
            "desktop-test\n",
            "desktop-build",
            "desktop test-swift",
            "desktop-verify",
        ],
        "desktop-ci",
    );
    assert_subsequence(
        task_block(&mise, "desktop-merge"),
        &["desktop-ci", "desktop-test-ui"],
        "desktop-merge",
    );
    assert_subsequence(
        task_block(&mise, "desktop-scheduled"),
        &["desktop-merge", "desktop-deadcode"],
        "desktop-scheduled",
    );
}

#[test]
fn release_workflow_invokes_canonical_mise_tasks() {
    let release = repo_text(".github/workflows/release.yml");
    for task in [
        "mise run desktop-build",
        "mise run desktop-verify",
        "mise run desktop-sign-notarize",
        "mise run desktop-release-state",
    ] {
        assert!(release.contains(task), "release.yml must invoke `{task}`");
    }
    for restated in [
        "cargo xtask desktop build",
        "cargo xtask desktop verify",
        "cargo xtask desktop sign-notarize",
        "cargo xtask desktop release-state",
    ] {
        assert!(
            !release.contains(restated),
            "release.yml must not restate `{restated}` beside the mise task"
        );
    }
}

#[test]
fn generated_ci_delegates_the_native_lane() {
    let ci = repo_text(".github/workflows/ci.yml");
    assert!(
        ci.contains("ci-native.yml"),
        "generated ci.yml must delegate the native lane to the reusable workflow"
    );
    for hand_restated in ["swift test", "cargo xtask desktop", "xcodebuild"] {
        assert!(
            !ci.contains(hand_restated),
            "generated ci.yml must not hand-restate native step `{hand_restated}`"
        );
    }
}
