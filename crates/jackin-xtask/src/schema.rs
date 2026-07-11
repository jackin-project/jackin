//! Schema-migration gate: enforce the five-artifact rule from `PRERELEASE.md`.
//!
//! When a PR bumps a versioned schema (`config.toml`, per-workspace files, or
//! `jackin.role.toml`), the rule requires a version bump, a migration step, a
//! new fixture directory, a re-bake of existing fixtures, and a
//! `schema-versions.mdx` Timeline entry. Shipping a subset still passes the unit
//! tests, so this gate runs in CI to catch the gap a reviewer would otherwise
//! have to eyeball.
//!
//! ```sh
//! cargo xtask schema-check --base origin/main
//! ```
//!
//! The migration step and the existing-fixture re-bake are already enforced by
//! `tests/migration_fixtures.rs` (it walks every chain on each run), so this gate
//! checks the two artifacts unit tests cannot see: the new `from-<predecessor>`
//! fixture directory, and the `schema-versions.mdx` entry for the new version.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

#[derive(Args)]
pub(crate) struct SchemaCheckArgs {
    /// Git ref to diff against — the pre-PR baseline. CI passes the merge base.
    #[arg(long, default_value = "origin/main")]
    base: String,
}

/// A versioned schema: where its `CURRENT_*_VERSION` lives and where its
/// migration fixtures sit.
struct SchemaKind {
    name: &'static str,
    const_name: &'static str,
    version_file: &'static str,
    fixtures_rel: &'static str,
}

const KINDS: &[SchemaKind] = &[
    SchemaKind {
        name: "config",
        const_name: "CURRENT_CONFIG_VERSION",
        version_file: "crates/jackin-config/src/versions.rs",
        fixtures_rel: "crates/jackin/tests/fixtures/migrations/config",
    },
    SchemaKind {
        name: "workspace",
        const_name: "CURRENT_WORKSPACE_VERSION",
        version_file: "crates/jackin-config/src/versions.rs",
        fixtures_rel: "crates/jackin/tests/fixtures/migrations/workspace",
    },
    SchemaKind {
        name: "manifest",
        const_name: "CURRENT_MANIFEST_VERSION",
        version_file: "crates/jackin-core/src/constants.rs",
        fixtures_rel: "crates/jackin/tests/fixtures/migrations/manifest",
    },
];

const SCHEMA_VERSIONS_DOC: &str = "docs/content/docs/reference/runtime/schema-versions.mdx";

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the gate result is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn run(args: SchemaCheckArgs) -> Result<()> {
    let root = repo_root()?;

    // A clear error beats silently passing when the baseline is unfetched.
    let verify = git(&root, &["rev-parse", "--verify", "--quiet", &args.base])?;
    if !verify.status.success() {
        bail!(
            "base ref `{}` not found — fetch it (e.g. `git fetch origin main`) or pass --base",
            args.base
        );
    }

    let mut problems = Vec::new();
    let mut bumps = 0_u32;
    for kind in KINDS {
        let head = read_version(&root.join(kind.version_file), kind.const_name)?;
        let base = git_show_version(&root, &args.base, kind.version_file, kind.const_name)?;
        // Only a real bump triggers the check. A file absent at base (a brand-new
        // schema) yields None and is skipped — there is no predecessor to migrate.
        if let Some(base) = base
            && base != head
        {
            bumps += 1;
            check_bump(&root, kind, &base, &head, &mut problems)?;
        }
    }

    if problems.is_empty() {
        emit(&if bumps == 0 {
            "schema-check: no versioned schema bump in this diff — nothing to verify.".to_owned()
        } else {
            format!("schema-check: {bumps} schema bump(s), all required artifacts present.")
        });
        return Ok(());
    }
    bail!(
        "schema-check found {} problem(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

/// Assert the fixture + doc artifacts a version bump must ship.
fn check_bump(
    root: &Path,
    kind: &SchemaKind,
    base_ver: &str,
    head_ver: &str,
    problems: &mut Vec<String>,
) -> Result<()> {
    let fixture = root
        .join(kind.fixtures_rel)
        .join(format!("from-{base_ver}"));
    for file in ["meta.toml", "before.toml", "after.toml"] {
        if !fixture.join(file).is_file() {
            problems.push(format!(
                "{}: bump {base_ver} → {head_ver} is missing fixture file {}",
                kind.name,
                fixture.join(file).display()
            ));
        }
    }

    let meta = fixture.join("meta.toml");
    if meta.is_file() {
        let text =
            fs::read_to_string(&meta).with_context(|| format!("reading {}", meta.display()))?;
        if !text.contains(&format!("target_version = \"{head_ver}\"")) {
            problems.push(format!(
                "{}: {} does not declare target_version = \"{head_ver}\"",
                kind.name,
                meta.display()
            ));
        }
    }

    let doc = root.join(SCHEMA_VERSIONS_DOC);
    let doc_text = fs::read_to_string(&doc).unwrap_or_default();
    if !doc_has_timeline_entry(&doc_text, head_ver) {
        problems.push(format!(
            "{}: no Timeline entry for `{head_ver}` in {SCHEMA_VERSIONS_DOC}",
            kind.name
        ));
    }
    Ok(())
}

/// True when `schema-versions.mdx` carries a Timeline entry for `version` — a
/// `###` heading containing the backtick-wrapped version. The backticks bound
/// the match so `v1alpha1` is not satisfied by an existing `v1alpha10` heading,
/// and a bare mention in prose does not count as the Timeline entry.
fn doc_has_timeline_entry(doc_text: &str, version: &str) -> bool {
    let token = format!("`{version}`");
    doc_text
        .lines()
        .any(|line| line.trim_start().starts_with("### ") && line.contains(&token))
}

/// Read a `pub const <NAME>: &str = "vX";` value from a source file.
fn read_version(path: &Path, const_name: &str) -> Result<String> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_version(&text, const_name)
        .with_context(|| format!("could not find `const {const_name}` in {}", path.display()))
}

/// Extract the string literal from the `const <NAME>` declaration line.
fn parse_version(text: &str, const_name: &str) -> Option<String> {
    let needle = format!("const {const_name}");
    text.lines().find_map(|line| {
        if line.contains(&needle) {
            line.split('"').nth(1).map(str::to_owned)
        } else {
            None
        }
    })
}

/// Read the version const from `<base>:<file>`. `None` when the file did not
/// exist at the base ref (a brand-new schema file).
fn git_show_version(
    root: &Path,
    base: &str,
    file: &str,
    const_name: &str,
) -> Result<Option<String>> {
    let out = git(root, &["show", &format!("{base}:{file}")])?;
    if !out.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(parse_version(&text, const_name))
}

fn git(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).args(args);
    crate::cmd::output_raw(&mut cmd)
}


#[cfg(test)]
mod tests;
