// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn names(values: &[&str]) -> Names {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn source_inventory_excludes_comments_strings_and_nested_helpers() {
    let source = r###"
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
    "###;
    let inventory = source_inventory(source).expect("valid Swift source");
    assert_eq!(inventory.xctest, names(&["Actual.testReal"]));
    assert_eq!(inventory.testing, names(&["Modern.real"]));
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
    assert!(parse_junit(xml).is_err());
}

#[test]
fn junit_rejects_repeated_declarations_and_nested_testsuites() {
    let valid = r#"<testsuites><testsuite tests="1" failures="0" errors="0"><testcase classname="Modern" name="real()"/></testsuite></testsuites>"#;
    let declaration = r#"<?xml version="1.0"?>"#;
    parse_junit(&format!("{declaration}{valid}")).expect("one declaration before root");
    assert!(parse_junit(&format!("{declaration}{declaration}{valid}")).is_err());
    let nested_root = format!("<testsuites>{valid}</testsuites>");
    assert!(parse_junit(&nested_root).is_err());
    let nested_suite = valid
        .replace("<testcase ", "<testsuites><testcase ")
        .replace("/></testsuite>", "/></testsuites></testsuite>");
    assert!(parse_junit(&nested_suite).is_err());
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

fn shipping_toolchain() -> Toolchain {
    Toolchain {
        host_version: "27.0".into(), host_build: "26A428".into(), architecture: "arm64".into(),
        developer: "/Applications/Xcode.app/Contents/Developer".into(),
        swift_path: "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/swift".into(),
        sdk_path: "/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX27.0.sdk".into(),
        xcode: SHIPPING_XCODE.into(), swift: SHIPPING_SWIFT.into(), sdk_version: "27.0".into(), sdk_build: "26A425".into(),
        swift_driver: "swift-driver version: 1.168.6".into(),
        signature_requirement: APPLE_XCODE_REQUIREMENT.into(),
        stable_release_source: STABLE_RELEASE_SOURCE.into(),
    }
}

#[test]
fn shipping_tuple_excludes_unapproved_versions_builds_and_compilers() {
    validate_toolchain(&shipping_toolchain()).expect("exact stable shipping tuple");
    for xcode in [
        "",
        "Xcode 26.6\nBuild version 17F113",
        "Xcode 27.1\nBuild version 27A266a",
        "Xcode 27.0 beta\nBuild version 27A266a",
        "Xcode 27.0",
        "Xcode 27.0\nBuild version beta",
        "Xcode 27.0.1\nBuild version 27A266a",
    ] {
        let mut candidate = shipping_toolchain();
        candidate.xcode = xcode.into();
        assert!(validate_toolchain(&candidate).is_err(), "accepted {xcode}");
    }
    for swift in [
        "",
        "Apple Swift version 6.4",
        "Apple Swift version 6.4 (swiftlang-beta clang-beta)",
    ] {
        let mut candidate = shipping_toolchain();
        candidate.swift = swift.into();
        assert!(validate_toolchain(&candidate).is_err(), "accepted {swift}");
    }
    for (version, build) in [
        ("", "26A425"),
        ("27.1", "26A425"),
        ("27.0", ""),
        ("27.0", "beta"),
        ("27.0", "26A428"),
    ] {
        let mut candidate = shipping_toolchain();
        candidate.sdk_version = version.into();
        candidate.sdk_build = build.into();
        assert!(validate_toolchain(&candidate).is_err());
    }
}

#[test]
fn compiler_output_streams_and_global_selector_are_distinct() {
    let (swift, driver) = compiler_streams(
        format!("{SHIPPING_SWIFT}\n").into_bytes(),
        b"swift-driver version: 1.168.6 ".to_vec(),
    )
    .expect("UTF-8 compiler identity");
    assert_eq!(swift, SHIPPING_SWIFT);
    assert_eq!(driver, "swift-driver version: 1.168.6");
    assert!(compiler_streams(vec![255], vec![]).is_err());
    let selection = global_selection_command();
    assert!(
        selection
            .get_envs()
            .any(|(key, value)| key == "DEVELOPER_DIR" && value.is_none())
    );
}

#[test]
fn compiler_streams_and_stable_provenance_are_independently_required() {
    for driver in [
        "",
        "swift-driver version: 1.168.7",
        "swift-driver version: beta",
    ] {
        let mut candidate = shipping_toolchain();
        candidate.swift_driver = driver.into();
        assert!(validate_toolchain(&candidate).is_err());
    }
    let mut candidate = shipping_toolchain();
    candidate.swift = format!("{} {}", candidate.swift_driver, candidate.swift);
    assert!(
        validate_toolchain(&candidate).is_err(),
        "stderr must not be mixed into compiler stdout"
    );
    candidate = shipping_toolchain();
    candidate.signature_requirement.clear();
    assert!(validate_toolchain(&candidate).is_err());
    candidate = shipping_toolchain();
    candidate.stable_release_source.clear();
    assert!(validate_toolchain(&candidate).is_err());
}

#[test]
fn shipping_host_and_selected_paths_fail_closed() {
    for version in [
        "26.5",
        "26.6",
        "26.9",
        "27.1",
        "28.0",
        "27.0-beta",
        "27.0.bad",
        "",
    ] {
        let mut candidate = shipping_toolchain();
        candidate.host_version = version.into();
        assert!(
            validate_toolchain(&candidate).is_err(),
            "accepted host {version}"
        );
    }
    let mut candidate = shipping_toolchain();
    candidate.host_build.clear();
    assert!(validate_toolchain(&candidate).is_err());
    candidate = shipping_toolchain();
    candidate.architecture = "x86_64".into();
    assert!(validate_toolchain(&candidate).is_err());
    candidate = shipping_toolchain();
    candidate.sdk_path = "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk".into();
    assert!(validate_toolchain(&candidate).is_err());
    candidate = shipping_toolchain();
    candidate.swift_path = "/usr/local/bin/swift".into();
    assert!(validate_toolchain(&candidate).is_err());
    candidate = shipping_toolchain();
    candidate.developer = "/Library/Developer/CommandLineTools".into();
    assert!(validate_toolchain(&candidate).is_err());
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
