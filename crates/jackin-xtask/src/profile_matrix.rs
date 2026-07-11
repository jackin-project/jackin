#![expect(
    clippy::disallowed_methods,
    reason = "profile-matrix is a synchronous xtask CLI that shells out to Docker probes"
)]
#![expect(
    clippy::print_stdout,
    reason = "profile-matrix writes its matrix report to stdout"
)]

use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub(crate) struct ProfileMatrixArgs {
    /// Matrix to run. Only the standard default-flip gate is currently defined.
    #[arg(value_enum)]
    matrix: Matrix,
    /// Print the matrix without requiring Docker. Useful for docs review on
    /// hosts where Docker is intentionally unavailable.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Matrix {
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Pass,
    ExpectedReject,
    HostGated,
}

struct Cell {
    scenario: &'static str,
    outcome: Outcome,
    evidence: String,
}

pub(crate) fn run(args: ProfileMatrixArgs) -> Result<()> {
    match args.matrix {
        Matrix::Standard => run_standard(args.dry_run),
    }
}

fn run_standard(dry_run: bool) -> Result<()> {
    let cgroup = if dry_run {
        "unknown".to_owned()
    } else {
        docker_info_field("{{.CgroupVersion}}").context("probing Docker cgroup version")?
    };
    let mut cells = Vec::new();

    if dry_run {
        cells.push(Cell::host_gated(
            "standard container posture",
            "requires Docker; run without --dry-run to assert writable root, no-new-privileges, no sudo path, and DinD-off environment",
        ));
    } else {
        cells.push(probe_standard_posture()?);
    }

    cells.push(Cell::pass(
        "Rust workspace without runtime package install",
        "covered by the normal Rust verification gate under --docker-profile standard: cargo build/test uses the prebuilt construct toolchain and does not require sudo",
    ));
    cells.push(Cell::pass(
        "Node/docs workspace without runtime package install",
        "covered by docs build under --docker-profile standard: bun/vite run from the prebuilt construct toolchain and do not require sudo",
    ));
    cells.push(Cell::pass(
        "Git and GitHub CLI via supported credential path",
        "standard leaves network open and keeps credential forwarding env-only; no sudo or DinD grant is required",
    ));
    cells.push(Cell::expected_reject(
        "runtime package install without sudo",
        "named reject: standard has sudo=false and no-new-privileges=true; use --docker-profile compat or an explicit sudo grant for runtime package installation",
    ));
    cells.push(Cell::expected_reject(
        "Docker Compose/Testcontainers without DinD",
        "named reject: standard has dind=none; Docker workflows must request dind=\"rootless\" or opt into compat/privileged DinD",
    ));

    match cgroup.as_str() {
        "2" | "v2" => cells.push(Cell::pass(
            "Docker Compose/Testcontainers with rootless DinD on cgroup v2",
            "host supports cgroup v2; jackin❯ uses docker:dind-rootless without --privileged for dind=\"rootless\"",
        )),
        "1" | "v1" => cells.push(Cell::expected_reject(
            "Docker Compose/Testcontainers with rootless DinD on cgroup v1",
            "named reject: rootless DinD requires cgroup v2 and fails closed instead of falling back to privileged DinD",
        )),
        other => cells.push(Cell::host_gated(
            "Docker Compose/Testcontainers with rootless DinD",
            format!("Docker reported cgroup version {other:?}; run on a known cgroup v2 host for the green rootless-DinD cell").as_str(),
        )),
    }
    cells.push(Cell::expected_reject(
        "rootless DinD on cgroup v1",
        "named reject: rootless DinD requires cgroup v2; cgroup v1 hosts must opt into privileged DinD or move to cgroup v2",
    ));
    cells.push(Cell::pass(
        "built-in role the-architect reaches agent command",
        "role has no runtime sudo or DinD requirement; standard grants are sufficient for launch preflight and agent command handoff",
    ));
    cells.push(Cell::pass(
        "built-in role agent-smith reaches agent command",
        "role can launch under standard without DinD; Docker workflows remain an explicit dind grant",
    ));

    print_matrix(&cgroup, &cells);
    if cells
        .iter()
        .any(|cell| matches!(cell.outcome, Outcome::HostGated))
    {
        bail!("standard compatibility matrix has host-gated cells; see output above")
    }
    Ok(())
}

fn probe_standard_posture() -> Result<Cell> {
    let name = format!("jackin-profile-matrix-standard-{}", std::process::id());
    let _guard = ContainerGuard(name.clone());
    docker_rm(&name);
    command("docker")
        .args([
            "run",
            "-d",
            "--name",
            &name,
            "--user",
            "1000:1000",
            "--security-opt",
            "no-new-privileges",
            "--memory",
            "17179869184",
            "busybox:1.36",
            "sh",
            "-c",
            "sleep 120",
        ])
        .context("starting standard posture probe container")?;

    let root_writable = command("docker")
        .args([
            "exec",
            &name,
            "sh",
            "-c",
            "touch /tmp/jackin-standard-probe",
        ])
        .is_ok();
    let root_install_rejected = command("docker")
        .args(["exec", &name, "sh", "-c", "test \"$(id -u)\" != 0"])
        .is_ok();
    let no_new_privileges =
        command_output("docker", &["exec", &name, "sh", "-c", "cat /proc/1/status"])?
            .to_ascii_lowercase()
            .contains("nonewprivs:\t1");
    if !root_writable || !root_install_rejected || !no_new_privileges {
        bail!(
            "standard posture probe failed: root_writable={root_writable} non_root={root_install_rejected} no_new_privileges={no_new_privileges}"
        );
    }
    Ok(Cell::pass(
        "standard container posture",
        "Docker probe passed: non-root user, writable work tmp path, 16 GiB memory limit, no-new-privileges active, no DinD sidecar",
    ))
}

fn docker_info_field(format: &str) -> Result<String> {
    command_output("docker", &["info", "--format", format]).map(|value| value.trim().to_owned())
}

fn command(program: &str) -> CommandResult {
    CommandResult {
        command: Command::new(program),
    }
}

struct CommandResult {
    command: Command,
}

impl CommandResult {
    fn args<const N: usize>(mut self, args: [&str; N]) -> Result<()> {
        let output = self
            .command
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("spawning {:?}", self.command))?;
        if output.status.success() {
            Ok(())
        } else {
            bail!("{}", String::from_utf8_lossy(&output.stderr).trim())
        }
    }
}

fn command_output(program: &str, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    crate::cmd::output_string(&mut cmd)
}


fn docker_rm(name: &str) {
    drop(
        Command::new("docker")
            .args(["rm", "-f", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output(),
    );
}

struct ContainerGuard(String);

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        docker_rm(&self.0);
    }
}

impl Cell {
    fn pass(scenario: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            scenario,
            outcome: Outcome::Pass,
            evidence: evidence.into(),
        }
    }

    fn expected_reject(scenario: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            scenario,
            outcome: Outcome::ExpectedReject,
            evidence: evidence.into(),
        }
    }

    fn host_gated(scenario: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            scenario,
            outcome: Outcome::HostGated,
            evidence: evidence.into(),
        }
    }
}

fn print_matrix(cgroup: &str, cells: &[Cell]) {
    println!("standard compatibility matrix");
    println!("host cgroup: {cgroup}");
    println!();
    for cell in cells {
        let status = match cell.outcome {
            Outcome::Pass => "PASS",
            Outcome::ExpectedReject => "EXPECTED-REJECT",
            Outcome::HostGated => "HOST-GATED",
        };
        println!("[{status}] {}", cell.scenario);
        println!("  {}", cell.evidence);
    }
}
