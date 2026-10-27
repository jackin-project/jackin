use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

#[derive(Args)]
pub(crate) struct SchemaCheckArgs {
    #[arg(long, default_value = "origin/main")]
    base: String,
}

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
        version_file: "crates/core/jackin-config/src/versions.rs",
        fixtures_rel: "crates/apps/jackin/tests/fixtures/migrations/config",
    },
    SchemaKind {
        name: "workspace",
        const_name: "CURRENT_WORKSPACE_VERSION",
        version_file: "crates/core/jackin-config/src/versions.rs",
        fixtures_rel: "crates/apps/jackin/tests/fixtures/migrations/workspace",
    },
    SchemaKind {
        name: "manifest",
        const_name: "CURRENT_MANIFEST_VERSION",
        version_file: "crates/core/jackin-core/src/constants.rs",
        fixtures_rel: "crates/apps/jackin/tests/fixtures/migrations/manifest",
    },
];

const SCHEMA_VERSIONS_DOC: &str = "docs/content/reference/runtime/schema-versions.mdx";

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the gate result is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn run(args: SchemaCheckArgs) -> Result<()> {
    let root = repo_root()?;

    let verify = git(&root, &["rev-parse", "--verify", "--quiet", &args.base])?;
    if !verify.success {
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

fn doc_has_timeline_entry(doc_text: &str, version: &str) -> bool {
    let token = format!("`{version}`");
    doc_text
        .lines()
        .any(|line| line.trim_start().starts_with("### ") && line.contains(&token))
}

fn read_version(path: &Path, const_name: &str) -> Result<String> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_version(&text, const_name)
        .with_context(|| format!("could not find `const {const_name}` in {}", path.display()))
}

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

fn git_show_version(
    root: &Path,
    base: &str,
    file: &str,
    const_name: &str,
) -> Result<Option<String>> {
    let out = git(root, &["show", &format!("{base}:{file}")])?;
    if !out.success {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(parse_version(&text, const_name))
}

fn git(root: &Path, args: &[&str]) -> Result<jackin_process::ExecResult> {
    let mut cmd = crate::cmd::command("git");
    cmd.arg("-C").arg(root).args(args);
    crate::cmd::output_raw(&mut cmd)
}
