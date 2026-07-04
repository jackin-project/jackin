use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

#[derive(Args, Debug)]
pub(crate) struct CiArgs {
    /// Skip intentionally slow lanes: feature-powerset and Docker E2E.
    #[arg(long)]
    fast: bool,
    /// Include Docker E2E with capsule export and Docker daemon preflight.
    #[arg(long)]
    e2e: bool,
    /// Git ref used by schema-check.
    #[arg(long, default_value = "origin/main")]
    base: String,
}

struct Step {
    name: String,
    program: OsString,
    args: Vec<OsString>,
    env: BTreeMap<String, OsString>,
}

impl Step {
    fn new(name: impl Into<String>, program: impl Into<OsString>, args: &[&str]) -> Self {
        Self {
            name: name.into(),
            program: program.into(),
            args: args.iter().map(OsString::from).collect(),
            env: BTreeMap::new(),
        }
    }

    fn with_env(mut self, key: impl Into<String>, value: impl Into<OsString>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    fn with_args(
        name: impl Into<String>,
        program: impl Into<OsString>,
        args: Vec<OsString>,
    ) -> Self {
        Self {
            name: name.into(),
            program: program.into(),
            args,
            env: BTreeMap::new(),
        }
    }
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; ci progress is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn run(args: CiArgs) -> Result<()> {
    let root = repo_root()?;
    let mut failures = Vec::new();

    for step in build_steps(&root, &args)? {
        if let Err(err) = run_step(&root, &step) {
            failures.push(format!("{}: {err:#}", step.name));
        }
    }

    if args.e2e {
        match build_e2e_step(&root) {
            Ok(step) => {
                if let Err(err) = run_step(&root, &step) {
                    failures.push(format!("{}: {err:#}", step.name));
                }
            }
            Err(err) => failures.push(format!("docker e2e: {err:#}")),
        }
    }

    if failures.is_empty() {
        emit("ci gate OK");
        return Ok(());
    }

    bail!(
        "{} ci step(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    )
}

fn build_steps(root: &Path, args: &CiArgs) -> Result<Vec<Step>> {
    let mut steps = vec![
        Step::with_args("actionlint", "actionlint", actionlint_args(root)?),
        cargo("fmt", &["fmt", "--check"]),
        cargo(
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
        cargo(
            "check",
            &["check", "--workspace", "--all-targets", "--locked"],
        ),
        cargo(
            "nextest",
            &[
                "nextest",
                "run",
                "--workspace",
                "--all-features",
                "--locked",
            ],
        ),
        cargo("audit", &["audit"]),
        cargo(
            "deny",
            &["deny", "check", "advisories", "bans", "licenses", "sources"],
        ),
        cargo_xtask(
            "schema-check",
            &["schema-check", "--base", args.base.as_str()],
        ),
        cargo_xtask("lint", &["lint", "--strict"]),
        cargo("shear", &["shear", "--deny-warnings"]),
        cargo(
            "msrv",
            &["check", "--workspace", "--all-targets", "--locked"],
        )
        .with_env("RUSTUP_TOOLCHAIN", msrv_version(root)?),
    ];

    if !args.fast {
        steps.push(cargo(
            "feature-powerset",
            &[
                "hack",
                "check",
                "--workspace",
                "--feature-powerset",
                "--all-targets",
                "--locked",
            ],
        ));
    }

    Ok(steps)
}

fn cargo(name: &str, args: &[&str]) -> Step {
    Step::new(format!("cargo {name}"), "cargo", args)
}

fn cargo_xtask(name: &str, args: &[&str]) -> Step {
    let mut cargo_args = vec!["xtask"];
    cargo_args.extend_from_slice(args);
    cargo(name, &cargo_args)
}

fn actionlint_args(root: &Path) -> Result<Vec<OsString>> {
    let workflows = root.join(".github/workflows");
    if !workflows.is_dir() {
        bail!("{} does not exist", workflows.display());
    }
    let mut files = Vec::new();
    for entry in
        std::fs::read_dir(&workflows).with_context(|| format!("reading {}", workflows.display()))?
    {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "yml") {
            files.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .as_os_str()
                    .to_owned(),
            );
        }
    }
    if files.is_empty() {
        bail!("no workflow files found under {}", workflows.display());
    }
    files.sort();
    Ok(files)
}

fn msrv_version(root: &Path) -> Result<String> {
    let cargo_toml = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&cargo_toml)
        .with_context(|| format!("reading {}", cargo_toml.display()))?;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("rust-version = ") {
            return Ok(value.trim_matches('"').to_owned());
        }
    }
    bail!("rust-version not found in {}", cargo_toml.display())
}

fn build_e2e_step(root: &Path) -> Result<Step> {
    run_step(root, &Step::new("docker preflight", "docker", &["info"])).context(
        "Docker daemon is not reachable; start Docker before running `cargo xtask ci --e2e`",
    )?;

    let export = output_step(
        root,
        &cargo(
            "build-jackin-capsule export",
            &["run", "--bin", "build-jackin-capsule", "--", "--export"],
        ),
    )?;
    let capsule_bin = parse_capsule_export(&export)?;

    Ok(cargo(
        "docker e2e",
        &[
            "nextest",
            "run",
            "-p",
            "jackin",
            "--features",
            "e2e",
            "--profile",
            "docker-e2e",
            "--locked",
        ],
    )
    .with_env("JACKIN_CAPSULE_BIN", capsule_bin))
}

fn parse_capsule_export(output: &str) -> Result<PathBuf> {
    for line in output.lines() {
        let Some(value) = line.strip_prefix("export JACKIN_CAPSULE_BIN=") else {
            continue;
        };
        let path = PathBuf::from(value.trim_matches(&['"', '\''][..]));
        if path.is_file() {
            return Ok(path);
        }
        bail!("capsule export path does not exist: {}", path.display());
    }
    bail!("build-jackin-capsule --export did not print JACKIN_CAPSULE_BIN")
}

fn run_step(root: &Path, step: &Step) -> Result<()> {
    emit(&format!("==> {}", display_step(step)));
    let status = Command::new(&step.program)
        .args(&step.args)
        .current_dir(root)
        .envs(&step.env)
        .status()
        .with_context(|| format!("starting {}", step.name))?;
    if status.success() {
        return Ok(());
    }
    bail!("exited with {status}")
}

fn output_step(root: &Path, step: &Step) -> Result<String> {
    emit(&format!("==> {}", display_step(step)));
    #[expect(
        clippy::disallowed_methods,
        reason = "xtask automation captures build-jackin-capsule --export output"
    )]
    let output = Command::new(&step.program)
        .args(&step.args)
        .current_dir(root)
        .envs(&step.env)
        .output()
        .with_context(|| format!("starting {}", step.name))?;
    if !output.status.success() {
        bail!("{} exited with {}", step.name, output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn display_step(step: &Step) -> String {
    let mut parts = vec![step.program.to_string_lossy().into_owned()];
    parts.extend(
        step.args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned()),
    );
    parts.join(" ")
}

#[cfg(test)]
mod tests;
