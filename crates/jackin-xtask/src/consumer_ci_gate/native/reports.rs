// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{Names, verify_names};
use anyhow::{Context, Result, bail, ensure};
use quick_xml::{
    XmlVersion,
    events::{BytesStart, Event},
    reader::Reader,
};
use std::collections::BTreeMap;

fn identity(class: &str, name: &str) -> Result<String> {
    let class = class.rsplit('.').next().unwrap_or(class);
    let name = name.strip_suffix("()").unwrap_or(name);
    ensure!(
        !name.is_empty()
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
        "invalid native test identity {name}"
    );
    if class.is_empty() {
        Ok(name.to_owned())
    } else {
        ensure!(
            class
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
            "invalid native test class {class}"
        );
        Ok(format!("{class}.{name}"))
    }
}

pub(super) fn verify_xctest(log: &str, expected: &Names) -> Result<()> {
    let mut passed = Names::new();
    let mut summary_pending = false;
    let mut summary = None;
    for line in log.lines() {
        if line.starts_with("Test Case '") {
            if line.contains("' started.") {
                continue;
            }
            let rest = line
                .strip_prefix("Test Case '")
                .context("invalid XCTest event")?;
            let (name, outcome) = rest
                .split_once("' ")
                .context("invalid XCTest event identity")?;
            ensure!(
                outcome.starts_with("passed ("),
                "XCTest did not pass: {line}"
            );
            let name = if let Some(name) = name
                .strip_prefix("-[")
                .and_then(|name| name.strip_suffix(']'))
            {
                let (class, method) = name
                    .split_once(' ')
                    .context("invalid Objective-C XCTest identity")?;
                identity(class, method)?
            } else {
                let (class, method) = name.rsplit_once('.').context("invalid XCTest identity")?;
                identity(class, method)?
            };
            ensure!(
                passed.insert(name.clone()),
                "duplicate XCTest pass event {name}"
            );
        }
        if line.starts_with("Test Suite 'All tests'") {
            if line.contains(" started ") {
                continue;
            }
            ensure!(line.contains(" passed "), "XCTest All tests did not pass");
            summary_pending = true;
        } else if summary_pending && line.trim_start().starts_with("Executed ") {
            let words: Vec<_> = line.split_whitespace().collect();
            let count = words
                .get(1)
                .context("missing XCTest summary count")?
                .parse::<usize>()?;
            ensure!(
                line.contains("with 0 failures (0 unexpected)") && !line.contains("skipped"),
                "XCTest summary has failures or skips: {line}"
            );
            ensure!(
                summary.replace(count).is_none(),
                "duplicate All tests summary"
            );
            summary_pending = false;
        }
    }
    ensure!(
        summary == Some(expected.len()),
        "XCTest final summary count differs from source inventory"
    );
    verify_names(&passed, expected, "XCTest executed tests")
}

fn attributes(
    element: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for attribute in element.attributes() {
        let attribute = attribute?;
        let key = std::str::from_utf8(attribute.key.as_ref())?.to_owned();
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())?
            .into_owned();
        ensure!(
            values.insert(key, value).is_none(),
            "duplicate JUnit attribute"
        );
    }
    Ok(values)
}

pub(super) fn parse_junit(source: &str) -> Result<Names> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().check_comments = true;
    let mut stack: Vec<(String, Option<(usize, usize)>)> = Vec::new();
    let mut names = Names::new();
    let mut roots = 0;
    let mut suites = 0;
    let mut declaration_seen = false;
    loop {
        let event = reader.read_event().context("parsing native JUnit XML")?;
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                let tag = std::str::from_utf8(element.name().as_ref())?.to_owned();
                ensure!(stack.len() < 64, "JUnit exceeds nesting bound");
                ensure!(
                    tag != "testsuites" || stack.is_empty(),
                    "JUnit testsuites must be the unique root"
                );
                if stack.is_empty() {
                    roots += 1;
                    ensure!(
                        roots == 1 && matches!(tag.as_str(), "testsuites" | "testsuite"),
                        "invalid JUnit root"
                    );
                }
                ensure!(
                    !matches!(
                        tag.as_str(),
                        "failure"
                            | "error"
                            | "skipped"
                            | "rerunFailure"
                            | "flakyFailure"
                            | "rerunError"
                            | "flakyError"
                            | "rerun"
                            | "flaky"
                            | "retry"
                    ),
                    "native JUnit contains {tag}"
                );
                let attrs = attributes(element, &reader)?;
                for key in ["retries", "reruns", "retry", "rerun"] {
                    if let Some(value) = attrs.get(key) {
                        ensure!(value.parse::<usize>()? == 0, "JUnit contains {key}");
                    }
                }
                if let Some(attempts) = attrs.get("attempts") {
                    ensure!(
                        attempts.parse::<usize>()? == 1,
                        "JUnit testcase must execute once"
                    );
                }
                if let Some(flaky) = attrs.get("flaky") {
                    ensure!(
                        matches!(flaky.as_str(), "false" | "0"),
                        "JUnit contains a flaky testcase"
                    );
                }
                if let Some(status) = attrs.get("status") {
                    ensure!(
                        matches!(status.as_str(), "run" | "passed"),
                        "JUnit did not run successfully: {status}"
                    );
                }
                if matches!(tag.as_str(), "testsuites" | "testsuite" | "testcase") {
                    for key in ["failures", "errors", "skipped", "disabled"] {
                        if let Some(value) = attrs.get(key) {
                            ensure!(value.parse::<usize>()? == 0, "JUnit reports {key}");
                        }
                    }
                }
                let mut suite = None;
                if tag == "testsuite" {
                    suites += 1;
                    for key in ["failures", "errors"] {
                        ensure!(
                            attrs
                                .get(key)
                                .with_context(|| format!("JUnit testsuite missing {key}"))?
                                .parse::<usize>()?
                                == 0,
                            "JUnit reports {key}"
                        );
                    }
                    if let Some(skipped) = attrs.get("skipped") {
                        ensure!(
                            skipped.parse::<usize>()? == 0,
                            "JUnit reports skipped tests"
                        );
                    }
                    suite = Some((
                        attrs
                            .get("tests")
                            .context("JUnit testsuite missing tests")?
                            .parse()?,
                        0,
                    ));
                } else if tag == "testsuites" {
                    if let Some(tests) = attrs.get("tests") {
                        suite = Some((tests.parse()?, 0));
                    }
                } else if tag == "testcase" {
                    if let Some(status) = attrs.get("status") {
                        ensure!(
                            matches!(status.as_str(), "run" | "passed"),
                            "JUnit testcase did not run: {status}"
                        );
                    }
                    ensure!(
                        stack.last().is_some_and(|(tag, _)| tag == "testsuite"),
                        "JUnit testcase outside testsuite"
                    );
                    let name = identity(
                        attrs
                            .get("classname")
                            .context("JUnit testcase missing classname")?,
                        attrs.get("name").context("JUnit testcase missing name")?,
                    )?;
                    ensure!(
                        names.insert(name.clone()),
                        "duplicate JUnit testcase {name}"
                    );
                    for (_, suite) in &mut stack {
                        if let Some((_, count)) = suite {
                            *count += 1;
                        }
                    }
                }
                if matches!(event, Event::Empty(_)) {
                    if let Some((expected, actual)) = suite {
                        ensure!(
                            expected == actual,
                            "JUnit suite count differs from testcase count"
                        );
                    }
                } else {
                    stack.push((tag, suite));
                }
            }
            Event::End(element) => {
                let (tag, suite) = stack.pop().context("unmatched JUnit closing tag")?;
                ensure!(
                    tag.as_bytes() == element.name().as_ref(),
                    "mismatched JUnit closing tag"
                );
                if let Some((expected, actual)) = suite {
                    ensure!(
                        expected == actual,
                        "JUnit suite count differs from testcase count"
                    );
                }
            }
            Event::DocType(_) => bail!("JUnit document types are forbidden"),
            Event::CData(_) | Event::GeneralRef(_) if stack.is_empty() => {
                bail!("content outside JUnit root")
            }
            Event::Decl(_) => {
                ensure!(
                    !declaration_seen && roots == 0 && stack.is_empty(),
                    "XML declaration must appear once before JUnit root"
                );
                declaration_seen = true;
            }
            Event::Text(text) if stack.is_empty() => ensure!(
                text.iter().all(u8::is_ascii_whitespace),
                "text outside JUnit root"
            ),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(
        roots == 1 && suites > 0 && stack.is_empty() && !names.is_empty(),
        "missing, empty, or truncated native JUnit evidence"
    );
    Ok(names)
}
