//! Extract a recorded PTY byte stream from a `--debug` run log.
//!
//! `jackin-xtask pty-fixture <run.jsonl> <session-label> <out.bin>` scans the
//! log for the capsule's `session feed_pty bytes:` debug lines, filters them
//! to one session label, decodes the hex byte dumps, and concatenates them in
//! order into a binary fixture for the echo-back conformance harness
//! (`crates/jackin-capsule/src/daemon/tests.rs`). The
//! input may be the host diagnostics run JSONL (feed lines embedded in JSON
//! string fields) or a raw `multiplexer.log` — both line shapes are handled.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;

#[derive(Args)]
pub(crate) struct PtyFixtureArgs {
    /// Diagnostics run log: `~/.jackin/data/diagnostics/runs/<run-id>.jsonl`
    /// or a raw capsule `multiplexer.log`.
    run_log: PathBuf,
    /// Session label to extract — the `label=` field of the
    /// `session feed_pty bytes:` lines (e.g. `Codex`).
    session_label: String,
    /// Output fixture path, conventionally
    /// `crates/jackin-capsule/tests/fixtures/pty/<agent>-<scenario>.bin`.
    out_bin: PathBuf,
}

pub(crate) fn run(args: PtyFixtureArgs) -> Result<()> {
    let raw = fs::read_to_string(&args.run_log)
        .with_context(|| format!("failed to read {}", args.run_log.display()))?;

    let mut out = Vec::new();
    let mut chunks = 0usize;
    for line in raw.lines() {
        // Run JSONL embeds the capsule log line inside JSON string fields;
        // the serde unescape recovers the original text. Raw log lines are
        // scanned as-is.
        if line.trim_start().starts_with('{')
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
        {
            visit_strings(&value, &mut |text| {
                if let Some(bytes) = extract_feed_pty_bytes(text, &args.session_label) {
                    out.extend_from_slice(&bytes);
                    chunks += 1;
                }
            });
            continue;
        }
        if let Some(bytes) = extract_feed_pty_bytes(line, &args.session_label) {
            out.extend_from_slice(&bytes);
            chunks += 1;
        }
    }

    if chunks == 0 {
        bail!(
            "no `session feed_pty bytes:` lines with label={} in {} — was the run recorded with --debug?",
            args.session_label,
            args.run_log.display()
        );
    }

    if let Some(parent) = args.out_bin.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&args.out_bin, &out)
        .with_context(|| format!("failed to write {}", args.out_bin.display()))?;
    #[expect(
        clippy::print_stdout,
        reason = "xtask is a CLI; the summary is its user-facing output"
    )]
    {
        println!(
            "wrote {} bytes from {chunks} PTY chunks to {}",
            out.len(),
            args.out_bin.display()
        );
    }
    Ok(())
}

fn visit_strings(value: &serde_json::Value, visit: &mut impl FnMut(&str)) {
    match value {
        serde_json::Value::String(s) => visit(s),
        serde_json::Value::Array(items) => {
            for item in items {
                visit_strings(item, visit);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                visit_strings(item, visit);
            }
        }
        _ => {}
    }
}

/// Parse one capsule debug line of the shape
/// `... session feed_pty bytes: agent=Some("codex") label=Codex len=42 bytes=[1b, 5b, ...]`
/// returning the decoded bytes when the label matches.
fn extract_feed_pty_bytes(line: &str, label: &str) -> Option<Vec<u8>> {
    let rest = &line[line.find("session feed_pty bytes:")?..];
    let label_value = rest
        .split_whitespace()
        .find_map(|field| field.strip_prefix("label="))?;
    if label_value != label {
        return None;
    }
    let hex = &rest[rest.find("bytes=[")? + "bytes=[".len()..];
    let hex = &hex[..hex.find(']')?];
    let mut out = Vec::new();
    for byte in hex.split(',') {
        let byte = byte.trim();
        if byte.is_empty() {
            continue;
        }
        out.push(u8::from_str_radix(byte, 16).ok()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests;
