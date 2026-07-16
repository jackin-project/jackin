use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

/// CI partition names for `--only` selection.
///
/// `lint` | `policy` | `tests` | `msrv` | `powerset` | `docs` | `snapshots`
pub(crate) const PARTITIONS: &[&str] = &[
    "lint",
    "policy",
    "tests",
    "msrv",
    "powerset",
    "docs",
    "snapshots",
];

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
    /// Run only the named partition(s). Repeatable.
    ///
    /// Partitions: lint, policy, tests, msrv, powerset, docs, snapshots.
    /// Local-dev convenience only — merge readiness remains the full `ci`.
    #[arg(long = "only", value_name = "PARTITION")]
    only: Vec<String>,
}

struct Step {
    name: String,
    program: OsString,
    args: Vec<OsString>,
    env: BTreeMap<String, OsString>,
    partition: &'static str,
}

impl Step {
    fn new(
        name: impl Into<String>,
        program: impl Into<OsString>,
        args: &[&str],
        partition: &'static str,
    ) -> Self {
        Self {
            name: name.into(),
            program: program.into(),
            args: args.iter().map(OsString::from).collect(),
            env: BTreeMap::new(),
            partition,
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
        partition: &'static str,
    ) -> Self {
        Self {
            name: name.into(),
            program: program.into(),
            args,
            env: BTreeMap::new(),
            partition,
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
            failures.push(format!("{} [{}]: {err:#}", step.name, step.partition));
        }
    }

    // E2E is opt-in and independent of `--only` partitions.
    if args.e2e {
        match build_e2e_step(&root) {
            Ok(step) => {
                if let Err(err) = run_step(&root, &step) {
                    failures.push(format!("{} [{}]: {err:#}", step.name, step.partition));
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

fn partition_selected(args: &CiArgs, partition: &str) -> bool {
    if args.only.is_empty() {
        return true;
    }
    args.only.iter().any(|p| p == partition)
}

fn build_steps(root: &Path, args: &CiArgs) -> Result<Vec<Step>> {
    if !args.only.is_empty() {
        for name in &args.only {
            if !PARTITIONS.contains(&name.as_str()) {
                bail!(
                    "unknown CI partition `{name}`; expected one of: {}",
                    PARTITIONS.join(", ")
                );
            }
        }
    }

    let mut steps = Vec::new();

    if partition_selected(args, "lint") {
        steps.push(Step::with_args(
            "actionlint",
            "actionlint",
            actionlint_args(root)?,
            "lint",
        ));
        steps.push(cargo("fmt", &["fmt", "--check"], "lint"));
        steps.push(cargo(
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
            "lint",
        ));
        steps.push(cargo_xtask("lint", &["lint", "--strict"], "lint"));
    }

    if partition_selected(args, "tests") {
        steps.push(cargo(
            "check",
            &["check", "--workspace", "--all-targets", "--locked"],
            "tests",
        ));
        steps.push(cargo(
            "nextest",
            &[
                "nextest",
                "run",
                "--workspace",
                "--all-features",
                "--locked",
            ],
            "tests",
        ));
        steps.push(cargo(
            "doctest",
            &["test", "--doc", "--workspace", "--locked"],
            "tests",
        ));
    }

    if partition_selected(args, "policy") {
        steps.push(cargo("audit", &["audit"], "policy"));
        steps.push(cargo(
            "deny",
            &["deny", "check", "advisories", "bans", "licenses", "sources"],
            "policy",
        ));
        steps.push(cargo_xtask(
            "schema-check",
            &["schema-check", "--base", args.base.as_str()],
            "policy",
        ));
        steps.push(cargo("shear", &["shear", "--deny-warnings"], "policy"));
    }

    if partition_selected(args, "msrv") {
        steps.push(
            cargo(
                "msrv",
                &["check", "--workspace", "--all-targets", "--locked"],
                "msrv",
            )
            .with_env("RUSTUP_TOOLCHAIN", msrv_version(root)?)
            .with_env(
                "CARGO_TARGET_DIR",
                root.join("target/msrv").into_os_string(),
            ),
        );
    }

    // Full powerset is the non-`--fast` default step; also selectable via `--only powerset`.
    let want_powerset = if args.only.is_empty() {
        !args.fast
    } else {
        partition_selected(args, "powerset")
    };
    if want_powerset {
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
            "powerset",
        ));
    }

    if partition_selected(args, "docs") {
        steps.push(cargo_xtask("roadmap audit", &["roadmap", "audit"], "docs"));
        steps.push(cargo_xtask(
            "docs repo-links",
            &["docs", "repo-links"],
            "docs",
        ));
        steps.push(cargo_xtask(
            "research check",
            &["research", "check"],
            "docs",
        ));
    }

    if partition_selected(args, "snapshots") {
        steps.push(cargo(
            "snapshots",
            &[
                "nextest",
                "run",
                "-p",
                "jackin-capsule",
                "-p",
                "jackin-console",
                "--locked",
            ],
            "snapshots",
        ));
    }

    Ok(steps)
}

fn cargo(name: &str, args: &[&str], partition: &'static str) -> Step {
    Step::new(format!("cargo {name}"), "cargo", args, partition)
}

fn cargo_xtask(name: &str, args: &[&str], partition: &'static str) -> Step {
    let mut cargo_args = vec!["xtask"];
    cargo_args.extend_from_slice(args);
    cargo(name, &cargo_args, partition)
}

fn actionlint_args(root: &Path) -> Result<Vec<OsString>> {
    let workflows = root.join(".github/workflows");
    if !workflows.is_dir() {
        bail!("{} does not exist", workflows.display());
    }
    let mut files = Vec::new();
    for entry in crate::fs_util::read_dir_sorted(&workflows)? {
        let path = entry.path();
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
    run_step(
        root,
        &Step::new("docker preflight", "docker", &["info"], "e2e"),
    )
    .context(
        "Docker daemon is not reachable; start Docker before running `cargo xtask ci --e2e`",
    )?;

    let export = output_step(
        root,
        &cargo(
            "build-jackin-capsule export",
            &["run", "--bin", "build-jackin-capsule", "--", "--export"],
            "e2e",
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
        "e2e",
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
    let mut cmd = crate::cmd::command(&step.program);
    cmd.args(&step.args).current_dir(root).envs(&step.env);
    crate::cmd::run(&mut cmd).with_context(|| format!("step {}", step.name))
}

fn output_step(root: &Path, step: &Step) -> Result<String> {
    emit(&format!("==> {}", display_step(step)));
    let mut cmd = crate::cmd::command(&step.program);
    cmd.args(&step.args).current_dir(root).envs(&step.env);
    crate::cmd::output_string(&mut cmd).with_context(|| format!("step {}", step.name))
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

/// Expose step names for tests without running them.
#[cfg(test)]
#[expect(
    dead_code,
    reason = "test helper reserved for partition/--only coverage"
)]
fn step_names(args: &CiArgs) -> Result<Vec<String>> {
    let root = repo_root()?;
    Ok(build_steps(&root, args)?
        .into_iter()
        .map(|s| s.name)
        .collect())
}

/// Expose partitions for tests.
#[cfg(test)]
#[expect(
    dead_code,
    reason = "test helper reserved for partition/--only coverage"
)]
fn step_partitions(args: &CiArgs) -> Result<Vec<&'static str>> {
    let root = repo_root()?;
    Ok(build_steps(&root, args)?
        .into_iter()
        .map(|s| s.partition)
        .collect())
}

#[cfg(test)]
mod tests;
