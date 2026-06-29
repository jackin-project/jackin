//! CLI argument structs for `jackin load`, `jackin console`, and `jackin hardline`.
//!
//! Not responsible for: resolving workspaces, building images, or spawning
//! containers — structs are parsed by `clap` and dispatched to runtime handlers.

use clap::{Args, Subcommand};
use std::path::PathBuf;

use super::{BANNER, HELP_STYLES};

/// Jack a role into an isolated container
///
/// TARGET can be a path (~/Projects/my-app), a path with container
/// destination (~/Projects/my-app:/app), or a saved workspace name.
/// When omitted, the current directory is used.
//
// Launch-time toggles plus the
// positional `selector` / `target` / `mounts` map directly to CLI flags;
// bundling them into nested structs would obscure rather than clarify.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin load                                          # use workspace + last role for cwd
  jackin load --rebuild                                # same, with fresh agent install
  jackin load agent-smith
  jackin load agent-smith ~/Projects/my-app
  jackin load agent-smith ~/Projects/my-app:/app
  jackin load agent-smith big-monorepo
  jackin load agent-smith big-monorepo --mount ~/extra-data
  jackin load agent-smith ~/app --mount ~/cache:/cache:ro
  jackin load the-architect --role-branch feat/my-pr   # build + test a PR branch locally"
)]
pub struct LoadArgs {
    /// Role class selector (e.g. `agent-smith`, `chainargos/agent-brown`).
    /// When omitted, uses the last-used or default role for the workspace.
    pub selector: Option<String>,
    /// Path, `path:container-dest`, or saved workspace name
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,
    /// Additional bind-mount spec as `path[:ro]` or `src:dst[:ro]` (repeatable)
    #[arg(long = "mount")]
    pub mounts: Vec<String>,
    /// Force rebuild the Docker image and refresh agent CLI install layers
    #[arg(long, default_value_t = false)]
    pub rebuild: bool,
    /// Acknowledge a dirty host working tree for isolated mounts.
    #[arg(long)]
    pub force: bool,
    /// Agent to launch under (claude, codex, amp, kimi, or opencode). Overrides the
    /// workspace's `default_agent` field for this launch only. When
    /// neither is set, defaults to claude.
    #[arg(long, value_parser = parse_agent)]
    pub agent: Option<jackin_core::Agent>,
    /// Check out a specific branch of the role repository for local testing.
    /// The published image is ignored and the image is built from the branch's
    /// Dockerfile using Docker's layer cache. Useful for verifying a PR before
    /// it merges to the default branch.
    #[arg(long)]
    pub role_branch: Option<String>,
    /// Docker security profile for this launch.
    #[arg(long, value_name = "PROFILE", value_parser = parse_docker_profile)]
    pub docker_profile: Option<jackin_runtime::runtime::DockerSecurityProfile>,
    /// Print the resolved launch plan (workspace, role, mounts, auth decisions,
    /// derived image) and exit without spawning any containers.
    #[arg(long)]
    pub dry_run: bool,
    /// Output format for `--dry-run` (`human` or `json`)
    #[arg(
        long,
        value_name = "FORMAT",
        default_value = "human",
        requires = "dry_run"
    )]
    pub format: String,
}

fn parse_agent(s: &str) -> Result<jackin_core::Agent, String> {
    s.parse()
        .map_err(|e: jackin_core::ParseAgentError| e.to_string())
}

fn parse_docker_profile(s: &str) -> Result<jackin_runtime::runtime::DockerSecurityProfile, String> {
    s.parse()
        .map_err(|e: jackin_runtime::runtime::docker_profile::ParseProfileError| e.to_string())
}

/// Reattach to a running role's session
///
/// When omitted, finds the saved workspace for the current directory and
/// reconnects to a running role container belonging to it.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin hardline                              # auto-detect workspace + running role for cwd
  jackin hardline --inspect                    # inspect detected instance state without attaching
  jackin hardline --new                        # start another agent session in the detected instance
  jackin hardline --new --agent codex          # start a specific runtime in the selected instance
  jackin hardline --shell                      # open a one-shot zsh shell in the detected instance
  jackin hardline agent-smith
  jackin hardline --inspect k7p9m2xq
  jackin hardline chainargos/the-architect
  jackin hardline k7p9m2xq
  jackin hardline jk-k7p9m2xq-agentsmith"
)]
pub struct HardlineArgs {
    /// Role class selector, instance ID, or container name to reconnect to.
    /// When omitted, uses the running role in the workspace for the current directory.
    pub selector: Option<String>,
    /// Print manifest, Docker, `DinD`, and mount state without attaching or restarting.
    #[arg(long)]
    pub inspect: bool,
    /// Start a new foreground agent process inside the selected running instance.
    #[arg(long, conflicts_with = "inspect")]
    pub new: bool,
    /// Agent runtime for `--new` (claude, codex, amp, kimi, or opencode). Defaults to the instance manifest.
    #[arg(long, value_parser = parse_agent, requires = "new")]
    pub agent: Option<jackin_core::Agent>,
    /// Open a zsh shell in the selected running instance without an agent slug.
    #[arg(long, conflicts_with_all = ["inspect", "new"])]
    pub shell: bool,
}

/// Open the operator console to manage workspaces, launch roles, and more
///
/// Running `jackin` with no subcommand on an interactive terminal opens the
/// same console.
/// The operator console is always the full experience — rich TUI, intro rain
/// on entry, outro rain on the last container's exit. There is nothing to
/// disable, so this carries no flags.
#[derive(Debug, Args, PartialEq, Eq, Default, Clone)]
// No `before_help` here: as the flattened root args it would leak the pill onto
// the root `jackin --help`, which instead shows the binary's frozen-rain banner
// with the centered lockup. The `Console` command variant re-adds the pill for
// `console --help`.
#[command(styles = HELP_STYLES)]
pub struct ConsoleArgs {}

/// Validate, migrate, and scaffold role repositories
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum RoleCommand {
    /// Validate a role repository's manifest, Dockerfile, hooks, and env declarations
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Validate(RoleRepoPathArgs),
    /// Migrate a role manifest to the current schema version, then validate it
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Migrate(RoleRepoPathArgs),
    /// Create a new role repository scaffold
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Create(RoleCreateArgs),
    /// Print the construct image version tag pinned in the role Dockerfile
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    ConstructVersion(RoleRepoPathArgs),
    /// Print the published Docker image declared in the role manifest
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    PublishedImage(RoleRepoPathArgs),
    /// Print the published Docker image repository without tag or digest
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    PublishedImageRepository(RoleRepoPathArgs),
    /// Print Docker labels for publishing the role image
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    PublishLabels(RolePublishLabelsArgs),
}

/// Role repository path argument shared by `validate` and `migrate`.
#[derive(Debug, Args, PartialEq, Eq)]
pub struct RoleRepoPathArgs {
    /// Role repository path. Defaults to the current directory.
    #[arg(value_name = "ROLE_REPO_PATH")]
    pub path: Option<PathBuf>,
}

/// Arguments for `jackin role publish-labels`.
#[derive(Debug, Args, PartialEq, Eq)]
pub struct RolePublishLabelsArgs {
    /// Git commit SHA for the role repository image being published.
    #[arg(long)]
    pub role_git_sha: String,
    /// Role repository path. Defaults to the current directory.
    #[arg(value_name = "ROLE_REPO_PATH")]
    pub path: Option<PathBuf>,
}

/// Arguments for `jackin role create`.
#[derive(Debug, Args, PartialEq, Eq)]
pub struct RoleCreateArgs {
    /// Role name or namespace/name selector, e.g. `docs-writer` or `chainargos/backend-engineer`
    pub role: String,
    /// Projects directory where the role repo should be created. Defaults to `JACKIN_PROJECTS_DIR` or ~/Projects.
    #[arg(value_name = "PROJECTS_DIR")]
    pub projects_dir: Option<PathBuf>,
}

#[cfg(test)]
mod tests;
