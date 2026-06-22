//! Developer tooling for local jackin pull request verification.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::Value;

const DEFAULT_REPO: &str = "jackin-project/jackin";
const REPO_DIR_NAME: &str = "jackin";
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
    Status(StatusArgs),
}

#[derive(Args)]
struct SyncArgs {
    /// GitHub pull request number.
    pr: u64,
    /// Repository in owner/name form.
    #[arg(long, default_value = DEFAULT_REPO)]
    repo: String,
    /// PR test root. Defaults to ~/Projects/jackin-project/test/pr-<number>.
    #[arg(long)]
    test_dir: Option<PathBuf>,
    /// Isolated config source for `JACKIN_CONFIG_DIR`.
    #[arg(long, value_enum, default_value_t = ConfigSource::Copy)]
    config: ConfigSource,
}

#[derive(Args)]
struct PrPathArgs {
    /// GitHub pull request number.
    pr: u64,
    /// PR test root. Defaults to ~/Projects/jackin-project/test/pr-<number>.
    #[arg(long)]
    test_dir: Option<PathBuf>,
}

#[derive(Args)]
struct StatusArgs {
    /// GitHub pull request number.
    pr: u64,
    /// Repository in owner/name form.
    #[arg(long, default_value = DEFAULT_REPO)]
    repo: String,
    /// PR test root. Defaults to ~/Projects/jackin-project/test/pr-<number>.
    #[arg(long)]
    test_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
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
    head_oid: String,
}

#[derive(Debug)]
struct AutoPrep {
    capsule: bool,
    construct: bool,
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
    let paths = PrPaths::new(args.pr, args.test_dir)?;
    let home = home_dir()?;
    let pr = pr_info(args.pr, &args.repo)?;

    fs::create_dir_all(&paths.root)
        .with_context(|| format!("creating {}", paths.root.display()))?;
    checkout_repo(&args.repo, args.pr, &paths.repo)?;
    run_checked(command("mise", ["trust"]).current_dir(&paths.repo))?;
    run_checked(command("mise", ["install"]).current_dir(&paths.repo))?;
    run_checked(command("cargo", ["build", "--bin", "jackin"]).current_dir(&paths.repo))?;

    prepare_config(args.config, &paths.config, &home)?;
    fs::create_dir_all(&paths.home)
        .with_context(|| format!("creating {}", paths.home.display()))?;

    let changed_files = pr_changed_files(args.pr, &args.repo)?;
    let auto = auto_prep(&changed_files);

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
    print_sync_summary(args.pr, &paths, &pr, &auto, &changed_files);
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
    emit_line(format!("cd {}", shell_quote(paths.repo.as_os_str())));
    emit_line(format!(
        "source {}",
        shell_quote(paths.env_file.as_os_str())
    ));
    emit_line("which jackin");
    Ok(())
}

fn print_path(args: PrPathArgs) -> Result<()> {
    let paths = PrPaths::new(args.pr, args.test_dir)?;
    emit_line(paths.root.display().to_string());
    Ok(())
}

fn status(args: StatusArgs) -> Result<()> {
    let paths = PrPaths::new(args.pr, args.test_dir)?;
    let pr = pr_info(args.pr, &args.repo)?;
    let local_head = if paths.repo.join(".git").exists() {
        Some(git_output(&paths.repo, ["rev-parse", "HEAD"])?)
    } else {
        None
    };
    let local_branch = if paths.repo.join(".git").exists() {
        Some(git_output(&paths.repo, ["branch", "--show-current"])?)
    } else {
        None
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
    emit_line(format!("fresh: {}", if fresh { "yes" } else { "no" }));
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
            "headRefOid",
        ],
    );
    let output = run_output(&mut cmd)?;
    let json: Value = serde_json::from_slice(&output).context("parsing gh pr view JSON")?;
    let head_oid = json_string(&json, "headRefOid")?;
    Ok(PullRequestInfo { head_oid })
}

fn json_string(json: &Value, key: &str) -> Result<String> {
    json.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("gh pr view did not return {key}"))
}

fn pr_changed_files(pr: u64, repo: &str) -> Result<Vec<String>> {
    let mut cmd = command(
        "gh",
        ["pr", "diff", &pr.to_string(), "--repo", repo, "--name-only"],
    );
    let output = run_output(&mut cmd)?;
    Ok(String::from_utf8_lossy(&output)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn checkout_repo(repo: &str, pr: u64, repo_dir: &Path) -> Result<()> {
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

    let remote_ref = format!("refs/remotes/origin/pr-{pr}-head");
    run_checked(
        command(
            "git",
            [
                "fetch",
                "-f",
                "origin",
                &format!("pull/{pr}/head:{remote_ref}"),
            ],
        )
        .current_dir(repo_dir),
    )?;
    run_checked(
        command("git", ["checkout", "-B", &format!("pr-{pr}"), &remote_ref]).current_dir(repo_dir),
    )?;
    Ok(())
}

fn prepare_config(source: ConfigSource, config_dir: &Path, home: &Path) -> Result<()> {
    if config_dir.exists() {
        fs::remove_dir_all(config_dir)
            .with_context(|| format!("removing {}", config_dir.display()))?;
    }
    match source {
        ConfigSource::Blank => fs::create_dir_all(config_dir)
            .with_context(|| format!("creating {}", config_dir.display()))?,
        ConfigSource::Copy => {
            let source_dir = home.join(".config/jackin");
            if source_dir.exists() {
                copy_dir_recursive(&source_dir, config_dir)?;
            } else {
                fs::create_dir_all(config_dir)
                    .with_context(|| format!("creating {}", config_dir.display()))?;
            }
        }
    }
    Ok(())
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

fn auto_prep(files: &[String]) -> AutoPrep {
    AutoPrep {
        capsule: files.iter().any(|file| {
            file.starts_with("crates/jackin-capsule/")
                || file.starts_with("crates/jackin-protocol/")
        }),
        construct: files.iter().any(|file| {
            file.starts_with("docker/construct/")
                || file == "docker-bake.hcl"
                || file == "mise.toml"
                || file.starts_with("crates/jackin-xtask/src/construct")
        }),
    }
}

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
    Ok(String::from_utf8_lossy(&output).trim().to_owned())
}

fn run_checked(cmd: &mut Command) -> Result<()> {
    let display = display_command(cmd);
    let status = cmd.status().with_context(|| format!("running {display}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{display} failed with {status}"))
    }
}

fn run_output(cmd: &mut Command) -> Result<Vec<u8>> {
    let display = display_command(cmd);
    #[expect(
        clippy::disallowed_methods,
        reason = "jackin-dev shells out to gh, git, cargo, and mise"
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

fn exists_label(path: &Path) -> &'static str {
    if path.exists() { "yes" } else { "no" }
}

fn print_sync_summary(
    pr: u64,
    paths: &PrPaths,
    info: &PullRequestInfo,
    auto: &AutoPrep,
    changed_files: &[String],
) {
    emit_line(format!("Synced PR #{pr}:"));
    emit_line(format!("  repo: {}", paths.repo.display()));
    emit_line(format!("  env:  {}", paths.env_file.display()));
    emit_line(format!("  config: {}", paths.config.display()));
    emit_line(format!("  home:   {}", paths.home.display()));
    emit_line(format!("  head:   {}", info.head_oid));
    emit_line(format!("  files:  {}", changed_files.len()));
    emit_line(format!(
        "  capsule: {}",
        if auto.capsule { "built" } else { "not needed" }
    ));
    emit_line(format!(
        "  construct: {}",
        if auto.construct {
            "built"
        } else {
            "not needed"
        }
    ));
    emit_line("");
    emit_line("Next:");
    emit_line(format!("  cd {}", shell_quote(paths.repo.as_os_str())));
    emit_line(format!(
        "  source {}",
        shell_quote(paths.env_file.as_os_str())
    ));
    emit_line("  which jackin");
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
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_pr_bundle() {
        let root = PathBuf::from("/Users/example/Projects/jackin-project/test/pr-580");
        let paths = PrPaths::from_root(root.clone());

        assert_eq!(paths.repo, root.join("jackin"));
        assert_eq!(paths.env_file, root.join("env.sh"));
        assert_eq!(paths.config, root.join("state/config"));
        assert_eq!(paths.home, root.join("state/home"));
    }

    #[test]
    fn env_points_at_bundle_state() {
        let root = PathBuf::from("/Users/example/Projects/jackin-project/test/pr-580");
        let paths = PrPaths::from_root(root.clone());
        let env = env_lines(&paths).join("\n");

        assert!(env.contains("/Users/example/Projects/jackin-project/test/pr-580/state/config"));
        assert!(env.contains("/Users/example/Projects/jackin-project/test/pr-580/state/home"));
        assert!(!env.contains(".config/jackin-pr-"));
        assert!(!env.contains(".jackin-pr-"));
    }

    #[test]
    fn auto_prep_detects_capsule_and_construct_inputs() {
        let auto = auto_prep(&[
            "crates/jackin-capsule/src/lib.rs".to_owned(),
            "docker/construct/Dockerfile".to_owned(),
        ]);

        assert!(auto.capsule);
        assert!(auto.construct);
    }

    #[test]
    fn auto_prep_ignores_docs_only_changes() {
        let auto =
            auto_prep(&["docs/content/docs/reference/roadmap/pr-verification.mdx".to_owned()]);

        assert!(!auto.capsule);
        assert!(!auto.construct);
    }

    #[test]
    fn shell_quote_quotes_spaces() {
        assert_eq!(
            shell_quote(OsStr::new("/tmp/with space/env.sh")),
            "'/tmp/with space/env.sh'"
        );
    }
}
