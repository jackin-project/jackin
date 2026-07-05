//! Pre-flight health checks for `jackin doctor` and load/hardline dispatch.
//!
//! Each check runs asynchronously and returns a `CheckResult` with a status
//! and a human-readable hint for fixing failures. The `preflight` function
//! runs a slice of check names, fails on any `Fail`, and returns `Ok(())`
//! when all pass or warn.
use jackin_docker::docker_client::DockerApi;
use owo_colors::OwoColorize;
use std::path::Path;

#[cfg(test)]
mod tests;

/// Status of a single doctor check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckStatus {
    Ok,
    Warn,
    Fail,
    Skip,
}

impl CheckStatus {
    pub(crate) const fn symbol(self) -> &'static str {
        match self {
            Self::Ok => "ok  ",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

/// Result of one doctor check.
#[derive(Debug, Clone)]
pub(crate) struct CheckResult {
    pub(crate) name: &'static str,
    pub(crate) status: CheckStatus,
    pub(crate) message: String,
    pub(crate) hint: Option<String>,
}

impl CheckResult {
    fn ok(name: &'static str, msg: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Ok,
            message: msg.into(),
            hint: None,
        }
    }

    fn warn(name: &'static str, msg: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Warn,
            message: msg.into(),
            hint: Some(hint.into()),
        }
    }

    fn fail(name: &'static str, msg: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            message: msg.into(),
            hint: Some(hint.into()),
        }
    }

    fn skip(name: &'static str, msg: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Skip,
            message: msg.into(),
            hint: None,
        }
    }
}

/// Named checks that can be run individually or in a preflight slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckName {
    DockerDaemon,
    DockerVersion,
    DiskSpace,
    ConfigDir,
    JackinDir,
    CapsuleBinaryCache,
    GhAuth,
    OpCli,
    Mise,
    ClockSkew,
    OrphanedContainers,
    StaleIsolation,
}

impl CheckName {
    pub(crate) const fn all() -> &'static [Self] {
        &[
            Self::DockerDaemon,
            Self::DockerVersion,
            Self::DiskSpace,
            Self::ConfigDir,
            Self::JackinDir,
            Self::CapsuleBinaryCache,
            Self::GhAuth,
            Self::OpCli,
            Self::Mise,
            Self::ClockSkew,
            Self::OrphanedContainers,
            Self::StaleIsolation,
        ]
    }

    /// Minimal set of checks run as a pre-flight gate before `load` / `hardline`.
    pub(crate) const fn preflight_required() -> &'static [Self] {
        &[
            Self::DockerDaemon,
            Self::DiskSpace,
            Self::ConfigDir,
            Self::JackinDir,
        ]
    }
}

/// Run a single check and return its result.
pub(crate) async fn run_check(check: CheckName, paths: &jackin_core::JackinPaths) -> CheckResult {
    match check {
        CheckName::DockerDaemon => check_docker_daemon().await,
        CheckName::DockerVersion => report_docker_version().await,
        CheckName::DiskSpace => check_disk_space(paths),
        CheckName::ConfigDir => check_config_dir(&paths.config_dir),
        CheckName::JackinDir => check_jackin_dir(&paths.data_dir),
        CheckName::CapsuleBinaryCache => check_capsule_cache(paths),
        CheckName::GhAuth => check_gh_auth(),
        CheckName::OpCli => check_op_cli(),
        CheckName::Mise => check_mise(),
        CheckName::ClockSkew => check_clock_skew(),
        CheckName::OrphanedContainers => check_orphaned_containers(paths).await,
        CheckName::StaleIsolation => check_stale_isolation(paths),
    }
}

/// Run a pre-flight subset and fail with an error if any check fails.
///
/// Warnings are printed but do not block execution. Failures print the hint
/// and return `Err`.
pub(crate) async fn preflight(
    checks: &[CheckName],
    paths: &jackin_core::JackinPaths,
) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    for &check in checks {
        let result = run_check(check, paths).await;
        match result.status {
            CheckStatus::Fail => {
                eprintln!(
                    "{} {} — {}",
                    "[fail]".red().bold(),
                    result.name,
                    result.message
                );
                if let Some(hint) = &result.hint {
                    eprintln!("       {}", hint.dimmed());
                }
                failures.push(result.name);
            }
            CheckStatus::Warn => {
                eprintln!(
                    "{} {} — {}",
                    "[warn]".yellow().bold(),
                    result.name,
                    result.message
                );
                if let Some(hint) = &result.hint {
                    eprintln!("       {}", hint.dimmed());
                }
            }
            _ => {}
        }
    }
    if !failures.is_empty() {
        // Emit a structured JackinError for Docker-daemon failures so the
        // E001 error block fires at the entry point.
        if failures.contains(&"docker_daemon") {
            return Err(crate::error::JackinError::DockerDaemonUnreachable {
                source: anyhow::anyhow!(
                    "pre-flight checks failed: {}. Run `jackin doctor` for details.",
                    failures.join(", ")
                ),
            }
            .into());
        }
        anyhow::bail!(
            "pre-flight checks failed: {}. Run `jackin doctor` for details.",
            failures.join(", ")
        );
    }
    Ok(())
}

// ── Individual check implementations ────────────────────────────────────────

async fn check_docker_daemon() -> CheckResult {
    match jackin_docker::docker_client::BollardDockerClient::connect() {
        Ok(docker) => {
            // Verify the daemon is actually reachable without scanning containers.
            match docker.ping().await {
                Ok(()) => CheckResult::ok("docker_daemon", "Docker daemon reachable"),
                Err(e) => CheckResult::fail(
                    "docker_daemon",
                    format!("Docker daemon connected but not responding: {e:#}"),
                    "Start Docker Desktop / OrbStack / run `colima start` / check `DOCKER_HOST`",
                ),
            }
        }
        Err(e) => CheckResult::fail(
            "docker_daemon",
            format!("Cannot connect to Docker daemon: {e:#}"),
            "Start Docker Desktop / OrbStack / run `colima start` / check `DOCKER_HOST`",
        ),
    }
}

async fn report_docker_version() -> CheckResult {
    // Use `docker version --format '{{.Server.Version}}'` via subprocess since
    // we don't expose a version endpoint on the DockerApi trait.
    let output = tokio::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            docker_version_report_result(&version)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            docker_version_command_failure_result(out.status.code(), &stderr)
        }
        Err(e) => CheckResult::skip(
            "docker_version",
            format!("docker CLI not found or could not spawn: {e}"),
        ),
    }
}

fn docker_version_report_result(raw_version: &str) -> CheckResult {
    let version = raw_version.trim();
    if version.is_empty() {
        return CheckResult::warn(
            "docker_version",
            "Docker server returned an empty version string",
            "Start Docker and ensure the Docker CLI can reach the daemon",
        );
    }
    CheckResult::ok("docker_version", format!("Docker server {version}"))
}

fn docker_version_command_failure_result(exit_code: Option<i32>, stderr: &str) -> CheckResult {
    let msg = if stderr.trim().is_empty() {
        format!(
            "Could not read Docker server version (exit {})",
            exit_code.unwrap_or(-1)
        )
    } else {
        format!("Could not read Docker server version: {}", stderr.trim())
    };
    CheckResult::warn(
        "docker_version",
        msg,
        "Start Docker and ensure the Docker CLI can reach the daemon",
    )
}

fn check_disk_space(paths: &jackin_core::JackinPaths) -> CheckResult {
    // Check that the jackin data directory has ≥1 GiB free.
    let dir = paths.data_dir.parent().unwrap_or(&paths.data_dir);
    match available_bytes(dir) {
        Some(bytes) if bytes < 1_073_741_824 => CheckResult::warn(
            "disk_space",
            format!(
                "Low disk space: {} MiB free in {}",
                bytes / 1_048_576,
                dir.display()
            ),
            "Run `jackin prune` or `docker system prune` to reclaim space",
        ),
        Some(bytes) => CheckResult::ok("disk_space", format!("{} MiB free", bytes / 1_048_576)),
        None => CheckResult::skip("disk_space", "Could not determine disk space"),
    }
}

fn check_config_dir(config_dir: &Path) -> CheckResult {
    if !config_dir.exists() {
        return CheckResult::warn(
            "config_dir",
            format!("{} does not exist", config_dir.display()),
            format!("Run: mkdir -p {}", config_dir.display()),
        );
    }
    // Check writability by attempting a temp file.
    let probe = config_dir.join(".jackin_probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            drop(std::fs::remove_file(&probe));
            CheckResult::ok(
                "config_dir",
                format!("{} exists and is writable", config_dir.display()),
            )
        }
        Err(e) => CheckResult::fail(
            "config_dir",
            format!("{} is not writable: {e}", config_dir.display()),
            format!("Run: chmod 700 {}", config_dir.display()),
        ),
    }
}

fn check_jackin_dir(data_dir: &Path) -> CheckResult {
    let jackin_dir = data_dir.parent().unwrap_or(data_dir);
    if !jackin_dir.exists() {
        return CheckResult::warn(
            "jackin_dir",
            format!("{} does not exist", jackin_dir.display()),
            format!(
                "Run: mkdir -p {} && chmod 700 {}",
                jackin_dir.display(),
                jackin_dir.display()
            ),
        );
    }
    CheckResult::ok("jackin_dir", format!("{} exists", jackin_dir.display()))
}

fn check_capsule_cache(paths: &jackin_core::JackinPaths) -> CheckResult {
    let version = jackin_image::capsule_binary::REQUIRED_VERSION;
    let arch = std::env::consts::ARCH;
    let cached = jackin_image::capsule_binary::cached_binary_path(&paths.cache_dir, version, arch);
    if cached.exists() {
        CheckResult::ok(
            "capsule_cache",
            format!("jackin-capsule {version} cached at {}", cached.display()),
        )
    } else {
        CheckResult::warn(
            "capsule_cache",
            format!("jackin-capsule {version} not in cache — will download on next load"),
            format!(
                "Run `jackin load` to trigger download, or clear stale cache: rm -rf {}",
                paths.cache_dir.join("jackin-capsule").display()
            ),
        )
    }
}

fn check_gh_auth() -> CheckResult {
    let mut command = std::process::Command::new("gh");
    command.args(["auth", "status"]);
    #[expect(
        clippy::disallowed_methods,
        reason = "preflight probes run before TUI render/runtime work begins"
    )]
    match command.output() {
        Ok(out) if out.status.success() => CheckResult::ok("gh_auth", "gh CLI authenticated"),
        Ok(_) => CheckResult::warn(
            "gh_auth",
            "gh CLI not authenticated",
            "Run `gh auth login` to authenticate (required for GitHub auth-forward)",
        ),
        Err(_) => CheckResult::skip("gh_auth", "gh CLI not found"),
    }
}

fn check_op_cli() -> CheckResult {
    let mut command = std::process::Command::new("op");
    command.args(["account", "list", "--format=json"]);
    #[expect(
        clippy::disallowed_methods,
        reason = "preflight probes run before TUI render/runtime work begins"
    )]
    match command.output() {
        Ok(out) if out.status.success() => CheckResult::ok("op_cli", "1Password CLI signed in"),
        Ok(_) => CheckResult::skip(
            "op_cli",
            "op CLI not signed in (only needed if workspaces reference op:// secrets)",
        ),
        Err(_) => CheckResult::skip(
            "op_cli",
            "op CLI not found (only needed if workspaces reference op:// secrets)",
        ),
    }
}

fn check_mise() -> CheckResult {
    // mise is only required in source checkouts, not for installed binaries.
    let in_checkout = Path::new(".mise.toml").exists() || Path::new(".tool-versions").exists();
    if !in_checkout {
        return CheckResult::skip("mise", "Not in a source checkout — mise not required");
    }
    let mut command = std::process::Command::new("mise");
    command.arg("--version");
    #[expect(
        clippy::disallowed_methods,
        reason = "preflight probes run before TUI render/runtime work begins"
    )]
    match command.output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            CheckResult::ok("mise", format!("mise {version}"))
        }
        Ok(_) => CheckResult::warn(
            "mise",
            "mise returned a non-zero exit code (installation may be broken)",
            "Check `mise --version` manually; reinstall if corrupted",
        ),
        Err(_) => CheckResult::warn(
            "mise",
            "mise not found in PATH",
            "Run: curl https://mise.run | sh",
        ),
    }
}

fn check_clock_skew() -> CheckResult {
    use std::time::{SystemTime, UNIX_EPOCH};
    // We can only check the local clock against itself; a meaningful skew
    // check requires an NTP query. For now, verify the system clock is after
    // late 2023 (Unix epoch 1,700,000,000 ≈ November 2023) as a sanity
    // check for wildly wrong clocks.
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => {
            return CheckResult::fail(
                "clock_skew",
                "System clock is set before the Unix epoch (pre-1970)",
                "Sync your system clock via NTP",
            );
        }
    };
    if now < 1_700_000_000 {
        CheckResult::fail(
            "clock_skew",
            format!("System clock appears wrong (UNIX epoch: {now})"),
            "Sync your system clock via NTP",
        )
    } else {
        CheckResult::ok("clock_skew", "System clock looks reasonable")
    }
}

async fn check_orphaned_containers(paths: &jackin_core::JackinPaths) -> CheckResult {
    use jackin_docker::docker_client::{BollardDockerClient, DockerApi};
    use jackin_runtime::runtime::naming::LABEL_MANAGED;

    let Ok(docker) = BollardDockerClient::connect() else {
        return CheckResult::skip("orphaned_containers", "Docker unavailable");
    };
    let Ok(containers) = docker.list_containers(&[LABEL_MANAGED], true).await else {
        return CheckResult::skip("orphaned_containers", "Could not list containers");
    };
    let Ok(index) =
        jackin_runtime::instance::manifest::InstanceIndex::read_or_rebuild(&paths.data_dir)
    else {
        return CheckResult::skip("orphaned_containers", "Could not read instance index");
    };
    let known: std::collections::HashSet<&str> = index
        .instances
        .iter()
        .map(|e| e.container_base.as_str())
        .collect();
    let orphans: Vec<_> = containers
        .iter()
        .filter(|c| !c.name.is_empty() && !known.contains(c.name.as_str()))
        .map(|c| c.name.as_str())
        .collect();
    if orphans.is_empty() {
        CheckResult::ok("orphaned_containers", "No orphaned containers found")
    } else {
        CheckResult::warn(
            "orphaned_containers",
            format!(
                "{} orphaned container(s): {}",
                orphans.len(),
                orphans.join(", ")
            ),
            "Run `jackin prune orphaned` to remove them",
        )
    }
}

fn check_stale_isolation(paths: &jackin_core::JackinPaths) -> CheckResult {
    // Look for isolation.json files that reference worktrees that no longer exist.
    use jackin_runtime::isolation::state::read_records;

    let state_dir = &paths.data_dir;
    let mut stale = 0usize;
    if let Ok(entries) = std::fs::read_dir(state_dir) {
        for entry in entries.flatten() {
            if let Ok(records) = read_records(&entry.path()) {
                stale += records
                    .iter()
                    .filter(|record| !Path::new(&record.worktree_path).exists())
                    .count();
            }
        }
    }
    if stale == 0 {
        CheckResult::ok("stale_isolation", "No stale isolation.json files found")
    } else {
        CheckResult::warn(
            "stale_isolation",
            format!("{stale} stale isolation.json file(s) pointing at vanished worktrees"),
            "Run `jackin prune isolation` to clean up",
        )
    }
}

// ── Disk space ────────────────────────────────────────────────────────────────

/// Return available bytes for the filesystem containing `path`.
fn available_bytes(path: &Path) -> Option<u64> {
    fs4::available_space(path).ok()
}
