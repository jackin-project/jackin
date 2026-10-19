mod affected_crates;
mod agent_files;
mod agent_links;
mod agent_status_truth;
mod arch;
mod ci;
mod ci_audit;
mod ci_build_times;
mod ci_cargo_audit;
mod ci_doc_examples;
mod ci_evidence;
mod ci_fuzz;
mod ci_junit;
mod ci_result;
mod ci_route;
mod ci_stage;
mod ci_target;
mod ci_toolchain;
mod clippy_affected;
mod cmd;
mod construct;
mod container_paths_gate;
mod desktop;
mod docs;
mod frame_timing;
mod fs_util;
mod github;
mod health;
mod layout_lint;
mod lint;
mod pr;
mod preview_package;
mod profile_matrix;
mod pty_fixture;
mod ratchet;
mod readme_freshness;
mod release_archive;
mod release_sbom;
mod report;
mod schema;
mod suppressions;
mod telemetry_bench;
mod telemetry_registry;
use jackin_xtask::workflow_yaml;

use std::{process::ExitCode, thread};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "jackin-xtask", about = "jackin workspace automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    AffectedCrates(affected_crates::AffectedCratesArgs),
    Ci(ci::CiArgs),
    #[command(name = "ci-audit")]
    CiAudit(ci_audit::CiAuditArgs),
    #[command(name = "ci-build-times")]
    CiBuildTimes(ci_build_times::CiBuildTimesArgs),
    #[command(name = "ci-cargo-audit")]
    CiCargoAudit(ci_cargo_audit::CiCargoAuditArgs),
    #[command(name = "ci-doc-examples")]
    CiDocExamples(ci_doc_examples::CiDocExamplesArgs),
    #[command(name = "ci-evidence", subcommand)]
    CiEvidence(ci_evidence::CiEvidenceCommand),
    #[command(name = "ci-fuzz")]
    CiFuzz(ci_fuzz::CiFuzzArgs),
    #[command(name = "ci-junit")]
    CiJunit(ci_junit::CiJunitArgs),
    #[command(name = "ci-result", subcommand)]
    CiResult(ci_result::CiResultCommand),
    #[command(name = "ci-route")]
    CiRoute(ci_route::CiRouteArgs),
    #[command(name = "clippy-affected")]
    ClippyAffected(clippy_affected::ClippyAffectedArgs),
    #[command(name = "ci-stage")]
    CiStage(ci_stage::CiStageArgs),
    #[command(name = "ci-target", subcommand)]
    CiTarget(ci_target::CiTargetCommand),
    #[command(name = "ci-toolchain", subcommand)]
    CiToolchain(ci_toolchain::CiToolchainCommand),
    #[command(name = "github", subcommand)]
    Github(github::GithubCommand),
    #[command(subcommand)]
    Construct(construct::ConstructCommand),
    #[command(subcommand)]
    Desktop(desktop::DesktopCommand),
    #[command(subcommand)]
    Pr(pr::PrCommand),
    PtyFixture(pty_fixture::PtyFixtureArgs),
    FrameTiming(frame_timing::FrameTimingArgs),
    #[command(subcommand)]
    Change(docs::ChangeCommand),
    #[command(subcommand)]
    Docs(docs::DocsCommand),
    #[command(subcommand)]
    Research(docs::ResearchCommand),
    #[command(subcommand)]
    Roadmap(docs::RoadmapCommand),
    SchemaCheck(schema::SchemaCheckArgs),
    #[command(name = "telemetry-registry")]
    TelemetryRegistry(telemetry_registry::TelemetryRegistryArgs),
    #[command(name = "telemetry-bench")]
    TelemetryBench(telemetry_bench::TelemetryBenchArgs),
    Lint {
        #[command(subcommand)]
        command: Option<LintCommand>,
        #[arg(long)]
        strict: bool,
    },
    #[command(name = "profile-matrix")]
    ProfileMatrix(profile_matrix::ProfileMatrixArgs),
    #[command(name = "release-archives")]
    ReleaseArchives(release_archive::ReleaseArchivesArgs),
    #[command(name = "release-preview-package")]
    ReleasePreviewPackage,
    Health(health::HealthArgs),
}

#[derive(Subcommand)]
enum LintCommand {
    Files(lint::LintFilesArgs),
    Tests(layout_lint::LintTestsArgs),
    Agents(agent_files::LintAgentFilesArgs),
    AgentLinks(agent_links::LintAgentLinksArgs),
    AgentStatusTruth(agent_status_truth::LintAgentStatusTruthArgs),
    Arch(arch::LintArchArgs),
    ContainerPaths(container_paths_gate::LintContainerPathsArgs),
    ReadmeFreshness(readme_freshness::LintReadmeFreshnessArgs),
    Suppressions(suppressions::LintSuppressionsArgs),
    Ratchet(ratchet::LintRatchetArgs),
}

fn run_all_lints(strict: bool) -> anyhow::Result<()> {
    fs_util::enforce_sorted_iteration(&docs::repo_root()?)?;

    let failures = thread::scope(|scope| {
        let gates = [
            ("agents", scope.spawn(agent_files::enforce)),
            ("agent-links", scope.spawn(agent_links::enforce)),
            (
                "container-paths",
                scope.spawn(container_paths_gate::enforce),
            ),
            (
                "telemetry-registry",
                scope.spawn(|| {
                    telemetry_registry::run(telemetry_registry::TelemetryRegistryArgs {
                        generate: false,
                    })
                }),
            ),
            ("ratchet", scope.spawn(ratchet::enforce)),
            ("arch", scope.spawn(move || arch::check(strict))),
        ];

        gates
            .into_iter()
            .filter_map(|(name, gate)| match gate.join() {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(format!("{name}: {error:#}")),
                Err(_) => Some(format!("{name}: gate panicked")),
            })
            .collect::<Vec<_>>()
    });

    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("lint gates failed:\n  {}", failures.join("\n  "))
    }
}

fn run_lint(command: Option<LintCommand>, strict: bool) -> anyhow::Result<()> {
    match command {
        Some(LintCommand::Files(args)) => lint::run(args),
        Some(LintCommand::Tests(args)) => layout_lint::run(args),
        Some(LintCommand::Agents(args)) => agent_files::run(args),
        Some(LintCommand::AgentLinks(args)) => agent_links::run(args),
        Some(LintCommand::AgentStatusTruth(args)) => agent_status_truth::run(args),
        Some(LintCommand::Arch(args)) => arch::run(args),
        Some(LintCommand::ContainerPaths(args)) => container_paths_gate::run(args),
        Some(LintCommand::ReadmeFreshness(args)) => readme_freshness::run(args),
        Some(LintCommand::Suppressions(args)) => suppressions::run(args),
        Some(LintCommand::Ratchet(args)) => ratchet::run(args),
        None => run_all_lints(strict),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::AffectedCrates(args) => affected_crates::run(args),
        Command::ClippyAffected(args) => clippy_affected::run(args),
        Command::Construct(cmd) => construct::run(cmd),
        Command::Desktop(cmd) => desktop::run(cmd),
        Command::Ci(args) => ci::run(args),
        Command::CiAudit(args) => ci_audit::run(args),
        Command::CiBuildTimes(args) => ci_build_times::run(args),
        Command::CiCargoAudit(args) => ci_cargo_audit::run(args),
        Command::CiDocExamples(args) => ci_doc_examples::run(args),
        Command::CiEvidence(command) => ci_evidence::run(command),
        Command::CiFuzz(args) => ci_fuzz::run(args),
        Command::CiJunit(args) => ci_junit::run(args),
        Command::CiResult(command) => ci_result::run(command),
        Command::CiRoute(args) => ci_route::run(args),
        Command::CiStage(args) => ci_stage::run(args),
        Command::CiTarget(command) => ci_target::run(command),
        Command::CiToolchain(command) => ci_toolchain::run(command),
        Command::Github(command) => github::run(command),
        Command::Pr(cmd) => pr::run(cmd),
        Command::PtyFixture(args) => pty_fixture::run(args),
        Command::FrameTiming(args) => frame_timing::run(args),
        Command::Change(cmd) => docs::run_change(cmd),
        Command::Docs(cmd) => docs::run_docs(cmd),
        Command::Research(cmd) => docs::run_research(cmd),
        Command::Roadmap(cmd) => docs::run_roadmap(cmd),
        Command::SchemaCheck(args) => schema::run(args),
        Command::TelemetryRegistry(args) => telemetry_registry::run(args),
        Command::TelemetryBench(args) => telemetry_bench::run(args),
        Command::ProfileMatrix(args) => profile_matrix::run(args),
        Command::ReleaseArchives(args) => release_archive::run(args),
        Command::ReleasePreviewPackage => preview_package::run(),
        Command::Health(args) => health::run(args),
        Command::Lint { command, strict } => run_lint(command, strict),
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
