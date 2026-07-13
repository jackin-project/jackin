//! Shared gate reporter: human | json | github formats.
//!
//! Every first-party gate should emit structured violations so agents parse
//! results instead of scraping prose. Human mode preserves the DX quality of
//! existing messages; JSON is additive; Github emits workflow-command
//! annotations (`::error file=…`) when running under Actions.

use std::env;
use std::io::{self, Write};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::Serialize;

/// Output format for gate reports.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
pub(crate) enum Format {
    /// Prose suitable for a human operator (default).
    #[default]
    Human,
    /// Machine-readable JSON on stdout.
    Json,
    /// GitHub Actions workflow-command annotations + human block on stderr.
    Github,
}

impl Format {
    /// Resolve the effective format: an explicit CLI flag wins; otherwise
    /// `GITHUB_ACTIONS=true` selects Github; else Human.
    #[must_use]
    pub(crate) fn detect(cli_flag: Option<Format>) -> Self {
        if let Some(fmt) = cli_flag {
            return fmt;
        }
        match env::var("GITHUB_ACTIONS") {
            Ok(v) if v == "true" => Format::Github,
            _ => Format::Human,
        }
    }
}

/// One gate violation with the fields agents need to act.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct Violation {
    /// Short rule id, e.g. `"file-size"`.
    pub rule: &'static str,
    /// Repo-relative file path.
    pub file: String,
    /// Optional 1-based line number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    /// One sentence: why this is a violation.
    pub why: String,
    /// The exact clearing edit or command.
    pub fix: String,
    /// Narrowest rerun command.
    pub rerun: String,
}

/// Collected report for a single gate.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Report {
    /// Informal schema version so field changes are detectable.
    pub schema: u32,
    /// Gate name, e.g. `"file-size"`.
    pub gate: &'static str,
    /// True when `violations` is empty.
    pub ok: bool,
    pub violations: Vec<Violation>,
}

impl Report {
    #[must_use]
    pub(crate) fn new(gate: &'static str, violations: Vec<Violation>) -> Self {
        let ok = violations.is_empty();
        Self {
            schema: 1,
            gate,
            ok,
            violations,
        }
    }

    /// Emit the report in the requested format. Returns `Err` when there are
    /// violations (exit-code behavior unchanged from pre-reporter gates).
    pub(crate) fn emit(&self, format: Format) -> Result<()> {
        match format {
            Format::Human => emit_human(self)?,
            Format::Json => emit_json(self)?,
            Format::Github => emit_github(self)?,
        }
        if self.violations.is_empty() {
            Ok(())
        } else {
            bail!(
                "{} {} violation(s) — see report above",
                self.violations.len(),
                self.gate
            )
        }
    }
}

fn emit_human(report: &Report) -> Result<()> {
    let mut out = io::stdout().lock();
    if report.ok {
        writeln!(out, "{} gate OK", report.gate).context("writing human report")?;
        return Ok(());
    }
    writeln!(
        out,
        "{} {} violation(s):",
        report.violations.len(),
        report.gate
    )
    .context("writing human report")?;
    for v in &report.violations {
        writeln!(out).context("writing human report")?;
        if let Some(line) = v.line {
            writeln!(out, "  {}:{}  [{}]", v.file, line, v.rule).context("writing human report")?;
        } else {
            writeln!(out, "  {}  [{}]", v.file, v.rule).context("writing human report")?;
        }
        writeln!(out, "    why:  {}", v.why).context("writing human report")?;
        writeln!(out, "    fix:  {}", v.fix).context("writing human report")?;
        writeln!(out, "    rerun: {}", v.rerun).context("writing human report")?;
    }
    Ok(())
}

fn emit_json(report: &Report) -> Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, report).context("serializing gate report")?;
    writeln!(out).context("writing json newline")?;
    Ok(())
}

fn emit_github(report: &Report) -> Result<()> {
    let mut out = io::stdout().lock();
    for v in &report.violations {
        let mut props = format!("file={}", escape_workflow_prop(&v.file));
        if let Some(line) = v.line {
            props.push_str(&format!(",line={line}"));
        }
        props.push_str(&format!(",title={}", escape_workflow_prop(v.rule)));
        let msg = format!(
            "{} Fix: {} Rerun: {}",
            escape_workflow_data(&v.why),
            escape_workflow_data(&v.fix),
            escape_workflow_data(&v.rerun)
        );
        writeln!(out, "::error {props}::{msg}").context("writing github annotation")?;
    }
    // Keep logs readable: human block on stderr.
    {
        let mut err = io::stderr().lock();
        if report.ok {
            writeln!(err, "{} gate OK", report.gate).context("writing human stderr")?;
        } else {
            writeln!(
                err,
                "{} {} violation(s):",
                report.violations.len(),
                report.gate
            )
            .context("writing human stderr")?;
            for v in &report.violations {
                writeln!(err, "  {} — {}", v.file, v.why).context("writing human stderr")?;
            }
        }
    }
    Ok(())
}

/// Escape property values for workflow commands (`%`, `\r`, `\n`, `:`).
fn escape_workflow_prop(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

/// Escape message data for workflow commands.
fn escape_workflow_data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

#[cfg(test)]
mod tests;
