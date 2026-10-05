// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use serde_json::json;

fn inventory() -> Value {
    json!({"rust-suites": {
        "jackin::consumer": {"binary-name":"consumer", "status":"listed", "testcases": {
            "required": {"ignored":false, "filter-match":{"status":"matches"}},
            "unselected": {"ignored":false, "filter-match":{"status":"mismatch"}}
        }},
        "jackin::other": {"binary-name":"other", "status":"listed", "testcases": {
            "unrelated": {"ignored":false, "filter-match":{"status":"matches"}}
        }}
    }})
}

#[test]
fn inventory_uses_only_selected_cases_in_required_binary() {
    let selected = parse_inventory(&serde_json::to_vec(&inventory()).unwrap(), "consumer").unwrap();
    assert_eq!(selected, BTreeSet::from(["required".to_owned()]));
}

#[test]
fn inventory_rejects_missing_binary_ignored_unknown_and_zero_selection() {
    assert!(parse_inventory(&serde_json::to_vec(&inventory()).unwrap(), "absent").is_err());
    for replacement in [
        json!({"ignored":true,"filter-match":{"status":"matches"}}),
        json!({"ignored":false,"filter-match":{"status":"unknown"}}),
        json!({"ignored":false,"filter-match":{"status":"mismatch"}}),
        json!({"filter-match":{"status":"matches"}}),
    ] {
        let mut value = inventory();
        value["rust-suites"]["jackin::consumer"]["testcases"]["required"] = replacement;
        assert!(parse_inventory(&serde_json::to_vec(&value).unwrap(), "consumer").is_err());
    }
}

#[test]
fn exact_inventory_cannot_accept_arbitrary_discovered_cases() {
    let required = BTreeSet::from(["required".to_owned()]);
    assert!(require_exact("selection", &BTreeSet::new(), &required).is_err());
    assert!(
        require_exact(
            "selection",
            &BTreeSet::from(["arbitrary".to_owned()]),
            &required
        )
        .is_err()
    );
    assert!(
        require_exact(
            "selection",
            &BTreeSet::from(["required".to_owned(), "extra".to_owned()]),
            &required
        )
        .is_err()
    );
    assert!(require_exact("selection", &required, &required).is_ok());
}

#[test]
fn junit_accepts_successes_and_decodes_names() {
    let report = br#"<?xml version="1.0"?><testsuites><testsuite tests="2" failures="0" errors="0" skipped="0"><testcase name="a&amp;b"/><testcase name="required"><system-out>captured output</system-out></testcase></testsuite></testsuites>"#;
    assert_eq!(
        parse_junit(report).unwrap(),
        BTreeSet::from(["a&b".to_owned(), "required".to_owned()])
    );
}

#[test]
fn junit_rejects_failures_skips_retries_and_flakes() {
    for element in [
        "failure",
        "error",
        "skipped",
        "rerunFailure",
        "rerunError",
        "flakyFailure",
        "flakyError",
    ] {
        let report =
            format!("<testsuite><testcase name=\"required\"><{element}/></testcase></testsuite>");
        assert!(
            parse_junit(report.as_bytes()).is_err(),
            "accepted {element}"
        );
    }
    for attribute in [
        "failures=\"1\"",
        "errors=\"1\"",
        "skipped=\"1\"",
        "retries=\"1\"",
        "reruns=\"1\"",
        "attempts=\"2\"",
        "flaky=\"true\"",
        "status=\"skipped\"",
        "disabled=\"1\"",
    ] {
        let report = format!("<testsuite><testcase name=\"required\" {attribute}/></testsuite>");
        assert!(
            parse_junit(report.as_bytes()).is_err(),
            "accepted {attribute}"
        );
    }
}

#[test]
fn junit_checks_each_declared_suite_and_root_total() {
    for report in [
        "<testsuite tests=\"0\"><testcase name=\"required\"/></testsuite>",
        "<testsuites tests=\"0\"><testsuite tests=\"1\"><testcase name=\"required\"/></testsuite></testsuites>",
        "<testsuites tests=\"2\"><testsuite tests=\"0\"><testcase name=\"a\"/></testsuite><testsuite tests=\"2\"><testcase name=\"b\"/></testsuite></testsuites>",
        "<testsuites><testsuite tests=\"1\"/><testsuite tests=\"1\"><testcase name=\"required\"/></testsuite></testsuites>",
        "<testsuite disabled=\"1\"><testcase name=\"required\"/></testsuite>",
    ] {
        assert!(parse_junit(report.as_bytes()).is_err(), "accepted {report}");
    }
    let report = b"<testsuites tests=\"2\"><testsuite tests=\"1\"><testcase name=\"a\"/></testsuite><testsuite tests=\"1\"><testcase name=\"b\"/></testsuite></testsuites>";
    assert_eq!(parse_junit(report).unwrap().len(), 2);
}

#[test]
fn junit_rejects_content_and_declarations_outside_root() {
    let valid = "<testsuite><testcase name=\"required\"/></testsuite>";
    for content in [
        "<![CDATA[outside]]>",
        "&amp;",
        "<!--outside-->",
        "<?outside content?>",
    ] {
        assert!(parse_junit(format!("{content}{valid}").as_bytes()).is_err());
        assert!(parse_junit(format!("{valid}{content}").as_bytes()).is_err());
    }
    assert!(parse_junit(format!("{valid}<?xml version=\"1.0\"?>").as_bytes()).is_err());
    assert!(
        parse_junit(format!("<?xml version=\"1.0\"?><?xml version=\"1.0\"?>{valid}").as_bytes())
            .is_err()
    );
    assert!(
        parse_junit(b"<testsuite><?xml version=\"1.0\"?><testcase name=\"required\"/></testsuite>")
            .is_err()
    );
}

#[test]
fn oversized_inventory_and_junit_fail_before_parsing() {
    let oversized = vec![b' '; MAX_EVIDENCE_BYTES + 1];
    assert!(
        parse_inventory(&oversized, "consumer")
            .unwrap_err()
            .to_string()
            .contains("16 MiB")
    );
    assert!(
        parse_junit(&oversized)
            .unwrap_err()
            .to_string()
            .contains("16 MiB")
    );
}

#[test]
fn junit_rejects_duplicates_empty_missing_and_malformed_reports() {
    for report in [
        "",
        "<testsuite/>",
        "<testsuite><testcase/></testsuite>",
        "<testsuite><testcase name=\"\"/></testsuite>",
        "<testsuite><testcase name=\"required\"/><testcase name=\"required\"/></testsuite>",
        "<testsuite><testcase name=\"required\"/>",
        "<testsuite><testcase name=\"required\"/></testsuites>",
        "<testsuite><testcase name=\"required\"/></testsuite><testsuite/>",
        "<testcase name=\"required\"/>",
        "<!DOCTYPE testsuite><testsuite><testcase name=\"required\"/></testsuite>",
    ] {
        assert!(parse_junit(report.as_bytes()).is_err(), "accepted {report}");
    }
    let cases = parse_junit(b"<testsuite><testcase name=\"wrong\"/></testsuite>").unwrap();
    assert!(require_exact("JUnit", &cases, &BTreeSet::from(["required".to_owned()])).is_err());
}
