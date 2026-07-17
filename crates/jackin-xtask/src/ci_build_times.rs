use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;

use crate::docs::repo_root;

const DEFAULT_PACKAGES: &[&str] = &[
    "jackin-runtime",
    "jackin-capsule",
    "jackin-console",
    "jackin-term",
    "jackin-config",
];

#[derive(Args, Debug)]
pub(crate) struct CiBuildTimesArgs {
    /// Destination consumed by the build-time ratchet.
    #[arg(long, default_value = "target/build-times.json")]
    output: PathBuf,
}

#[derive(Debug, Serialize)]
struct Timing {
    clean_s: u64,
    incremental_s: u64,
}

pub(crate) fn run(args: CiBuildTimesArgs) -> Result<()> {
    let root = repo_root()?;
    let sources = package_sources(&root)?;
    let mut timings = BTreeMap::new();
    for package in DEFAULT_PACKAGES {
        cargo(&root, &["clean", "-p", package])?;
        let clean_s = timed_build(&root, package)?;
        refresh_source(
            sources
                .get(*package)
                .with_context(|| format!("package `{package}` has no library source"))?,
        )?;
        let incremental_s = timed_build(&root, package)?;
        timings.insert(
            (*package).to_owned(),
            Timing {
                clean_s,
                incremental_s,
            },
        );
    }
    write_results(&root, &args.output, &timings)?;
    append_summary(&timings)
}

fn timed_build(root: &Path, package: &str) -> Result<u64> {
    let started = Instant::now();
    cargo(root, &["build", "-p", package, "--locked", "--offline"])?;
    Ok(started.elapsed().as_secs())
}

fn cargo(root: &Path, args: &[&str]) -> Result<()> {
    crate::cmd::run_streaming(Command::new("cargo").args(args).current_dir(root))
        .with_context(|| format!("running cargo {}", args.join(" ")))
}

fn package_sources(root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let output = crate::cmd::output(
        Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--locked",
                "--offline",
            ])
            .current_dir(root),
    )
    .context("reading Cargo workspace metadata")?;
    package_sources_from_json(&output)
}

fn package_sources_from_json(bytes: &[u8]) -> Result<BTreeMap<String, PathBuf>> {
    let metadata: serde_json::Value =
        serde_json::from_slice(bytes).context("parsing Cargo workspace metadata")?;
    let mut sources = BTreeMap::new();
    for package in metadata["packages"]
        .as_array()
        .context("metadata packages")?
    {
        let Some(name) = package["name"].as_str() else {
            continue;
        };
        let Some(targets) = package["targets"].as_array() else {
            continue;
        };
        let Some(source) = targets.iter().find_map(|target| {
            target["kind"]
                .as_array()?
                .iter()
                .any(|kind| kind.as_str() == Some("lib"))
                .then(|| target["src_path"].as_str())
                .flatten()
        }) else {
            continue;
        };
        sources.insert(name.to_owned(), PathBuf::from(source));
    }
    Ok(sources)
}

fn refresh_source(path: &Path) -> Result<()> {
    let contents = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    fs::write(path, contents).with_context(|| format!("refreshing {}", path.display()))
}

fn write_results(root: &Path, output: &Path, timings: &BTreeMap<String, Timing>) -> Result<()> {
    let output = root.join(output);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let contents = serde_json::to_vec_pretty(timings).context("serializing build timings")?;
    fs::write(&output, contents).with_context(|| format!("writing {}", output.display()))
}

fn append_summary(timings: &BTreeMap<String, Timing>) -> Result<()> {
    let Some(summary) = env::var_os("GITHUB_STEP_SUMMARY") else {
        return Ok(());
    };
    let mut contents = fs::read(&summary).unwrap_or_default();
    writeln!(contents, "### Per-crate build times (advisory)\n")?;
    writeln!(contents, "| Crate | Clean (s) | Incremental (s) |")?;
    writeln!(contents, "| --- | ---: | ---: |")?;
    for (package, timing) in timings {
        writeln!(
            contents,
            "| `{package}` | {} | {} |",
            timing.clean_s, timing.incremental_s
        )?;
    }
    fs::write(&summary, contents).context("writing GitHub step summary")
}

#[cfg(test)]
mod tests {
    use super::package_sources_from_json;

    #[test]
    fn resolves_library_sources_from_cargo_metadata() {
        let metadata = br#"{"packages":[{"name":"jackin-core","targets":[{"kind":["lib"],"src_path":"/repo/crates/jackin-core/src/lib.rs"}]}]}"#;

        let sources = package_sources_from_json(metadata).unwrap();

        assert_eq!(
            sources["jackin-core"].to_string_lossy(),
            "/repo/crates/jackin-core/src/lib.rs"
        );
    }
}
