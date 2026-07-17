use std::fs;

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
                flaky: false,
            },
            TestCase {
                name: "flaky".into(),
                seconds: 0.0,
                flaky: true,
            },
        ]
    );
}

#[test]
fn rejects_unquarantined_flakes() {
    let directory = tempfile::tempdir().unwrap();
    let quarantine = directory.path().join("flaky-tests.toml");
    fs::write(&quarantine, "[[test]]\nname = \"known\"\n").unwrap();
    let cases = vec![TestCase {
        name: "unknown".into(),
        seconds: 0.0,
        flaky: true,
    }];
    assert!(enforce_quarantine(&quarantine, &cases).is_err());
}
