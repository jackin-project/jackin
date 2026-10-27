// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Args;
use quick_xml::{XmlVersion, events::Event, reader::Reader};

#[derive(Args, Debug)]
pub(crate) struct CiJunitArgs {
    #[arg(long)]
    report: PathBuf,
    #[arg(long)]
    group: String,
}

#[derive(Debug, PartialEq)]
struct TestCase {
    name: String,
    seconds: f64,
}

pub(crate) fn run(args: CiJunitArgs) -> Result<()> {
    if !args.report.is_file() {
        write_output("exists", "false")?;
        return Ok(());
    }
    write_output("exists", "true")?;
    let cases = parse_report(&args.report)?;
    write_summary(&args.group, &cases)
}

fn parse_report(path: &Path) -> Result<Vec<TestCase>> {
    let mut reader = Reader::from_file(path)
        .with_context(|| format!("opening JUnit report {}", path.display()))?;
    let mut buffer = Vec::new();
    let mut cases = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element) | Event::Empty(element))
                if element.name().as_ref() == "testcase" =>
            {
                let mut name = String::new();
                let mut seconds = 0.0;
                for attribute in element.attributes() {
                    let attribute = attribute.context("parsing JUnit testcase attribute")?;
                    let value = attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .context("decoding JUnit testcase attribute")?;
                    match attribute.key.as_ref() {
                        "name" => name = value.into_owned(),
                        "time" => seconds = value.parse().unwrap_or(0.0),
                        _ => {}
                    }
                }
                cases.push(TestCase { name, seconds });
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => bail!(
                "parsing JUnit report {} at byte {}: {error}",
                path.display(),
                reader.error_position()
            ),
        }
        buffer.clear();
    }
    Ok(cases)
}

fn write_summary(group: &str, cases: &[TestCase]) -> Result<()> {
    let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") else {
        return Ok(());
    };
    let mut summary = Vec::new();
    writeln!(summary, "### Slowest tests ({group})")?;
    let mut slowest = cases.iter().collect::<Vec<_>>();
    slowest.sort_by(|left, right| right.seconds.total_cmp(&left.seconds));
    for case in slowest.into_iter().take(10) {
        writeln!(summary, "- {}s `{}`", case.seconds, case.name)?;
    }
    let total = cases.iter().map(|case| case.seconds).sum::<f64>();
    writeln!(
        summary,
        "\nCrate wall time (sum of testcase times): {total:.3}s"
    )?;
    let path = Path::new(&path);
    let mut contents = fs::read(path).unwrap_or_default();
    contents.extend_from_slice(&summary);
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

fn write_output(name: &str, value: &str) -> Result<()> {
    let Some(path) = env::var_os("GITHUB_OUTPUT") else {
        return Ok(());
    };
    let mut contents = fs::read(&path).unwrap_or_default();
    writeln!(contents, "{name}={value}")?;
    fs::write(&path, contents).with_context(|| format!("writing {}", Path::new(&path).display()))
}
