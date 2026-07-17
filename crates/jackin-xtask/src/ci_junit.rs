use std::{
    collections::BTreeSet,
    env,
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
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
    #[arg(long, default_value = "flaky-tests.toml")]
    quarantine: PathBuf,
}

#[derive(Debug, PartialEq)]
struct TestCase {
    name: String,
    seconds: f64,
    flaky: bool,
}

pub(crate) fn run(args: CiJunitArgs) -> Result<()> {
    if !args.report.is_file() {
        write_output("exists", "false")?;
        return Ok(());
    }
    write_output("exists", "true")?;
    let cases = parse_report(&args.report)?;
    write_summary(&args.group, &cases)?;
    enforce_quarantine(&args.quarantine, &cases)
}

fn parse_report(path: &Path) -> Result<Vec<TestCase>> {
    let mut reader = Reader::from_file(path)
        .with_context(|| format!("opening JUnit report {}", path.display()))?;
    let mut buffer = Vec::new();
    let mut cases = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element) | Event::Empty(element))
                if element.name().as_ref() == b"testcase" =>
            {
                let mut name = String::new();
                let mut seconds = 0.0;
                let mut flaky = false;
                for attribute in element.attributes() {
                    let attribute = attribute.context("parsing JUnit testcase attribute")?;
                    let value = attribute
                        .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                        .context("decoding JUnit testcase attribute")?;
                    match attribute.key.as_ref() {
                        b"name" => name = value.into_owned(),
                        b"time" => seconds = value.parse().unwrap_or(0.0),
                        b"flaky" => flaky = matches!(value.as_ref(), "true" | "1"),
                        _ => {}
                    }
                }
                cases.push(TestCase {
                    name,
                    seconds,
                    flaky,
                });
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

#[expect(
    clippy::disallowed_methods,
    reason = "the synchronous CI tool owns this short-lived summary writer"
)]
fn write_summary(group: &str, cases: &[TestCase]) -> Result<()> {
    let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") else {
        return Ok(());
    };
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening step summary {}", Path::new(&path).display()))?;
    let mut summary = BufWriter::new(file);
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
    Ok(())
}

fn enforce_quarantine(path: &Path, cases: &[TestCase]) -> Result<()> {
    let flaky = cases
        .iter()
        .filter(|case| case.flaky)
        .map(|case| case.name.as_str())
        .collect::<BTreeSet<_>>();
    if flaky.is_empty() {
        return Ok(());
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("reading flake quarantine {}", path.display()))?;
    let document = contents
        .parse::<toml::Value>()
        .with_context(|| format!("parsing flake quarantine {}", path.display()))?;
    let quarantined = document
        .get("test")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("name").and_then(toml::Value::as_str))
        .collect::<BTreeSet<_>>();
    let unquarantined = flaky.difference(&quarantined).copied().collect::<Vec<_>>();
    if !unquarantined.is_empty() {
        bail!(
            "flaky test(s) detected and not quarantined in {}:\n{}",
            path.display(),
            unquarantined.join("\n")
        );
    }
    Ok(())
}

fn write_output(name: &str, value: &str) -> Result<()> {
    let Some(path) = env::var_os("GITHUB_OUTPUT") else {
        return Ok(());
    };
    let mut contents = fs::read(&path).unwrap_or_default();
    writeln!(contents, "{name}={value}")?;
    fs::write(&path, contents).with_context(|| format!("writing {}", Path::new(&path).display()))
}

#[cfg(test)]
mod tests;
