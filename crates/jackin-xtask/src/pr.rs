//! Pull-request verification helpers.
//!
//! These tasks replace the long copy-paste checkout/setup blocks in PR bodies.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::Value;

const DEFAULT_REPO: &str = "jackin-project/jackin";
const REPO_DIR_NAME: &str = "jackin";

#[derive(Subcommand)]
pub(crate) enum PrCommand {
    /// Clone/fetch/build a PR checkout and write a shell env file.
    Prepare(PrepareArgs),
}

#[derive(Args)]
pub(crate) struct PrepareArgs {
    /// GitHub pull request number.
    pr: u64,
    /// Repository in owner/name form.
    #[arg(long, default_value = DEFAULT_REPO)]
    repo: String,
    /// PR test root. Defaults to ~/Projects/jackin-project/test/pr-<number>.
    #[arg(long)]
    test_dir: Option<PathBuf>,
    /// Isolated config source for `JACKIN_CONFIG_DIR`.
    #[arg(long, value_enum, default_value_t = ConfigSource::Blank)]
    config: ConfigSource,
    /// Replace an existing PR-scoped config dir before preparing config.
    #[arg(long)]
    replace_config: bool,
    /// Build the local construct image and export `JACKIN_CONSTRUCT_IMAGE`.
    #[arg(long)]
    construct: bool,
    /// Build/export jackin-capsule and append `JACKIN_CAPSULE_BIN` to env.sh.
    #[arg(long)]
    capsule: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum ConfigSource {
    /// Start with an empty PR-scoped config directory.
    Blank,
    /// Copy ~/.config/jackin into the PR-scoped config directory.
    Copy,
}

pub(crate) fn run(command: PrCommand) -> Result<()> {
    match command {
        PrCommand::Prepare(args) => prepare(args),
    }
}

fn prepare(args: PrepareArgs) -> Result<()> {
    let home = home_dir()?;
    let test_dir = args
        .test_dir
        .unwrap_or_else(|| home.join(format!("Projects/jackin-project/test/pr-{}", args.pr)));
    let repo_dir = test_dir.join(REPO_DIR_NAME);
    let config_dir = home.join(format!(".config/jackin-pr-{}", args.pr));
    let home_dir = home.join(format!(".jackin-pr-{}", args.pr));
    let env_file = test_dir.join("env.sh");
    let pr = pr_info(args.pr, &args.repo)?;

    fs::create_dir_all(&test_dir).with_context(|| format!("creating {}", test_dir.display()))?;
    checkout_repo(&args.repo, args.pr, &pr, &repo_dir)?;
    run_checked(command("mise", ["trust"]).current_dir(&repo_dir))?;
    run_checked(command("mise", ["install"]).current_dir(&repo_dir))?;
    run_checked(command("cargo", ["build", "--bin", "jackin"]).current_dir(&repo_dir))?;
    prepare_config(args.config, args.replace_config, &config_dir, &home)?;
    fs::create_dir_all(&home_dir).with_context(|| format!("creating {}", home_dir.display()))?;

    let mut env_lines = Vec::new();
    env_lines.push(format!(
        "export PATH=\"{}:$PATH\"",
        repo_dir.join("target/debug").display()
    ));
    env_lines.push(format!(
        "export JACKIN_CONFIG_DIR={}",
        shell_quote(config_dir.as_os_str())
    ));
    env_lines.push(format!(
        "export JACKIN_HOME_DIR={}",
        shell_quote(home_dir.as_os_str())
    ));

    if args.construct {
        run_checked(command("mise", ["run", "construct-build-local"]).current_dir(&repo_dir))?;
        env_lines.push("export JACKIN_CONSTRUCT_IMAGE=jackin-local/construct:trixie".to_owned());
    }

    if args.capsule {
        let output = run_output(
            command(
                "cargo",
                ["run", "--bin", "build-jackin-capsule", "--", "--export"],
            )
            .current_dir(&repo_dir),
        )?;
        let stdout = String::from_utf8(output)
            .context("build-jackin-capsule --export output was not valid UTF-8")?;
        let export_line = stdout
            .lines()
            .rev()
            .find(|line| line.starts_with("export JACKIN_CAPSULE_BIN="))
            .ok_or_else(|| {
                anyhow!("build-jackin-capsule --export did not print JACKIN_CAPSULE_BIN")
            })?;
        env_lines.push(export_line.to_owned());
    }

    fs::write(&env_file, format!("{}\n", env_lines.join("\n")))
        .with_context(|| format!("writing {}", env_file.display()))?;
    print_summary(args.pr, &repo_dir, &env_file, &config_dir, &home_dir);
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
            "headRefName,headRepository",
        ],
    );
    let output = run_output(&mut cmd)?;
    let json: Value = serde_json::from_slice(&output).context("parsing gh pr view JSON")?;
    let head_ref = json
        .get("headRefName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("gh pr view did not return headRefName"))?
        .to_owned();
    let head_repo = json
        .get("headRepository")
        .and_then(|repo| repo.get("nameWithOwner"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(PullRequestInfo {
        head_ref,
        head_repo,
    })
}

#[derive(Debug)]
struct PullRequestInfo {
    head_ref: String,
    head_repo: Option<String>,
}

fn checkout_repo(repo: &str, pr: u64, info: &PullRequestInfo, repo_dir: &Path) -> Result<()> {
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

    let same_repo = info
        .head_repo
        .as_deref()
        .is_none_or(|head_repo| head_repo == repo);
    if same_repo {
        let remote_ref = format!("refs/remotes/origin/{}", info.head_ref);
        run_checked(
            command(
                "git",
                [
                    "fetch",
                    "-f",
                    "origin",
                    &format!("{}:{remote_ref}", info.head_ref),
                ],
            )
            .current_dir(repo_dir),
        )?;
        run_checked(
            command("git", ["checkout", "-B", &info.head_ref, &remote_ref]).current_dir(repo_dir),
        )?;
    } else {
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
            command("git", ["checkout", "-B", &format!("pr-{pr}"), &remote_ref])
                .current_dir(repo_dir),
        )?;
    }
    Ok(())
}

fn prepare_config(
    source: ConfigSource,
    replace_config: bool,
    config_dir: &Path,
    home: &Path,
) -> Result<()> {
    match source {
        ConfigSource::Blank => {
            if replace_config && config_dir.exists() {
                fs::remove_dir_all(config_dir)
                    .with_context(|| format!("removing {}", config_dir.display()))?;
            }
            fs::create_dir_all(config_dir)
                .with_context(|| format!("creating {}", config_dir.display()))?;
        }
        ConfigSource::Copy => {
            if config_dir.exists() {
                if replace_config {
                    fs::remove_dir_all(config_dir)
                        .with_context(|| format!("removing {}", config_dir.display()))?;
                } else if has_entries(config_dir)? {
                    bail!(
                        "{} already exists and is not empty; pass --replace-config to refresh it",
                        config_dir.display()
                    );
                }
            }
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

fn has_entries(path: &Path) -> Result<bool> {
    let mut entries = fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(entries.next().transpose()?.is_some())
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
        reason = "xtask automation shells out to git, gh, cargo, and mise"
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

fn print_summary(pr: u64, repo_dir: &Path, env_file: &Path, config_dir: &Path, home_dir: &Path) {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; setup instructions are its output"
    )]
    {
        println!("Prepared PR #{pr} checkout:");
        println!("  repo: {}", repo_dir.display());
        println!("  env:  {}", env_file.display());
        println!("  config: {}", config_dir.display());
        println!("  home:   {}", home_dir.display());
        println!();
        println!("Next:");
        println!("  cd {}", shell_quote(repo_dir.as_os_str()));
        println!("  source {}", shell_quote(env_file.as_os_str()));
        println!("  which jackin");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_leaves_plain_paths_bare() {
        assert_eq!(
            shell_quote(OsStr::new("/tmp/jackin-pr-550")),
            "/tmp/jackin-pr-550"
        );
    }

    #[test]
    fn shell_quote_wraps_spaces_and_quotes() {
        assert_eq!(
            shell_quote(OsStr::new("/tmp/PR user's checkout")),
            "'/tmp/PR user'\"'\"'s checkout'"
        );
    }
}
