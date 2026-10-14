// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parses_junit_testcases_and_xml_entities() {
    let directory = tempfile::tempdir().unwrap();
    let report = directory.path().join("junit.xml");
    fs::write(
        &report,
        r#"<testsuites><testcase name="fast &amp; safe" time="0.25"/><testcase name="flaky" time="bad" flaky="true"/></testsuites>"#,
    )
    .unwrap();
    assert_eq!(
        parse_report(&report).unwrap(),
        vec![
            TestCase {
                name: "fast & safe".into(),
                seconds: 0.25,
            },
            TestCase {
                name: "flaky".into(),
                seconds: 0.0,
            },
        ]
    );
}
