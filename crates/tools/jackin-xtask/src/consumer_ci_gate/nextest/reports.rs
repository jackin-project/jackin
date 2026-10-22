// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail, ensure};
use quick_xml::{
    XmlVersion,
    events::{BytesStart, Event},
    reader::Reader,
};
use serde_json::Value;

pub(super) const MAX_EVIDENCE_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn require_exact(
    label: &str,
    actual: &BTreeSet<String>,
    expected: &BTreeSet<String>,
) -> Result<()> {
    ensure!(!actual.is_empty(), "{label} is empty");
    ensure!(
        actual == expected,
        "{label} differs from required tests; missing={:?}; unexpected={:?}",
        expected.difference(actual).collect::<Vec<_>>(),
        actual.difference(expected).collect::<Vec<_>>()
    );
    Ok(())
}

pub(super) fn parse_inventory(bytes: &[u8], binary: &str) -> Result<BTreeSet<String>> {
    ensure!(
        bytes.len() <= MAX_EVIDENCE_BYTES,
        "nextest inventory exceeds 16 MiB bound"
    );
    let inventory: Value = serde_json::from_slice(bytes).context("parsing nextest list JSON")?;
    let suites = inventory
        .get("rust-suites")
        .and_then(Value::as_object)
        .context("nextest list has no rust-suites object")?;
    let mut selected = BTreeSet::new();
    let mut found = false;
    for suite in suites.values() {
        let name = suite
            .get("binary-name")
            .and_then(Value::as_str)
            .context("nextest suite has no binary-name")?;
        if name != binary {
            continue;
        }
        ensure!(!found, "nextest listed binary {binary} more than once");
        found = true;
        ensure!(
            suite.get("status").and_then(Value::as_str) == Some("listed"),
            "nextest did not list binary {binary}"
        );
        let tests = suite
            .get("testcases")
            .and_then(Value::as_object)
            .context("nextest suite has no testcases object")?;
        for (name, test) in tests {
            let status = test
                .get("filter-match")
                .and_then(|value| value.get("status"))
                .and_then(Value::as_str)
                .context("nextest test has no filter-match status")?;
            match status {
                "mismatch" => continue,
                "matches" => {}
                _ => bail!("unknown nextest filter status {status}"),
            }
            ensure!(
                test.get("ignored").and_then(Value::as_bool) == Some(false),
                "selected nextest test {name} is ignored or missing ignored metadata"
            );
            ensure!(
                !name.is_empty() && selected.insert(name.clone()),
                "invalid or duplicate selected test {name}"
            );
        }
    }
    ensure!(found, "nextest did not enumerate required binary {binary}");
    ensure!(
        !selected.is_empty(),
        "nextest selected no tests for {binary}"
    );
    Ok(selected)
}

type SuiteCount = Option<(u64, u64)>;
type ElementStack = Vec<(String, SuiteCount)>;

fn parse_attributes(element: &BytesStart<'_>, tag: &str) -> Result<(Option<String>, SuiteCount)> {
    let mut name = None;
    let mut counts = None;
    for attribute in element.attributes() {
        let attribute = attribute.context("parsing consumer JUnit attribute")?;
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .context("decoding consumer JUnit attribute")?;
        match attribute.key.as_ref() {
            "name" if tag == "testcase" => name = Some(value.into_owned()),
            "tests" if matches!(tag, "testsuite" | "testsuites") => {
                counts = Some((value.parse::<u64>()?, 0));
            }
            "failures" | "errors" | "skipped" | "disabled" | "retries" | "reruns" | "retry"
            | "rerun" => ensure!(
                value.parse::<u64>()? == 0,
                "JUnit contains nonzero failure, skip, or retry count"
            ),
            "flaky" => ensure!(
                matches!(value.as_ref(), "false" | "0"),
                "JUnit contains flaky test"
            ),
            "status" if tag == "testcase" => ensure!(
                matches!(value.as_ref(), "passed" | "run"),
                "JUnit testcase is not passed"
            ),
            "attempts" => ensure!(
                value.parse::<u64>()? == 1,
                "JUnit testcase has retry attempts"
            ),
            _ => {}
        }
    }
    Ok((name, counts))
}

fn count_testcase(stack: &mut ElementStack) {
    for (_, actual) in stack.iter_mut().filter_map(|(_, counts)| counts.as_mut()) {
        *actual += 1;
    }
}

fn open_element(
    element: &BytesStart<'_>,
    empty: bool,
    stack: &mut ElementStack,
    cases: &mut BTreeSet<String>,
    root_seen: &mut bool,
) -> Result<()> {
    let tag = element.name();
    let tag = tag.as_ref();
    ensure!(stack.len() < 64, "JUnit exceeds nesting bound");
    if stack.is_empty() {
        ensure!(
            !*root_seen && matches!(tag, "testsuites" | "testsuite"),
            "invalid JUnit root"
        );
        *root_seen = true;
    } else if tag == "testsuites" {
        bail!("nested JUnit testsuites element");
    } else if tag == "testsuite" {
        ensure!(
            stack
                .last()
                .is_some_and(|(parent, _)| parent == "testsuites"),
            "JUnit testsuite outside testsuites"
        );
    }
    ensure!(
        !matches!(
            tag,
            "failure"
                | "error"
                | "skipped"
                | "rerunFailure"
                | "rerunError"
                | "flakyFailure"
                | "flakyError"
                | "rerun"
                | "flaky"
                | "retry"
        ),
        "JUnit contains failure, skip, or retry evidence"
    );
    if tag == "testcase" {
        ensure!(
            stack
                .last()
                .is_some_and(|(parent, _)| parent == "testsuite"),
            "JUnit testcase outside testsuite"
        );
    }
    let (name, counts) = parse_attributes(element, tag)?;
    if tag == "testcase" {
        let name = name.context("JUnit testcase lacks name")?;
        ensure!(
            !name.is_empty() && cases.insert(name.clone()),
            "empty or duplicate JUnit testcase {name}"
        );
        count_testcase(stack);
    }
    if empty {
        if let Some((expected, actual)) = counts {
            ensure!(
                expected == actual,
                "JUnit declared test count differs from testcase count"
            );
        }
    } else {
        stack.push((tag.to_owned(), counts));
    }
    Ok(())
}

fn close_element(element: &str, stack: &mut ElementStack) -> Result<()> {
    let (tag, counts) = stack.pop().context("unmatched JUnit closing element")?;
    ensure!(tag == element, "unbalanced JUnit XML");
    if let Some((expected, actual)) = counts {
        ensure!(
            expected == actual,
            "JUnit declared test count differs from testcase count"
        );
    }
    Ok(())
}

pub(super) fn parse_junit(bytes: &[u8]) -> Result<BTreeSet<String>> {
    ensure!(
        bytes.len() <= MAX_EVIDENCE_BYTES,
        "JUnit report exceeds 16 MiB bound"
    );
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_comments = true;
    let mut buffer = Vec::new();
    let mut stack = ElementStack::new();
    let mut cases = BTreeSet::new();
    let mut root_seen = false;
    let mut declaration_seen = false;
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .context("parsing consumer JUnit XML")?;
        match event {
            Event::Start(element) => {
                open_element(&element, false, &mut stack, &mut cases, &mut root_seen)?;
            }
            Event::Empty(element) => {
                open_element(&element, true, &mut stack, &mut cases, &mut root_seen)?;
            }
            Event::End(element) => close_element(element.name().as_ref(), &mut stack)?,
            Event::DocType(_) => bail!("JUnit DTD is forbidden"),
            Event::CData(_) | Event::GeneralRef(_) if stack.is_empty() => {
                bail!("content outside JUnit root")
            }
            Event::Decl(_) => {
                ensure!(
                    !root_seen && stack.is_empty() && !declaration_seen,
                    "invalid or repeated XML declaration"
                );
                declaration_seen = true;
            }
            Event::Comment(_) | Event::PI(_) if stack.is_empty() => {
                bail!("content outside JUnit root")
            }
            Event::Text(text) if stack.is_empty() => ensure!(
                text.as_ref().chars().all(char::is_whitespace),
                "content outside JUnit root"
            ),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    ensure!(
        root_seen && stack.is_empty() && !cases.is_empty(),
        "JUnit report is empty or incomplete"
    );
    Ok(cases)
}
