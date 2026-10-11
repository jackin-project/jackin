// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn broker_is_sibling_of_the_desktop_executable() {
    assert_eq!(
        broker_path(std::path::Path::new("JackinDesktop.app")),
        std::path::Path::new("JackinDesktop.app/Contents/MacOS/jackin-usage-broker")
    );
}

#[test]
fn broker_requires_exact_native_architecture() {
    assert_native_broker_archs("arm64\n").unwrap();
    for wrong in ["", "x86_64", "arm64 x86_64", "arm64 arm64", "arm64e"] {
        assert!(assert_native_broker_archs(wrong).is_err(), "{wrong}");
    }
}

#[test]
fn broker_version_probe_requires_matching_binary_and_release() {
    assert_broker_version("jackin-usage-broker 0.6.0\n", "0.6.0").unwrap();
    for wrong in [
        "jackin-usage-broker 0.5.0",
        "jackin 0.6.0",
        "jackin-usage-broker 0.6.0-dev",
        "jackin-usage-broker 0.6.0\nstarting daemon",
        "",
    ] {
        assert!(assert_broker_version(wrong, "0.6.0").is_err(), "{wrong}");
    }
}

#[cfg(unix)]
#[test]
fn broker_requires_regular_executable_file() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile_dir("jackin-desktop-broker-file-test").unwrap();
    let broker = temp.join("broker");
    assert!(assert_executable_file(&broker).is_err());
    assert!(assert_executable_file(&temp).is_err());
    std::fs::write(&broker, b"broker").unwrap();
    std::fs::set_permissions(&broker, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(assert_executable_file(&broker).is_err());
    std::fs::set_permissions(&broker, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_executable_file(&broker).unwrap();
    let link = temp.join("link");
    symlink(&broker, &link).unwrap();
    assert!(assert_executable_file(&link).is_err());
    std::fs::remove_dir_all(&temp).unwrap();
}

#[test]
fn desktop_build_packages_native_broker_before_bundle_signing() {
    let source = include_str!("../../desktop.rs");
    let build = source.split("fn build_app(").nth(1).unwrap();
    let build = build.split("pub(super) fn verify_app(").next().unwrap();
    assert_subsequence(
        build,
        &[
            ".env(\"JACKIN_VERSION_OVERRIDE\", version)",
            "DESKTOP_PROFILE,",
            "HOST_TARGET,",
            "\"jackin\",",
            "\"--bin\",",
            "BROKER_EXECUTABLE,",
            ".arg(\"--target-dir\")",
            ".arg(root.join(\"target\"))",
            "verify_broker(&built_broker, version)?;",
            "fs::copy(&built_broker, &broker)",
            "verify_broker(&broker, version)?;",
            "sign_broker(&dist, \"-\", false)?;",
            "dist.to_str().context(\"dist utf-8\")?",
        ],
        "native broker assembly",
    );
}

#[test]
fn developer_id_signs_and_checks_broker_before_notarization() {
    assert_subsequence(
        include_str!("../sign_notarize.rs"),
        &[
            "verify_app(&app, None, &version, &build, false)?;",
            "sign_broker(&app, &identity, true)?;",
            "app.to_str().context(\"app utf-8\")?",
            "check_expected_cert(&broker)?;",
            "check_expected_team(&broker)?;",
            "reject_get_task_allow(&broker)?;",
            "run_notarytool(&submit_zip, &notary_json)?;",
        ],
        "nested broker signing",
    );
}

#[test]
fn xcframework_pack_generates_bindings_and_headers_for_clean_builds() {
    // The pack command must use Boltffi's default generation path: the clean
    // build needs generated headers before XCFramework assembly.
    assert!(include_str!("../../desktop.rs").contains(".args([\"pack\", \"apple\"]);"));
}

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
fn xunit_accepts_xml_whitespace_comments_and_processing_instructions() {
    let source = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
        "<!-- prolog comment --><?xml-stylesheet href=\"report.xsl\"?>\n",
        "<testsuites>\n",
        "  <!-- suite list -->\n",
        "  <testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\">",
        "<?inside report?>before &amp; after &#x9;<![CDATA[raw text]]>",
        "</testsuite>\n",
        "</testsuites>\r\n<!-- epilog comment --><?tail report?>\n"
    );
    assert_eq!(
        parse_xunit_totals(source).unwrap(),
        XunitTotals {
            tests: 1,
            failures: 0,
            errors: 0,
        }
    );
}

#[test]
fn xunit_rejects_outside_root_content_and_misplaced_prolog_markup() {
    let report = "<testsuites><testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\"/></testsuites>";
    let invalid = [
        format!("prefix{report}"),
        format!("{report}suffix"),
        format!("<![CDATA[outside]]>{report}"),
        format!("{report}<![CDATA[outside]]>"),
        format!("&#32;{report}"),
        format!("{report}&#32;"),
        format!("<!DOCTYPE testsuites>{report}"),
        format!("{report}<!DOCTYPE testsuites>"),
        format!("{report}<?xml version=\"1.0\"?>"),
        format!("{report}{report}"),
        format!("<?xml version=\"1.0\"?> <?xml version=\"1.0\"?>{report}"),
        format!("<!-- too early --><?xml version=\"1.0\"?>{report}"),
        format!("<?XML version=\"1.0\"?>{report}"),
        format!("<?xml encoding=\"UTF-8\" version=\"1.0\"?>{report}"),
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" encoding=\"UTF-8\"?>{report}"),
        format!("<?xml version=\"1.0\" standalone=\"maybe\"?>{report}"),
        format!("<?xml version=\"1.0\" extra=\"x\"?>{report}"),
        format!("\u{000b}{report}"),
        "<testsuites><unexpected/></testsuites>".to_owned(),
        "<testsuites><testsuite name=\"outer\" tests=\"1\" failures=\"0\" errors=\"0\"><testsuite name=\"inner\" tests=\"1\" failures=\"0\" errors=\"0\"/></testsuite></testsuites>".to_owned(),
        "<testsuites><testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\"><1bad/></testsuite></testsuites>".to_owned(),
        "<testsuites><testsuite name=\"unit\" tests=\"1\" tests=\"1\" failures=\"0\" errors=\"0\"/></testsuites>".to_owned(),
        "<testsuites><testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\" notes=\"&#x1;\"/></testsuites>".to_owned(),
        "<testsuites><testsuite name=\"&custom;\" tests=\"1\" failures=\"0\" errors=\"0\"/></testsuites>".to_owned(),
        "<testsuites><testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\">&custom;</testsuite></testsuites>".to_owned(),
        "<testsuites><testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\">&#x1;</testsuite></testsuites>".to_owned(),
        "<!-- invalid -- comment --><testsuites><testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\"/></testsuites>".to_owned(),
        "<?xml version=\"1.1\"?><testsuites><testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\"/></testsuites>".to_owned(),
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?><testsuites><testsuite name=\"unit\" tests=\"1\" failures=\"0\" errors=\"0\"/></testsuites>".to_owned(),
    ];
    for source in invalid {
        assert!(
            parse_xunit_totals(&source).is_err(),
            "accepted malformed XML: {source:?}"
        );
    }
}

#[test]
fn parallel_xctest_xunit_counts_every_worker_suite() {
    let source = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<testsuites tests=\"5\" failures=\"0\" errors=\"0\">\n",
        "  <testsuite name=\"worker-1\" tests=\"2\" failures=\"0\" errors=\"0\"/>\n",
        "  <testsuite name=\"worker-2\" tests=\"3\" failures=\"0\" errors=\"0\"/>\n",
        "</testsuites>\n"
    );
    let totals = parse_xunit_totals(source).unwrap();
    assert_eq!(
        totals,
        XunitTotals {
            tests: 5,
            failures: 0,
            errors: 0,
        }
    );
    validate_test_totals("XCTest", &totals).unwrap();
}

#[test]
fn xunit_totals_reject_corrupt_or_incomplete_reports() {
    parse_xunit_totals("").unwrap_err();
    parse_xunit_totals("<testsuites></testsuites>").unwrap_err();
    parse_xunit_totals(
        "<testsuites><testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\">",
    )
    .unwrap_err();
    parse_xunit_totals("<testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\"")
        .unwrap_err();
    parse_xunit_totals("<testsuites><testsuite name=\"a\" tests=\"1\" errors=\"0\"/></testsuites>")
        .unwrap_err();
    parse_xunit_totals(
        "<testsuites><testsuite name=\"a\" tests=\"many\" failures=\"0\" errors=\"0\"/></testsuites>",
    )
        .unwrap_err();
    parse_xunit_totals(
        "<testsuites><testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\"></testsuites>",
    )
    .unwrap_err();
    parse_xunit_totals(
        "<testsuites><testsuite name=\"worker\" tests=\"7\" failures=\"0\" errors=\"0\"/>",
    )
    .unwrap_err();
    parse_xunit_totals(concat!(
        "<testsuites><testsuite name=\"a\" tests=\"1\" failures=\"0\" errors=\"0\"/>",
        "</testsuites><testsuites><testsuite name=\"b\" tests=\"1\" failures=\"0\" errors=\"0\"/>",
        "</testsuites>"
    ))
    .unwrap_err();
    parse_xunit_totals("<testsuite name=\"worker\" tests=\"7\" failures=\"0\" errors=\"0\"/>")
        .unwrap_err();
}

#[test]
fn xunit_counts_failures_and_errors_and_rejects_zero_tests() {
    let failures = parse_xunit_totals(
        "<testsuites><testsuite name=\"xctest\" tests=\"3\" failures=\"1\" errors=\"0\"/></testsuites>",
    )
    .unwrap();
    assert!(validate_test_totals("XCTest", &failures).is_err());

    let errors = parse_xunit_totals(
        "<testsuites><testsuite name=\"swift-testing\" tests=\"2\" failures=\"0\" errors=\"1\"/></testsuites>",
    )
    .unwrap();
    assert!(validate_test_totals("Swift Testing", &errors).is_err());

    let empty = parse_xunit_totals(
        "<testsuites><testsuite name=\"empty\" tests=\"0\" failures=\"0\" errors=\"0\"/></testsuites>",
    )
    .unwrap();
    assert!(validate_test_totals("Swift Testing", &empty).is_err());
}

#[test]
fn report_reader_fails_closed_on_missing_or_invalid_framework_reports() {
    let temp = tempfile::tempdir().unwrap();
    let xctest = temp.path().join("swift-unit-tests.xml");
    assert!(
        read_xunit_totals(&xctest, "XCTest")
            .unwrap_err()
            .to_string()
            .contains("missing XCTest xUnit report")
    );

    std::fs::write(&xctest, "<testsuites><testsuite").unwrap();
    let error = read_xunit_totals(&xctest, "XCTest").unwrap_err();
    assert!(format!("{error:#}").contains("invalid XCTest xUnit report"));
}

#[test]
fn swift_testing_xunit_is_a_separate_required_counted_report() {
    let report = parse_xunit_totals(
        "<testsuites><testsuite name=\"ProjectBaselineTests\" tests=\"2\" failures=\"0\" errors=\"0\"/></testsuites>",
    )
    .unwrap();
    assert_eq!(report.tests, 2);
    validate_test_totals("Swift Testing", &report).unwrap();
}
