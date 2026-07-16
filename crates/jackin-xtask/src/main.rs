//! jackin-xtask: workspace automation (cargo xtask) for CI, lints, and docs gates.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`main`] — cargo xtask command dispatcher.

mod agent_files;
mod agent_links;
mod arch;
mod ci;
mod cmd;
mod construct;
mod container_paths_gate;
mod docs;
mod frame_timing;
mod fs_util;
mod headers;
mod health;
mod lint;
mod pr;
mod profile_matrix;
mod pty_fixture;
mod ratchet;
mod readme_freshness;
mod release_verify;
mod report;
mod schema;
mod suppressions;
mod test_layout;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "jackin-xtask", about = "jackin workspace automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the local CI merge-readiness gate.
    ///
    /// Partitions (`--only`, repeatable): lint, policy, tests, msrv, powerset,
    /// docs, snapshots. `--only` is a local-dev tool; merge readiness is the
    /// full `ci` (or `ci --fast` without powerset).
    ///
    /// Use as `cargo xtask ci --fast` for the non-e2e gate, or add `--e2e`
    /// to include Docker-backed smoke tests.
    Ci(ci::CiArgs),
    /// Construct base-image build and publish tasks.
    ///
    /// Use as `cargo xtask construct <subcommand>`.
    #[command(subcommand)]
    Construct(construct::ConstructCommand),
    /// Generate pull request body skeletons.
    ///
    /// Use as `cargo xtask pr body`.
    #[command(subcommand)]
    Pr(pr::PrCommand),
    /// Extract a PTY byte-stream fixture from a `--debug` run log for the
    /// capsule render-conformance harness.
    PtyFixture(pty_fixture::PtyFixtureArgs),
    /// Measure console first-frame and input-to-frame latency through a PTY.
    FrameTiming(frame_timing::FrameTimingArgs),
    /// Scaffold a new roadmap item and register it in the sidebar.
    ///
    /// Use as `cargo xtask change new <slug> --group <group>`.
    #[command(subcommand)]
    Change(docs::ChangeCommand),
    /// Documentation checks that do not require the TypeScript/Fumadocs runtime.
    ///
    /// Use as `cargo xtask docs repo-links`.
    #[command(subcommand)]
    Docs(docs::DocsCommand),
    /// Scaffold or validate research dossiers.
    ///
    /// Use as `cargo xtask research scaffold <slug>` / `research check`.
    #[command(subcommand)]
    Research(docs::ResearchCommand),
    /// Roadmap sidebar maintenance.
    ///
    /// Use as `cargo xtask roadmap audit` / `roadmap retire <slug>`.
    #[command(subcommand)]
    Roadmap(docs::RoadmapCommand),
    /// Enforce the versioned-schema five-artifact rule on a diff.
    ///
    /// Use as `cargo xtask schema-check --base origin/main`.
    SchemaCheck(schema::SchemaCheckArgs),
    /// Codebase-health lint gates (completed codebase-health W3 + W4).
    ///
    /// `cargo xtask lint` (no subcommand) runs **every** gate — the file-size
    /// ratchet, the test-file-layout rule, the AGENTS/CLAUDE symlink rule, and
    /// the dependency-direction check. This is the CI entry point. Add
    /// `--strict` to fail on architecture violations instead of just reporting
    /// them.
    ///
    /// Subcommands run a single gate: `cargo xtask lint files`
    /// (`--print-budget` refreshes the budget file), `cargo xtask lint tests`,
    /// `cargo xtask lint agents`, `cargo xtask lint arch` (`--dump` /
    /// `--strict`).
    Lint {
        #[command(subcommand)]
        command: Option<LintCommand>,
        /// When running all gates (no subcommand), fail on architecture
        /// violations. Forwarded to the arch gate; ignored when a subcommand
        /// is given.
        #[arg(long)]
        strict: bool,
    },
    /// Run Docker security-profile compatibility probes.
    ///
    /// Use as `cargo xtask profile-matrix standard`. The command runs the
    /// cheap local probes directly and reports heavyweight/host-specific cells
    /// as gated evidence with their required host prerequisites.
    #[command(name = "profile-matrix")]
    ProfileMatrix(profile_matrix::ProfileMatrixArgs),
    /// Verify a signed release archive and its published sidecars.
    ///
    /// Use as `cargo xtask release-verify <archive>.tar.gz`.
    #[command(name = "release-verify")]
    ReleaseVerify(release_verify::ReleaseVerifyArgs),
    /// Report-only code-health dashboard (completed codebase-health Phase 0).
    ///
    /// Use as `cargo xtask health`, `cargo xtask health --format json`, or
    /// `cargo xtask health --write-baseline`.
    Health(health::HealthArgs),
}

#[derive(Subcommand)]
enum LintCommand {
    /// Enforce the file-size ratchet (`ratchet.toml` families
    /// `file-size-production` / `file-size-test`).
    Files(lint::LintFilesArgs),
    /// Enforce the test-file-layout rule (tests live in a sibling
    /// `tests.rs`, never inline `#[cfg(test)] mod tests` or split across
    /// `tests/` sub-modules; allowlist in `ratchet.toml` family `test-layout`).
    Tests(test_layout::LintTestsArgs),
    /// Enforce that first-party `CLAUDE.md` files are symlinks to sibling
    /// `AGENTS.md` files.
    Agents(agent_files::LintAgentFilesArgs),
    /// Enforce that no `README.md` or `AGENTS.md` links to an `AGENTS.md`
    /// (both files are self-contained; nearest-`AGENTS.md`-wins).
    AgentLinks(agent_links::LintAgentLinksArgs),
    /// Dependency-direction gate (Workstream 4).
    Arch(arch::LintArchArgs),
    /// Residual `/jackin` production-literal shrink-only gate.
    ContainerPaths(container_paths_gate::LintContainerPathsArgs),
    /// Ownership-header contract for lib.rs/main.rs roots.
    Headers(headers::LintHeadersArgs),
    /// README freshness vs structural src layout changes.
    ReadmeFreshness(readme_freshness::LintReadmeFreshnessArgs),
    /// Bare-allow / per-lint expect suppression shrink-only reason-gate
    /// (`ratchet.toml` families `bare-allow-per-crate` / `expect-per-lint-crate`).
    Suppressions(suppressions::LintSuppressionsArgs),
    /// Unified shrink-only ratchet engine over `ratchet.toml` (all families).
    Ratchet(ratchet::LintRatchetArgs),
}

/// Run every codebase-health lint gate in sequence — the `cargo xtask lint`
/// (no subcommand) entry point used by CI. The file-size ratchet and the
/// test-file-layout rule always hard-fail on violations; the dependency-
/// direction gate fails only in `strict` mode (informational otherwise, while
/// the P2 inversions are still being cleaned up).
fn run_all_lints(strict: bool) -> anyhow::Result<()> {
    fs_util::enforce_sorted_iteration(&docs::repo_root()?)?;
    agent_files::enforce()?;
    agent_links::enforce()?;
    container_paths_gate::enforce()?;
    headers::enforce()?;
    // The unified ratchet owns file-size, test-layout, and suppression
    // families. Running their legacy shims here measured the same tree twice.
    ratchet::enforce()?;
    arch::check(strict)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Construct(cmd) => construct::run(cmd),
        Command::Ci(args) => ci::run(args),
        Command::Pr(cmd) => pr::run(cmd),
        Command::PtyFixture(args) => pty_fixture::run(args),
        Command::FrameTiming(args) => frame_timing::run(args),
        Command::Change(cmd) => docs::run_change(cmd),
        Command::Docs(cmd) => docs::run_docs(cmd),
        Command::Research(cmd) => docs::run_research(cmd),
        Command::Roadmap(cmd) => docs::run_roadmap(cmd),
        Command::SchemaCheck(args) => schema::run(args),
        Command::ProfileMatrix(args) => profile_matrix::run(args),
        Command::ReleaseVerify(args) => release_verify::run(args),
        Command::Health(args) => health::run(args),
        Command::Lint { command, strict } => match command {
            Some(LintCommand::Files(args)) => lint::run(args),
            Some(LintCommand::Tests(args)) => test_layout::run(args),
            Some(LintCommand::Agents(args)) => agent_files::run(args),
            Some(LintCommand::AgentLinks(args)) => agent_links::run(args),
            Some(LintCommand::Arch(args)) => arch::run(args),
            Some(LintCommand::ContainerPaths(args)) => container_paths_gate::run(args),
            Some(LintCommand::Headers(args)) => headers::run(args),
            Some(LintCommand::ReadmeFreshness(args)) => readme_freshness::run(args),
            Some(LintCommand::Suppressions(args)) => suppressions::run(args),
            Some(LintCommand::Ratchet(args)) => ratchet::run(args),
            None => run_all_lints(strict),
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            #[expect(
                clippy::print_stderr,
                reason = "jackin-xtask is a CLI; the error report is its user-facing output"
            )]
            {
                eprintln!("error: {err:#}");
            }
            ExitCode::FAILURE
        }
    }
}
