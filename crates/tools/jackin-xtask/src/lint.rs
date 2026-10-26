use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;
use crate::ratchet::{self, FILE_SIZE_FAMILIES};
use crate::report::{Format, Report, Violation};

const PRODUCTION_GLOB: &str = "crates";
const RERUN: &str = "cargo xtask lint files";

#[derive(Args, Debug)]
pub(crate) struct LintFilesArgs {
    #[arg(long)]
    print_budget: bool,
    #[arg(long, value_enum)]
    format: Option<Format>,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the lint report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn run(args: LintFilesArgs) -> Result<()> {
    if args.print_budget {
        return ratchet::print_families(FILE_SIZE_FAMILIES);
    }

    let format = Format::detect(args.format);
    let outcome = ratchet::check_families_at_root(FILE_SIZE_FAMILIES)?;
    if outcome.problems.is_empty() {
        if matches!(format, Format::Human) {
            let root = repo_root()?;
            let counts = measure_lines(&root)?;
            let prod_cap = ratchet::family_cap("file-size-production")?;
            emit(&format!(
                "file-size budget OK — {} files measured, production cap = {} (ratchet.toml)",
                counts.len(),
                prod_cap,
            ));
        } else {
            Report::new("file-size", Vec::new()).emit(format)?;
        }
        return Ok(());
    }

    let violations: Vec<Violation> = outcome
        .problems
        .into_iter()
        .map(|p| Violation {
            rule: "file-size",
            file: p.key,
            line: None,
            message: p.message.clone(),
            fix: format!(
                "update `ratchet.toml` family `{}` (or refactor the source); regenerate: cargo xtask lint ratchet --print {}",
                p.family, p.family
            ),
            rerun: RERUN.into(),
        })
        .collect();
    Report::new("file-size", violations).emit(format)
}

pub(crate) fn measure_lines(root: &Path) -> Result<BTreeMap<PathBuf, usize>> {
    let crates_dir = root.join(PRODUCTION_GLOB);
    if !crates_dir.is_dir() {
        bail!("`{PRODUCTION_GLOB}/` not found under {}", root.display());
    }
    let mut out = BTreeMap::new();
    walk(&crates_dir, &mut out)?;
    Ok(out)
}

pub(crate) fn is_test_source_path(relative_path: &Path) -> bool {
    let mut has_tests_dir = false;
    for component in relative_path.components() {
        match component {
            std::path::Component::RootDir
            | std::path::Component::Prefix(_)
            | std::path::Component::ParentDir => return false,
            std::path::Component::Normal(name) if name == "tests" => has_tests_dir = true,
            _ => {}
        }
    }
    has_tests_dir
        || relative_path
            .file_name()
            .is_some_and(|name| name == "tests.rs")
}

fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, usize>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out)?;
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "rs") {
            let text =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let lines = text.lines().count();
            out.insert(path, lines);
        }
    }
    Ok(())
}
