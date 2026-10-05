// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, fs, io::Read, path::Path, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use quick_xml::{XmlVersion, events::Event, reader::Reader};
use serde_json::Value;

use crate::cmd;

const MAX_EVIDENCE_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn run(
    root: &Path,
    binary: &str,
    filter: &str,
    expected: &BTreeSet<String>,
    capsule: Option<&Path>,
) -> Result<Vec<String>> {
    ensure!(
        !expected.is_empty(),
        "consumer gate must require at least one test"
    );
    let mut list = nextest_command(root, "list", binary, filter, capsule);
    list.args(["--message-format", "json"]);
    let inventory = cmd::output_timeout(&mut list, Duration::from_secs(1800))?;
    let selected = parse_inventory(&inventory, binary)?;
    require_exact("nextest selection", &selected, expected)?;

    let report = root.join("target/nextest/docker-e2e/junit.xml");
    check_report_ancestors(root)?;
    match fs::symlink_metadata(&report) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "JUnit report must be a regular file: {}",
                report.display()
            );
            fs::remove_file(&report).context("removing stale consumer JUnit report")?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspecting stale consumer JUnit report"),
    }
    let mut run = nextest_command(root, "run", binary, filter, capsule);
    run.args(["--retries", "0", "--no-tests", "fail"]);
    cmd::output_timeout(&mut run, Duration::from_secs(3600))?;
    check_report_ancestors(root)?;
    let metadata =
        fs::symlink_metadata(&report).context("nextest produced no fresh JUnit report")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "fresh JUnit report must be a regular file"
    );
    ensure!(
        metadata.len() <= MAX_EVIDENCE_BYTES as u64,
        "JUnit report exceeds 16 MiB bound"
    );
    let executed = parse_junit(&read_report(&report)?)?;
    require_exact("executed JUnit tests", &executed, expected)?;
    Ok(executed.into_iter().collect())
}

#[expect(
    clippy::disallowed_methods,
    reason = "synchronous CLI evidence read outside runtime and render paths"
)]
fn read_report(report: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(report)
        .context("opening fresh consumer JUnit report")?
        .take(MAX_EVIDENCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .context("reading fresh consumer JUnit report")?;
    ensure!(
        bytes.len() <= MAX_EVIDENCE_BYTES,
        "JUnit report exceeds 16 MiB bound"
    );
    Ok(bytes)
}

fn nextest_command(
    root: &Path,
    action: &str,
    binary: &str,
    filter: &str,
    capsule: Option<&Path>,
) -> std::process::Command {
    let mut command = cmd::command("cargo");
    command
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"));
    if let Some(capsule) = capsule {
        command.env("JACKIN_CAPSULE_BIN", capsule);
    }
    command.args([
        "nextest",
        action,
        "-p",
        "jackin",
        "--features",
        "e2e",
        "--test",
        binary,
        "--profile",
        "docker-e2e",
        "-E",
        filter,
    ]);
    command
}

fn check_report_ancestors(root: &Path) -> Result<()> {
    let mut path = root.to_path_buf();
    for component in ["target", "nextest", "docker-e2e"] {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "JUnit directory must be a real directory: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error).context("inspecting JUnit directory"),
        }
    }
    Ok(())
}

fn require_exact(
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

fn parse_inventory(bytes: &[u8], binary: &str) -> Result<BTreeSet<String>> {
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

fn parse_junit(bytes: &[u8]) -> Result<BTreeSet<String>> {
    ensure!(
        bytes.len() <= MAX_EVIDENCE_BYTES,
        "JUnit report exceeds 16 MiB bound"
    );
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_comments = true;
    let mut buffer = Vec::new();
    let mut stack: Vec<(Vec<u8>, Option<(u64, u64)>)> = Vec::new();
    let mut cases = BTreeSet::new();
    let mut root_seen = false;
    let mut declaration_seen = false;
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .context("parsing consumer JUnit XML")?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let tag = element.name();
                let tag = tag.as_ref();
                ensure!(stack.len() < 64, "JUnit exceeds nesting bound");
                if stack.is_empty() {
                    ensure!(
                        !root_seen && matches!(tag, b"testsuites" | b"testsuite"),
                        "invalid JUnit root"
                    );
                    root_seen = true;
                } else if tag == b"testsuites" {
                    bail!("nested JUnit testsuites element");
                } else if tag == b"testsuite" {
                    ensure!(
                        stack
                            .last()
                            .is_some_and(|(parent, _)| parent == b"testsuites"),
                        "JUnit testsuite outside testsuites"
                    );
                }
                ensure!(
                    !matches!(
                        tag,
                        b"failure"
                            | b"error"
                            | b"skipped"
                            | b"rerunFailure"
                            | b"rerunError"
                            | b"flakyFailure"
                            | b"flakyError"
                            | b"rerun"
                            | b"flaky"
                            | b"retry"
                    ),
                    "JUnit contains failure, skip, or retry evidence"
                );
                if tag == b"testcase" {
                    ensure!(
                        stack
                            .last()
                            .is_some_and(|(parent, _)| parent == b"testsuite"),
                        "JUnit testcase outside testsuite"
                    );
                }
                let mut name = None;
                let mut counts = None;
                for attribute in element.attributes() {
                    let attribute = attribute.context("parsing consumer JUnit attribute")?;
                    let value = attribute
                        .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                        .context("decoding consumer JUnit attribute")?;
                    match attribute.key.as_ref() {
                        b"name" if tag == b"testcase" => name = Some(value.into_owned()),
                        b"tests" if matches!(tag, b"testsuite" | b"testsuites") => {
                            counts = Some((value.parse::<u64>()?, 0))
                        }
                        b"failures" | b"errors" | b"skipped" | b"disabled" | b"retries"
                        | b"reruns" | b"retry" | b"rerun" => ensure!(
                            value.parse::<u64>()? == 0,
                            "JUnit contains nonzero failure, skip, or retry count"
                        ),
                        b"flaky" => ensure!(
                            matches!(value.as_ref(), "false" | "0"),
                            "JUnit contains flaky test"
                        ),
                        b"status" if tag == b"testcase" => ensure!(
                            matches!(value.as_ref(), "passed" | "run"),
                            "JUnit testcase is not passed"
                        ),
                        b"attempts" => ensure!(
                            value.parse::<u64>()? == 1,
                            "JUnit testcase has retry attempts"
                        ),
                        _ => {}
                    }
                }
                if tag == b"testcase" {
                    let name = name.context("JUnit testcase lacks name")?;
                    ensure!(
                        !name.is_empty() && cases.insert(name.clone()),
                        "empty or duplicate JUnit testcase {name}"
                    );
                    for (_, count) in &mut stack {
                        if let Some((_, actual)) = count {
                            *actual += 1;
                        }
                    }
                }
                if empty {
                    if let Some((expected, actual)) = counts {
                        ensure!(
                            expected == actual,
                            "JUnit declared test count differs from testcase count"
                        );
                    }
                } else {
                    stack.push((tag.to_vec(), counts));
                }
            }
            Event::End(element) => {
                let (tag, counts) = stack.pop().context("unmatched JUnit closing element")?;
                ensure!(tag == element.name().as_ref(), "unbalanced JUnit XML");
                if let Some((expected, actual)) = counts {
                    ensure!(
                        expected == actual,
                        "JUnit declared test count differs from testcase count"
                    );
                }
            }
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
                text.iter().all(u8::is_ascii_whitespace),
                "text outside JUnit root"
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

#[cfg(test)]
mod tests;
