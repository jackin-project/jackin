// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use anyhow::Result;
use std::{ffi::OsStr, fs, os::unix::fs::PermissionsExt};

fn names(values: &[&str]) -> Names {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn report_repository() -> Result<(tempfile::TempDir, RepositoryDirectory)> {
    let temp = tempfile::tempdir()?;
    fs::create_dir_all(temp.path().join("native"))?;
    let repository = RepositoryDirectory::open(temp.path())?;
    Ok((temp, repository))
}

#[test]
fn native_unit_report_parent_replacement_rejects_stale_valid_xml() -> Result<()> {
    let (temp, repository) = report_repository()?;
    let relative = Path::new("native/.build");
    let reports = repository.open_or_create_directory(relative)?;
    let stale_xml = r#"<testsuite tests="1" failures="0" errors="0"><testcase classname="Bridge" name="real()"/></testsuite>"#;
    fs::rename(
        temp.path().join("native/.build"),
        temp.path().join("native/.build-detached"),
    )?;
    fs::create_dir(temp.path().join("native/.build"))?;
    fs::write(
        temp.path().join("native/.build/swift-unit-tests.xml"),
        stale_xml,
    )?;

    reports
        .read_file(
            &repository,
            OsStr::new("swift-unit-tests.xml"),
            MAX_REPORT_BYTES,
            true,
            "native report",
        )
        .unwrap_err();
    Ok(())
}

#[test]
fn native_ui_report_ancestor_replacement_rejects_stale_valid_xml() -> Result<()> {
    let (temp, repository) = report_repository()?;
    let reports = repository.open_or_create_directory(Path::new("native/.build/test-results"))?;
    let report_directory =
        repository.open_or_create_directory(Path::new("native/.build/test-results/ui-run"))?;
    let relative_report = report_directory.relative().join("testReal.xml");
    let stale_xml = r#"<testsuite tests="1" failures="0" errors="0"><testcase classname="UIExample" name="testReal()"/></testsuite>"#;
    fs::write(temp.path().join(&relative_report), stale_xml)?;
    let expected = names(&["UIExample.testReal"]);
    verify_ui_reports(&repository, &report_directory, &expected)?;

    fs::rename(
        temp.path().join("native/.build/test-results"),
        temp.path().join("native/.build/test-results-detached"),
    )?;
    fs::create_dir(temp.path().join("native/.build/test-results"))?;
    fs::set_permissions(
        temp.path().join("native/.build/test-results"),
        fs::Permissions::from_mode(0o700),
    )?;
    fs::create_dir(temp.path().join("native/.build/test-results/ui-run"))?;
    fs::write(
        temp.path()
            .join("native/.build/test-results/ui-run/testReal.xml"),
        stale_xml,
    )?;

    assert!(reports.verify(&repository).is_err());
    verify_ui_reports(&repository, &report_directory, &expected).unwrap_err();
    Ok(())
}

#[test]
fn native_ui_report_directory_replacement_rejects_stale_valid_xml() -> Result<()> {
    let (temp, repository) = report_repository()?;
    let report_root =
        repository.open_or_create_directory(Path::new("native/.build/test-results"))?;
    let report_directory =
        repository.open_or_create_directory(Path::new("native/.build/test-results/ui-run"))?;
    let relative_report = report_directory.relative().join("testReal.xml");
    let stale_xml = r#"<testsuite tests="1" failures="0" errors="0"><testcase classname="UIExample" name="testReal()"/></testsuite>"#;
    fs::write(temp.path().join(&relative_report), stale_xml)?;
    let expected = names(&["UIExample.testReal"]);
    verify_ui_reports(&repository, &report_directory, &expected)?;

    let visible = temp.path().join(report_directory.relative());
    fs::rename(&visible, visible.with_file_name("ui-run-detached"))?;
    fs::create_dir(&visible)?;
    fs::set_permissions(&visible, fs::Permissions::from_mode(0o700))?;
    fs::write(visible.join("testReal.xml"), stale_xml)?;

    report_root.verify(&repository)?;
    verify_ui_reports(&repository, &report_directory, &expected).unwrap_err();
    Ok(())
}

#[test]
fn source_inventory_excludes_comments_strings_and_nested_helpers() {
    let source = r##"
        // final class Fake: XCTestCase { func testFake() {} }
        /* /* nested */ @Test func fake() {} */
        final class Actual: XCTestCase {
            let fixture = #"func testFake() { }"#
            let multiline = """
                final class Wrong: XCTestCase { func testWrong() {} }
                """
            func testReal() async throws {
                func testLocalHelper() {}
                let fixture = "@Test func wrong() {}"
            }
            private func helper() {}
        }
        @Suite("Example") struct Modern {
            @Test("Display label") @MainActor func real() throws {}
        }
    "##;
    let inventory = source_inventory(source).expect("valid Swift source");
    assert_eq!(inventory.xctest, names(&["Actual.testReal"]));
    assert_eq!(inventory.testing, names(&["Modern.real"]));
}

#[test]
fn source_inventory_resolves_xctest_extensions_across_files() {
    let sources = vec![
        "final class Legacy: XCTestCase { }".to_owned(),
        "extension Legacy { func testFromExtension() {} }".to_owned(),
        "extension NotATestCase { func testIgnored() {} }".to_owned(),
    ];
    let inventory = sources_inventory(&sources).expect("valid Swift source files");
    assert_eq!(inventory.xctest, names(&["Legacy.testFromExtension"]));
    assert!(inventory.testing.is_empty());
}

#[test]
fn unsupported_parameterized_tests_and_incomplete_sources_fail() {
    for source in [
        "struct Modern { @Test(arguments: [1]) func real(value: Int) {} }",
        "class Legacy: XCTestCase { func testReal(value: Int) {} }",
        "struct Modern { @Test }",
        "class Legacy: XCTestCase { func testReal() {}",
        "/* incomplete",
        "let fixture = \"incomplete",
    ] {
        assert!(source_inventory(source).is_err(), "accepted {source}");
    }
}

#[test]
fn junit_reconciles_names_and_nested_suite_counts() {
    let xml = r#"<?xml version="1.0"?>
        <testsuites><testsuite name="all" tests="2" failures="0" errors="0">
          <testsuite name="nested" tests="2" failures="0" errors="0" skipped="0">
            <testcase classname="Module.Modern" name="first()"/>
            <testcase classname="Modern" name="second()"><system-out>ok</system-out></testcase>
          </testsuite>
        </testsuite></testsuites>"#;
    assert_eq!(
        parse_junit(xml).expect("valid JUnit"),
        names(&["Modern.first", "Modern.second"])
    );
}

#[test]
fn junit_rejects_false_green_shapes() {
    let valid = r#"<testsuite tests="1" failures="0" errors="0"><testcase classname="Modern" name="real()"/></testsuite>"#;
    for xml in [
        valid.replace("tests=\"1\"", "tests=\"2\""),
        valid.replace("failures=\"0\"", "failures=\"1\""),
        valid.replace("errors=\"0\"", "errors=\"1\""),
        valid.replace("tests=\"1\"", "tests=\"1\" skipped=\"1\""),
        valid.replace("<testcase classname", "<testcase name=\"extra\" classname"),
        valid.replace("/></testsuite>", "><skipped/></testcase></testsuite>"),
        valid.replace("/></testsuite>", "><failure/></testcase></testsuite>"),
        valid.replace("/></testsuite>", "><error/></testcase></testsuite>"),
        valid.replace("</testsuite>", ""),
        format!("{valid}{valid}"),
        format!("<!DOCTYPE testsuite>{valid}"),
        format!("<testsuites failures=\"1\">{valid}</testsuites>"),
        format!("<testsuites tests=\"2\">{valid}</testsuites>"),
        valid.replace("<testcase classname", "<testcase status=\"notrun\" classname"),
        "<testsuite tests=\"0\" failures=\"0\" errors=\"0\"/>".to_owned(),
        "<testsuites><testcase classname=\"Modern\" name=\"real()\"/></testsuites>".to_owned(),
        "<testsuite tests=\"1\" errors=\"0\"><testcase classname=\"Modern\" name=\"real()\"/></testsuite>".to_owned(),
    ] {
        assert!(parse_junit(&xml).is_err(), "accepted {xml}");
    }
}

#[test]
fn junit_rejects_duplicate_cases_even_when_totals_match() {
    let xml = r#"<testsuite tests="2" failures="0" errors="0">
      <testcase classname="Module.Modern" name="real()"/>
      <testcase classname="Modern" name="real()"/>
    </testsuite>"#;
    parse_junit(xml).unwrap_err();
}

#[test]
fn junit_rejects_repeated_declarations_and_nested_testsuites() {
    let valid = r#"<testsuites><testsuite tests="1" failures="0" errors="0"><testcase classname="Modern" name="real()"/></testsuite></testsuites>"#;
    let declaration = r#"<?xml version="1.0"?>"#;
    parse_junit(&format!("{declaration}{valid}")).expect("one declaration before root");
    parse_junit(&format!("{declaration}{declaration}{valid}")).unwrap_err();
    let nested_root = format!("<testsuites>{valid}</testsuites>");
    parse_junit(&nested_root).unwrap_err();
    let nested_suite = valid
        .replace("<testcase ", "<testsuites><testcase ")
        .replace("/></testsuite>", "/></testsuites></testsuite>");
    parse_junit(&nested_suite).unwrap_err();
}

#[test]
fn junit_rejects_disabled_tests_at_every_report_level() {
    let valid = r#"<testsuites><testsuite tests="1" failures="0" errors="0"><testcase classname="Modern" name="real()"/></testsuite></testsuites>"#;
    parse_junit(valid).expect("valid executed test");
    for tag in ["testsuites", "testsuite", "testcase"] {
        let xml = if tag == "testsuites" {
            valid.replace("<testsuites>", "<testsuites disabled=\"1\">")
        } else {
            valid.replace(&format!("<{tag} "), &format!("<{tag} disabled=\"1\" "))
        };
        assert!(parse_junit(&xml).is_err(), "accepted disabled {tag}");
        parse_junit(&xml.replace("disabled=\"1\"", "disabled=\"0\""))
            .expect("zero disabled tests remain valid");
    }
}

#[test]
fn junit_rejects_retries_flakiness_and_invalid_execution_status() {
    let valid = r#"<testsuite tests="1" failures="0" errors="0"><testcase classname="Modern" name="real()"/></testsuite>"#;
    for tag in [
        "rerunFailure",
        "flakyFailure",
        "rerunError",
        "flakyError",
        "rerun",
        "flaky",
        "retry",
    ] {
        let xml = valid.replace(
            "/></testsuite>",
            &format!("><{tag}/></testcase></testsuite>"),
        );
        assert!(parse_junit(&xml).is_err(), "accepted {tag}");
    }
    for attribute in [
        "retries=\"1\"",
        "attempts=\"2\"",
        "attempts=\"0\"",
        "status=\"failed\"",
        "status=\"skipped\"",
        "flaky=\"true\"",
        "reruns=\"1\"",
    ] {
        let xml = valid.replace("<testcase ", &format!("<testcase {attribute} "));
        assert!(parse_junit(&xml).is_err(), "accepted {attribute}");
    }
    for xml in [
        format!("<![CDATA[outside]]>{valid}"),
        format!("{valid}<![CDATA[outside]]>"),
        format!("{valid}<?xml version=\"1.0\"?>"),
    ] {
        assert!(parse_junit(&xml).is_err(), "accepted {xml}");
    }
}

fn selected_toolchain() -> Toolchain {
    let developer = PathBuf::from("/Applications/Xcode.app/Contents/Developer");
    let digest = "a".repeat(64);
    Toolchain {
        host_version: "26.6".into(),
        host_build: "26G90".into(),
        architecture: "arm64".into(),
        developer: developer.clone(),
        xcodebuild_path: developer.join("usr/bin/xcodebuild"),
        xcodebuild_sha256: digest.clone(),
        swift_path: developer.join("Toolchains/XcodeDefault.xctoolchain/usr/bin/swift"),
        swift_sha256: digest.clone(),
        swift_format_path: developer
            .join("Toolchains/XcodeDefault.xctoolchain/usr/bin/swift-format"),
        swift_format_sha256: digest,
        sdk_path: developer.join("Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk"),
        xcode_version: "Xcode 27.1\nBuild version 27B20".into(),
        swift_version: "Apple Swift version 6.x".into(),
        swift_format_version: "swift-format version 600.0.0".into(),
        sdk_version: "27.1".into(),
        sdk_build: "26G90".into(),
        toolchains_override: None,
    }
}

#[test]
fn selected_toolchain_identity_accepts_current_versions_and_rejects_incoherent_paths() {
    validate_identity(&selected_toolchain()).expect("internally consistent selected tools");
    let mut candidate = selected_toolchain();
    candidate.developer = "/Library/Developer/CommandLineTools".into();
    assert!(validate_identity(&candidate).is_err());
    candidate = selected_toolchain();
    candidate.swift_sha256 = "not-a-digest".into();
    assert!(validate_identity(&candidate).is_err());
    candidate = selected_toolchain();
    candidate.sdk_version.clear();
    assert!(validate_identity(&candidate).is_err());
}

fn xctest_log() -> &'static str {
    "Test Suite 'All tests' started at now\n\
     Test Case '-[Module.Legacy testReal]' started.\n\
     Test Case '-[Module.Legacy testReal]' passed (0.001 seconds).\n\
     Test Suite 'Legacy' passed at now\n\
     Executed 1 test, with 0 failures (0 unexpected) in 0.001 seconds\n\
     Test Suite 'All tests' passed at now\n\
     Executed 1 test, with 0 failures (0 unexpected) in 0.001 seconds\n"
}

#[test]
fn xctest_requires_individual_passes_and_matching_final_summary() {
    verify_xctest(xctest_log(), &names(&["Legacy.testReal"])).expect("valid XCTest run");
    assert!(verify_xctest(xctest_log(), &names(&["Legacy.testOther"])).is_err());
    assert!(
        verify_xctest(
            &xctest_log().replace("passed (0.001 seconds)", "skipped (0.001 seconds)"),
            &names(&["Legacy.testReal"])
        )
        .is_err()
    );
    assert!(
        verify_xctest(
            &xctest_log().replace("with 0 failures", "with 1 failures"),
            &names(&["Legacy.testReal"])
        )
        .is_err()
    );
    assert!(
        verify_xctest(
            &xctest_log().replace("Executed 1 test", "Executed 0 tests"),
            &names(&["Legacy.testReal"])
        )
        .is_err()
    );
    assert!(verify_xctest("Test Suite 'All tests' passed at now\nExecuted 1 test, with 0 failures (0 unexpected)\n", &names(&["Legacy.testReal"])).is_err());
    assert!(
        verify_xctest(
            &format!("{}{}", xctest_log(), xctest_log()),
            &names(&["Legacy.testReal"])
        )
        .is_err()
    );
}

#[test]
fn identity_inventory_detects_equal_count_wrong_names() {
    assert!(
        verify_names(
            &names(&["Class.wrong"]),
            &names(&["Class.expected"]),
            "cases"
        )
        .is_err()
    );
}

#[test]
fn mandatory_named_contract_rejects_deletion_replacement_and_count_inflation() {
    for baseline in [required::XCTEST, required::SWIFT_TESTING, required::UI] {
        let mut current = names(baseline);
        required::require_baseline(&current, baseline, "baseline")
            .expect("complete named baseline");
        current.insert("Added.testNewScenario".to_owned());
        required::require_baseline(&current, baseline, "baseline").expect("additive cases allowed");
        let missing = baseline[0];
        current.remove(missing);
        assert!(
            required::require_baseline(&current, baseline, "baseline").is_err(),
            "same-count replacement hid removed {missing}"
        );
        for index in 0..100 {
            current.insert(format!("Inflated.test{index}"));
        }
        assert!(
            required::require_baseline(&current, baseline, "baseline").is_err(),
            "inflated counts hid removed {missing}"
        );
    }
}
