//! Developer tooling for local jackin pull request verification.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::Value;

const DEFAULT_REPO: &str = "jackin-project/jackin";
const REPO_DIR_NAME: &str = "jackin";
// Locked to the `construct-build-local` default tag composed from
// `LOCAL_REGISTRY_IMAGE`/`STABLE_TAG` in `jackin-xtask/src/construct.rs`; if
// those defaults move, or are overridden in the environment, the exported
// `JACKIN_CONSTRUCT_IMAGE` drifts from the image the build actually produces.
const CONSTRUCT_IMAGE: &str = "jackin-local/construct:trixie";

#[derive(Parser)]
#[command(name = "jackin-dev", about = "Developer tooling for jackin")]
struct Cli {
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Subcommand)]
enum TopCommand {
    /// Pull request verification helpers.
    #[command(subcommand)]
    Pr(PrCommand),
}

#[derive(Subcommand)]
enum PrCommand {
    /// Clone or refresh a PR checkout and prepare its isolated environment.
    Sync(SyncArgs),
    /// Remove a PR verification bundle.
    Clean(PrPathArgs),
    /// Print the shell commands for entering a PR verification bundle.
    Env(PrPathArgs),
    /// Print the PR verification bundle path.
    Path(PrPathArgs),
    /// Show local checkout/env freshness for a PR verification bundle.
    Status(PrRepoArgs),
}

/// Fields shared by every command that resolves a PR against a remote repo.
#[derive(Args)]
struct PrRepoArgs {
    /// GitHub pull request number.
    pr: u64,
    /// Repository in owner/name form.
    #[arg(long, default_value = DEFAULT_REPO)]
    repo: String,
    /// PR test root. Defaults to ~/Projects/jackin-project/test/pr-<number>.
    #[arg(long)]
    test_dir: Option<PathBuf>,
}

#[derive(Args)]
struct SyncArgs {
    #[command(flatten)]
    common: PrRepoArgs,
    /// Isolated config source for `JACKIN_CONFIG_DIR`.
    #[arg(long, value_enum, default_value_t = ConfigSource::Copy)]
    config: ConfigSource,
}

/// Fields for local-only commands; these never touch the remote, so no `--repo`.
#[derive(Args)]
struct PrPathArgs {
    /// GitHub pull request number.
    pr: u64,
    /// PR test root. Defaults to ~/Projects/jackin-project/test/pr-<number>.
    #[arg(long)]
    test_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ConfigSource {
    /// Start with an empty PR-scoped config directory.
    Blank,
    /// Copy ~/.config/jackin into the PR-scoped config directory.
    Copy,
}

#[derive(Debug)]
struct PrPaths {
    root: PathBuf,
    repo: PathBuf,
    env_file: PathBuf,
    config: PathBuf,
    home: PathBuf,
}

impl PrPaths {
    fn new(pr: u64, test_dir: Option<PathBuf>) -> Result<Self> {
        let root = match test_dir {
            Some(test_dir) => test_dir,
            None => home_dir()?.join(format!("Projects/jackin-project/test/pr-{pr}")),
        };
        Ok(Self::from_root(root))
    }

    fn from_root(root: PathBuf) -> Self {
        let repo = root.join(REPO_DIR_NAME);
        let env_file = root.join("env.sh");
        let state = root.join("state");
        let config = state.join("config");
        let home = state.join("home");
        Self {
            root,
            repo,
            env_file,
            config,
            home,
        }
    }
}

#[derive(Debug)]
struct PullRequestInfo {
    head_ref_name: String,
    head_oid: String,
    changed_files: Vec<String>,
}

#[derive(Debug)]
struct AutoPrep {
    capsule: bool,
    construct: bool,
}

#[derive(Debug)]
struct WorkspacePackage {
    name: String,
    root: PathBuf,
    dependencies: Vec<String>,
}

fn main() -> std::process::ExitCode {
    match run(Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            #[expect(
                clippy::print_stderr,
                reason = "jackin-dev is a CLI; errors belong on stderr"
            )]
            {
                eprintln!("error: {err:#}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        TopCommand::Pr(command) => match command {
            PrCommand::Sync(args) => sync(args),
            PrCommand::Clean(args) => clean(args),
            PrCommand::Env(args) => print_env(args),
            PrCommand::Path(args) => print_path(args),
            PrCommand::Status(args) => status(args),
        },
    }
}

fn sync(args: SyncArgs) -> Result<()> {
    let paths = PrPaths::new(args.common.pr, args.common.test_dir)?;
    let home = home_dir()?;
    let pr = pr_info(args.common.pr, &args.common.repo)?;

    fs::create_dir_all(&paths.root)
        .with_context(|| format!("creating {}", paths.root.display()))?;
    checkout_repo(
        &args.common.repo,
        args.common.pr,
        &pr.head_ref_name,
        &paths.repo,
    )?;
    run_checked(command("mise", ["trust"]).current_dir(&paths.repo))?;
    run_checked(command("mise", ["install"]).current_dir(&paths.repo))?;
    run_checked(command("cargo", ["build", "--bin", "jackin"]).current_dir(&paths.repo))?;

    prepare_config(args.config, &paths.config, &home)?;
    fs::create_dir_all(&paths.home)
        .with_context(|| format!("creating {}", paths.home.display()))?;

    let auto = auto_prep(&paths.repo, &pr.changed_files)?;

    let mut env_lines = env_lines(&paths);

    if auto.construct {
        run_checked(command("mise", ["run", "construct-build-local"]).current_dir(&paths.repo))?;
        env_lines.push(format!("export JACKIN_CONSTRUCT_IMAGE={CONSTRUCT_IMAGE}"));
    }

    if auto.capsule {
        env_lines.push(build_capsule_export(&paths.repo)?);
    }

    fs::write(&paths.env_file, format!("{}\n", env_lines.join("\n")))
        .with_context(|| format!("writing {}", paths.env_file.display()))?;
    print_sync_summary(args.common.pr, &paths, &pr, &auto);
    Ok(())
}

fn clean(args: PrPathArgs) -> Result<()> {
    let paths = PrPaths::new(args.pr, args.test_dir)?;
    if paths.root.exists() {
        fs::remove_dir_all(&paths.root)
            .with_context(|| format!("removing {}", paths.root.display()))?;
        emit_line(format!("Removed {}", paths.root.display()));
    } else {
        emit_line(format!("No PR bundle at {}", paths.root.display()));
    }
    Ok(())
}

fn print_env(args: PrPathArgs) -> Result<()> {
    let paths = PrPaths::new(args.pr, args.test_dir)?;
    for line in enter_lines(&paths) {
        emit_line(line);
    }
    Ok(())
}

/// Shell commands an operator runs to enter a synced PR bundle.
fn enter_lines(paths: &PrPaths) -> [String; 3] {
    [
        format!("cd {}", shell_quote(paths.repo.as_os_str())),
        format!("source {}", shell_quote(paths.env_file.as_os_str())),
        "which jackin".to_owned(),
    ]
}

fn print_path(args: PrPathArgs) -> Result<()> {
    let paths = PrPaths::new(args.pr, args.test_dir)?;
    emit_line(paths.root.display().to_string());
    Ok(())
}

fn status(args: PrRepoArgs) -> Result<()> {
    let paths = PrPaths::new(args.pr, args.test_dir)?;
    let pr = pr_info(args.pr, &args.repo)?;
    let (local_head, local_branch) = if paths.repo.join(".git").exists() {
        (
            Some(git_output(&paths.repo, ["rev-parse", "HEAD"])?),
            Some(git_output(&paths.repo, ["branch", "--show-current"])?),
        )
    } else {
        (None, None)
    };
    let fresh = local_head.as_deref() == Some(pr.head_oid.as_str());

    emit_line(format!("PR: #{}", args.pr));
    emit_line(format!("root: {}", paths.root.display()));
    emit_line(format!("repo: {}", paths.repo.display()));
    emit_line(format!(
        "branch: {}",
        local_branch.as_deref().unwrap_or("<missing>")
    ));
    emit_line(format!(
        "local head: {}",
        local_head.as_deref().unwrap_or("<missing>")
    ));
    emit_line(format!("remote head: {}", pr.head_oid));
    emit_line(format!("fresh: {}", yes_no(fresh)));
    emit_line(format!("env: {}", exists_label(&paths.env_file)));
    emit_line(format!("config: {}", exists_label(&paths.config)));
    emit_line(format!("home: {}", exists_label(&paths.home)));
    Ok(())
}

fn pr_info(pr: u64, repo: &str) -> Result<PullRequestInfo> {
    let mut cmd = command(
        "gh",
        [
            "pr",
            "view",
            &pr.to_string(),
            "--repo",
            repo,
            "--json",
            "headRefName,headRefOid",
        ],
    );
    let output = run_output(&mut cmd)?;
    let json: Value = serde_json::from_slice(&output).context("parsing gh pr view JSON")?;
    let (head_ref_name, head_oid) = parse_pr_refs(&json)?;

    // `gh pr view --json files` caps at 100 files, so a large PR (e.g. #528 with
    // 113) silently drops changed paths like `docker/construct/*` — downgrading
    // every `auto_prep` build decision to "not needed" and launching against a
    // stale image. `gh pr diff --name-only` lists every changed path, uncapped.
    let mut diff_cmd = command(
        "gh",
        ["pr", "diff", &pr.to_string(), "--repo", repo, "--name-only"],
    );
    let diff_output = run_output(&mut diff_cmd)?;
    let diff_text = String::from_utf8(diff_output)
        .context("`gh pr diff --name-only` output was not valid UTF-8")?;
    let changed_files = parse_changed_files(&diff_text)?;

    Ok(PullRequestInfo {
        head_ref_name,
        head_oid,
        changed_files,
    })
}

fn parse_pr_refs(json: &Value) -> Result<(String, String)> {
    Ok((
        json_string(json, "headRefName")?,
        json_string(json, "headRefOid")?,
    ))
}

// An empty file list is a contract break, not a zero-file PR: silently
// collapsing it to empty would downgrade every `auto_prep` build decision to
// "not needed" and launch the operator against a stale binary/image.
fn parse_changed_files(diff_name_only: &str) -> Result<Vec<String>> {
    let changed_files: Vec<String> = diff_name_only
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect();
    if changed_files.is_empty() {
        bail!("`gh pr diff --name-only` returned no changed files");
    }
    Ok(changed_files)
}

fn json_string(json: &Value, key: &str) -> Result<String> {
    json.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("gh pr view did not return {key}"))
}

fn checkout_repo(repo: &str, pr: u64, head_ref_name: &str, repo_dir: &Path) -> Result<()> {
    if !repo_dir.join(".git").exists() {
        let parent = repo_dir
            .parent()
            .ok_or_else(|| anyhow!("repo dir {} has no parent", repo_dir.display()))?;
        run_checked(
            command(
                "git",
                [
                    "clone",
                    &format!("https://github.com/{repo}.git"),
                    REPO_DIR_NAME,
                ],
            )
            .current_dir(parent),
        )?;
    }

    let head_remote_ref = format!("refs/remotes/origin/{head_ref_name}");
    let head_upstream = format!("origin/{head_ref_name}");
    let fetched_head_branch = run_status(
        command(
            "git",
            [
                "fetch",
                "-f",
                "origin",
                &format!("refs/heads/{head_ref_name}:{head_remote_ref}"),
            ],
        )
        .current_dir(repo_dir),
    )?;

    let (remote_ref, upstream) = if fetched_head_branch {
        (head_remote_ref, head_upstream)
    } else {
        let pr_remote_ref = format!("refs/remotes/origin/pr-{pr}-head");
        run_checked(
            command(
                "git",
                [
                    "fetch",
                    "-f",
                    "origin",
                    &format!("pull/{pr}/head:{pr_remote_ref}"),
                ],
            )
            .current_dir(repo_dir),
        )?;
        (pr_remote_ref, format!("origin/pr-{pr}-head"))
    };

    run_checked(
        command("git", ["checkout", "-B", head_ref_name, &remote_ref]).current_dir(repo_dir),
    )?;
    run_checked(
        command(
            "git",
            ["branch", "--set-upstream-to", &upstream, head_ref_name],
        )
        .current_dir(repo_dir),
    )?;
    Ok(())
}

fn prepare_config(source: ConfigSource, config_dir: &Path, home: &Path) -> Result<()> {
    if config_dir.exists() {
        fs::remove_dir_all(config_dir)
            .with_context(|| format!("removing {}", config_dir.display()))?;
    }
    let source_dir = home.join(".config/jackin");
    match source {
        ConfigSource::Copy if source_dir.exists() => copy_dir_recursive(&source_dir, config_dir),
        ConfigSource::Copy | ConfigSource::Blank => fs::create_dir_all(config_dir)
            .with_context(|| format!("creating {}", config_dir.display())),
    }
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target).with_context(|| format!("creating {}", target.display()))?;
    for entry in fs::read_dir(source).with_context(|| format!("reading {}", source.display()))? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::metadata(&source_path)
            .with_context(|| format!("reading {}", source_path.display()))?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &target_path).with_context(|| {
                format!(
                    "copying {} to {}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        } else {
            bail!("unsupported config entry {}", source_path.display());
        }
    }
    Ok(())
}

fn env_lines(paths: &PrPaths) -> Vec<String> {
    vec![
        format!(
            "export PATH=\"{}:$PATH\"",
            paths.repo.join("target/debug").display()
        ),
        format!(
            "export JACKIN_CONFIG_DIR={}",
            shell_quote(paths.config.as_os_str())
        ),
        format!(
            "export JACKIN_HOME_DIR={}",
            shell_quote(paths.home.as_os_str())
        ),
    ]
}

fn auto_prep(repo_dir: &Path, files: &[String]) -> Result<AutoPrep> {
    Ok(AutoPrep {
        capsule: capsule_build_required(repo_dir, files)?,
        construct: construct_build_required(files),
    })
}

fn construct_build_required(files: &[String]) -> bool {
    files.iter().any(|file| {
        file.starts_with("docker/construct/")
            || file == "docker-bake.hcl"
            || file == "mise.toml"
            || file.starts_with("crates/jackin-xtask/src/construct")
    })
}

fn capsule_build_required(repo_dir: &Path, files: &[String]) -> Result<bool> {
    if files.iter().any(|file| {
        matches!(
            file.as_str(),
            "Cargo.lock" | "Cargo.toml" | "rust-toolchain.toml" | "mise.toml"
        ) || file.starts_with(".cargo/")
    }) {
        return Ok(true);
    }

    let packages = workspace_packages(repo_dir)?;
    let affected = affected_workspace_packages(repo_dir, files, &packages)?;
    if affected.is_empty() {
        return Ok(false);
    }
    let closure = local_dependency_closure(&packages, "jackin-capsule")?;
    Ok(!affected.is_disjoint(&closure))
}

fn workspace_packages(repo_dir: &Path) -> Result<Vec<WorkspacePackage>> {
    let output = run_output(
        command("cargo", ["metadata", "--format-version=1", "--no-deps"]).current_dir(repo_dir),
    )?;
    let json: Value = serde_json::from_slice(&output).context("parsing cargo metadata JSON")?;
    let packages = json
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("cargo metadata did not return a packages array"))?;

    packages
        .iter()
        .map(|package| {
            let name = json_string(package, "name")?;
            let manifest_path = PathBuf::from(json_string(package, "manifest_path")?);
            let root = manifest_path
                .parent()
                .ok_or_else(|| anyhow!("package {name} manifest has no parent"))?
                .to_owned();
            let dependencies = package
                .get("dependencies")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("package {name} missing dependencies array"))?
                .iter()
                .filter(|dependency| {
                    dependency.get("path").and_then(Value::as_str).is_some()
                        && dependency.get("kind").and_then(Value::as_str) != Some("dev")
                })
                .filter_map(|dependency| dependency.get("name").and_then(Value::as_str))
                .map(str::to_owned)
                .collect();
            Ok(WorkspacePackage {
                name,
                root,
                dependencies,
            })
        })
        .collect()
}

fn affected_workspace_packages(
    repo_dir: &Path,
    files: &[String],
    packages: &[WorkspacePackage],
) -> Result<BTreeSet<String>> {
    let mut affected = BTreeSet::new();
    for file in files {
        let file = Path::new(file);
        for package in packages {
            let package_root = package.root.strip_prefix(repo_dir).with_context(|| {
                format!(
                    "package {} root {} is outside repo {}",
                    package.name,
                    package.root.display(),
                    repo_dir.display()
                )
            })?;
            if file.starts_with(package_root) {
                affected.insert(package.name.clone());
            }
        }
    }
    Ok(affected)
}

fn local_dependency_closure(packages: &[WorkspacePackage], root: &str) -> Result<BTreeSet<String>> {
    let by_name: BTreeMap<&str, &WorkspacePackage> = packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();
    let mut closure = BTreeSet::new();
    let mut stack = vec![root.to_owned()];

    while let Some(name) = stack.pop() {
        if !closure.insert(name.clone()) {
            continue;
        }
        let package = by_name
            .get(name.as_str())
            .ok_or_else(|| anyhow!("workspace package {name:?} not found"))?;
        stack.extend(package.dependencies.iter().cloned());
    }

    Ok(closure)
}

// Couples to the `build-jackin-capsule --export` contract: it prints exactly
// one `export JACKIN_CAPSULE_BIN=<path>` line to stdout (build chatter goes to
// stderr). If that output format changes, this match must change with it.
fn build_capsule_export(repo_dir: &Path) -> Result<String> {
    let output = run_output(
        command(
            "cargo",
            ["run", "--bin", "build-jackin-capsule", "--", "--export"],
        )
        .current_dir(repo_dir),
    )?;
    let stdout =
        String::from_utf8(output).context("build-jackin-capsule --export output was not UTF-8")?;
    stdout
        .lines()
        .rev()
        .find(|line| line.starts_with("export JACKIN_CAPSULE_BIN="))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("build-jackin-capsule --export did not print JACKIN_CAPSULE_BIN"))
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| anyhow!("HOME is not set"))
}

fn command<I, S>(program: &str, args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd
}

fn git_output<I, S>(repo_dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = run_output(command("git", args).current_dir(repo_dir))?;
    let text = String::from_utf8(output).context("git output was not UTF-8")?;
    Ok(text.trim().to_owned())
}

fn run_checked(cmd: &mut Command) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("running {}", display_command(cmd)))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{} failed with {status}", display_command(cmd)))
    }
}

fn run_status(cmd: &mut Command) -> Result<bool> {
    let status = cmd
        .status()
        .with_context(|| format!("running {}", display_command(cmd)))?;
    Ok(status.success())
}

fn run_output(cmd: &mut Command) -> Result<Vec<u8>> {
    #[expect(
        clippy::disallowed_methods,
        reason = "jackin-dev shells out to gh, git, cargo, and mise"
    )]
    let output = cmd
        .output()
        .with_context(|| format!("running {}", display_command(cmd)))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "{} failed with {}\n{}",
            display_command(cmd),
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

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn exists_label(path: &Path) -> &'static str {
    yes_no(path.exists())
}

fn built_label(value: bool) -> &'static str {
    if value { "built" } else { "not needed" }
}

fn print_sync_summary(pr: u64, paths: &PrPaths, info: &PullRequestInfo, auto: &AutoPrep) {
    emit_line(format!("Synced PR #{pr}:"));
    emit_line(format!("  repo: {}", paths.repo.display()));
    emit_line(format!("  env:  {}", paths.env_file.display()));
    emit_line(format!("  config: {}", paths.config.display()));
    emit_line(format!("  home:   {}", paths.home.display()));
    emit_line(format!("  head:   {}", info.head_oid));
    emit_line(format!("  files:  {}", info.changed_files.len()));
    emit_line(format!("  capsule: {}", built_label(auto.capsule)));
    emit_line(format!("  construct: {}", built_label(auto.construct)));
    emit_line("");
    emit_line("Next:");
    for line in enter_lines(paths) {
        emit_line(format!("  {line}"));
    }
}

fn emit_line(line: impl AsRef<str>) {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-dev is a CLI; command output belongs on stdout"
    )]
    {
        println!("{}", line.as_ref());
    }
}

#[cfg(test)]
mod tests;
