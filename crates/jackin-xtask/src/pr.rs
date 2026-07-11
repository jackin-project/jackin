//! Pull-request body helpers.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub(crate) enum PrCommand {
    /// Print a change digest (stderr) and a PR body skeleton (stdout) with the
    /// verify-locally blocks auto-selected from the diff.
    Body(BodyArgs),
}

#[derive(Args)]
pub(crate) struct BodyArgs {
    /// Git ref to diff against — the pre-PR baseline.
    #[arg(long, default_value = "origin/main")]
    base: String,
}

pub(crate) fn run(command: PrCommand) -> Result<()> {
    match command {
        PrCommand::Body(args) => body(args),
    }
}

// ---------------------------------------------------------------------------
// `pr body` — change digest + verify-block selection
// ---------------------------------------------------------------------------

/// Which categories of file the diff touches; each gates a verify-locally block.
#[derive(Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal file-bucket categories (rust, docs, capsule, schema) \
              used by `classify()` to bucket changed files in a PR digest. Each \
              bool is an independent path-prefix match; named-field reads match \
              the git path-filter idiom this helper parallels."
)]
struct Categories {
    rust: bool,
    docs: bool,
    capsule: bool,
    schema: bool,
}

fn classify(files: &[String]) -> Categories {
    let mut cats = Categories::default();
    for file in files {
        let path = Path::new(file);
        let is_rust = path.extension().is_some_and(|ext| ext == "rs")
            || matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("Cargo.toml" | "Cargo.lock")
            );
        if is_rust {
            cats.rust = true;
        }
        if file.starts_with("docs/") {
            cats.docs = true;
        }
        if file.starts_with("crates/jackin-capsule/") {
            cats.capsule = true;
        }
        if is_schema_path(file) {
            cats.schema = true;
        }
    }
    cats
}

/// Paths whose serde representation lives in a versioned schema file.
fn is_schema_path(file: &str) -> bool {
    file.starts_with("crates/jackin-config/")
        || file.starts_with("crates/jackin-manifest/")
        || file == "crates/jackin-core/src/constants.rs"
        || file.starts_with("crates/jackin/src/manifest/")
        || file.starts_with("crates/jackin/tests/fixtures/migrations/")
}

/// Keep a `### <block>` verify-locally subsection? Checkout always; the rest are
/// gated on the diff. Unknown blocks are kept (the template is the source of
/// truth for which blocks exist).
fn keep_block(name: &str, cats: &Categories) -> bool {
    match name {
        "Checkout" => true,
        "Static checks" | "Rust tests" | "User smoke" => cats.rust,
        "Schema migration smoke" => cats.schema,
        "Docs checks" | "Documentation" => cats.docs,
        "jackin-capsule smoke" => cats.capsule,
        _ => true,
    }
}

/// Drop the verify-locally `### ` subsections that do not apply to the diff,
/// keeping every other line of the template verbatim.
fn filter_template(template: &str, cats: &Categories) -> String {
    let mut out = Vec::new();
    let mut in_verify = false;
    let mut keep = true;
    let mut in_fence = false;
    for line in template.lines() {
        // A heading inside a ``` code fence (the verify blocks embed shell
        // fences) is content, not a section marker.
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            if let Some(heading) = line.strip_prefix("## ") {
                in_verify = heading.trim() == "Verify locally";
                keep = true;
                out.push(line);
                continue;
            }
            if in_verify && line.starts_with("### ") {
                keep = keep_block(line.trim_start_matches("### ").trim(), cats);
                if keep {
                    out.push(line);
                }
                continue;
            }
        }
        if in_verify && !keep {
            continue;
        }
        out.push(line);
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn body(args: BodyArgs) -> Result<()> {
    let root = crate::docs::repo_root()?;
    let files = changed_files(&root, &args.base)?;
    let cats = classify(&files);

    let template_path = root.join(".github/PULL_REQUEST_TEMPLATE.md");
    let template = fs::read_to_string(&template_path)
        .with_context(|| format!("reading {}", template_path.display()))?;
    let skeleton = filter_template(&template, &cats);

    emit_digest(&args.base, &files, &cats);
    emit_body(&skeleton);
    Ok(())
}

fn changed_files(root: &Path, base: &str) -> Result<Vec<String>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .args(["diff", "--name-only", &format!("{base}...HEAD")]);
    let stdout = crate::cmd::output(&mut cmd)?;
    Ok(String::from_utf8_lossy(&stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

#[expect(
    clippy::print_stderr,
    reason = "the change digest is agent-facing context, kept off stdout so the body can be redirected to a file"
)]
fn emit_digest(base: &str, files: &[String], cats: &Categories) {
    eprintln!("change digest (base {base}, {} file(s)):", files.len());
    eprintln!(
        "  categories: rust={} docs={} capsule={} schema={}",
        cats.rust, cats.docs, cats.capsule, cats.schema
    );
    for file in files {
        eprintln!("  {file}");
    }
    eprintln!("(prose sections are yours to fill; verify-locally blocks are pre-selected)");
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the body skeleton is its output, redirectable to a file"
)]
fn emit_body(skeleton: &str) {
    print!("{skeleton}");
}

fn crate::cmd::output(cmd: &mut Command) -> Result<Vec<u8>> {
    let display = display_command(cmd);
    #[expect(
        clippy::disallowed_methods,
        reason = "xtask automation shells out to git, gh, cargo, and mise"
    )]
    let output = cmd.output().with_context(|| format!("running {display}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "{display} failed with {}\n{}",
            output.status,
            stderr.trim()
        ))
    }
}

fn display_command(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy();
    let args = cmd
        .get_args()
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    if args.is_empty() {
        program.into_owned()
    } else {
        format!("{program} {args}")
    }
}

fn shell_quote(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '+'))
    {
        value.into_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

#[cfg(test)]
mod tests;
