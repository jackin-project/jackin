// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Args;
use std::collections::BTreeMap;
use std::io::Write;

use crate::cli::BANNER;
use crate::cli::format::OutputFormat;
use jackin_core::{ContainerHandle, InstanceIndexEntry, JackinPaths};
use jackin_docker::docker_client::{BollardDockerClient, ContainerState, DockerApi};
use jackin_protocol::control::AgentRegistryEntry;
use jackin_runtime::instance::manifest::InstanceIndex;

/// Command string for querying the agent registry over the capsule socket.
const AGENTS_PENDING: &str = "jackin-agents-pending";

fn jackin_agents_command() -> String {
    format!(
        "if test -S /jackin/run/jackin.sock; then /jackin/runtime/jackin-capsule protocol-check --expected-major {} && /jackin/runtime/jackin-capsule agents --format json; else printf 'jackin-agents-pending\\n'; fi",
        jackin_protocol::capsule_transport::CONTROL_PROTOCOL_MAJOR
    )
}

/// Command for getting the current git branch inside the container workdir.
const GIT_BRANCH_CMD: &str =
    "git -C \"$JACKIN_WORKDIR\" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown";

/// Command for getting PR info including CI status. Requires `gh` and `GH_TOKEN`.
const GH_PR_CMD: &str = "gh pr view --json number,title,url,statusCheckRollup 2>/dev/null";

/// `jackin status` — three-level fleet overview.
///
/// - `jackin status`                     workspace summary
/// - `jackin status <workspace>`         instances in that workspace
/// - `jackin status <workspace> <id>`    full instance detail
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Show fleet status — workspaces, instances, and agents",
    long_about = "Show a three-level fleet overview.\n\n\
        Run `jackin status` for a workspace summary.\n\
        Run `jackin status <workspace>` to list instances in a workspace.\n\
        Run `jackin status <workspace> <instance-id>` for full detail including\n\
        branch, PR, CI, and per-agent codename table."
)]
pub struct StatusArgs {
    /// Workspace name to drill into (optional)
    pub workspace: Option<String>,
    /// Instance ID to show full detail for (requires workspace)
    pub instance_id: Option<String>,
    /// Include full instance detail in human summaries
    #[arg(long)]
    pub detail: bool,
    /// Filter by instance state
    #[arg(long, value_name = "STATE", value_parser = ["running", "stopped", "paused", "restarting", "removing", "created", "dead", "missing", "unavailable"])]
    pub state: Option<String>,
    /// Output format
    #[arg(long, value_name = "FORMAT", default_value = "human")]
    pub format: String,
}

impl StatusArgs {
    pub fn output_format(&self) -> OutputFormat {
        OutputFormat::parse(&self.format)
    }
}

pub async fn run(args: &StatusArgs, paths: &JackinPaths) -> anyhow::Result<()> {
    let docker = BollardDockerClient::connect()?;
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir)?;
    let rows = collect_instances(args, &index.instances, &docker).await?;
    render_status(args, &rows, &mut std::io::stdout().lock())
}

#[derive(Debug)]
enum AgentHydration {
    Ready(Vec<AgentRegistryEntry>),
    Pending,
    Unavailable,
    Stopped,
}

impl AgentHydration {
    fn status(&self) -> &'static str {
        match self {
            Self::Ready(_) => "ready",
            Self::Pending => "pending",
            Self::Unavailable => "unavailable",
            Self::Stopped => "stopped",
        }
    }

    fn entries(&self) -> Option<&[AgentRegistryEntry]> {
        match self {
            Self::Ready(entries) => Some(entries),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct HydratedInstance {
    entry: InstanceIndexEntry,
    state: ContainerState,
    branch: Option<String>,
    pr: Option<PrInfo>,
    agents: AgentHydration,
}

impl HydratedInstance {
    fn workspace(&self) -> &str {
        self.entry
            .workspace_name
            .as_deref()
            .unwrap_or(&self.entry.workspace_label)
    }

    fn json(&self) -> serde_json::Value {
        let pr = self.pr.as_ref().map(|pr| {
            serde_json::json!({
                "number": pr.number,
                "title": pr.title,
                "url": pr.url,
                "ci_status": pr.ci.status(),
                "ci_failing_check": pr.ci.failing_check(),
            })
        });
        serde_json::json!({
            "instance_id": self.entry.instance_id,
            "container_base": self.entry.container_base,
            "workspace": self.workspace(),
            "role": self.entry.role_key,
            "state": self.state.short_label(),
            "branch": self.branch,
            "pull_request": pr,
            "agents": self.agents.entries(),
            "agents_status": self.agents.status(),
        })
    }
}

fn state_key(state: &ContainerState) -> &'static str {
    match state {
        ContainerState::Running => "running",
        ContainerState::Stopped { .. } => "stopped",
        ContainerState::Paused => "paused",
        ContainerState::Restarting => "restarting",
        ContainerState::Removing => "removing",
        ContainerState::Created => "created",
        ContainerState::Dead => "dead",
        ContainerState::NotFound => "missing",
        ContainerState::InspectUnavailable(_) => "unavailable",
    }
}

async fn collect_instances(
    args: &StatusArgs,
    entries: &[InstanceIndexEntry],
    docker: &impl DockerApi,
) -> anyhow::Result<Vec<HydratedInstance>> {
    if let Some(filter) = &args.state {
        anyhow::ensure!(
            [
                "running",
                "stopped",
                "paused",
                "restarting",
                "removing",
                "created",
                "dead",
                "missing",
                "unavailable"
            ]
            .contains(&filter.as_str()),
            "unknown instance state {filter:?}"
        );
    }
    anyhow::ensure!(
        args.instance_id.is_none() || args.workspace.is_some(),
        "instance ID requires a workspace"
    );
    let selected: Vec<_> = entries
        .iter()
        .filter(|entry| {
            args.workspace.as_deref().is_none_or(|workspace| {
                entry.workspace_name.as_deref() == Some(workspace)
                    || entry.workspace_label == workspace
            }) && args
                .instance_id
                .as_deref()
                .is_none_or(|id| entry.instance_id == id)
        })
        .collect();
    if selected.is_empty() && args.workspace.is_some() {
        anyhow::bail!("no instances found for requested workspace or instance");
    }
    let mut rows = Vec::new();
    for entry in selected {
        let inspection = docker
            .inspect_container_by_name(&entry.container_base)
            .await;
        if args
            .state
            .as_deref()
            .is_some_and(|filter| state_key(&inspection.state) != filter)
        {
            continue;
        }
        rows.push(hydrate_instance(entry, inspection, docker).await);
    }
    rows.sort_by(|a, b| {
        (a.workspace(), &a.entry.instance_id).cmp(&(b.workspace(), &b.entry.instance_id))
    });
    Ok(rows)
}

async fn hydrate_instance(
    entry: &InstanceIndexEntry,
    inspection: jackin_core::ContainerInspection,
    docker: &impl DockerApi,
) -> HydratedInstance {
    let mut row = HydratedInstance {
        entry: entry.clone(),
        state: inspection.state,
        branch: None,
        pr: None,
        agents: AgentHydration::Stopped,
    };
    if !matches!(row.state, ContainerState::Running) {
        row.agents = match row.state {
            ContainerState::Paused
            | ContainerState::Removing
            | ContainerState::InspectUnavailable(_) => AgentHydration::Unavailable,
            ContainerState::Restarting => AgentHydration::Pending,
            _ => AgentHydration::Stopped,
        };
        return row;
    }
    row.agents = AgentHydration::Unavailable;
    let Some(container) = inspection.handle else {
        return row;
    };
    let agents_command = jackin_agents_command();
    row.agents = match docker
        .exec_capture_by_id(&container, &["sh", "-c", &agents_command])
        .await
    {
        Ok(output) if output.trim() == AGENTS_PENDING => AgentHydration::Pending,
        Ok(output) => match serde_json::from_str::<Vec<AgentRegistryEntry>>(&output) {
            Ok(mut agents) => {
                agents.sort_by(|a, b| {
                    (a.status != "active", &a.started_at, &a.codename).cmp(&(
                        b.status != "active",
                        &b.started_at,
                        &b.codename,
                    ))
                });
                AgentHydration::Ready(agents)
            }
            Err(_) => AgentHydration::Unavailable,
        },
        Err(_) => AgentHydration::Unavailable,
    };
    row.branch = docker
        .exec_capture_by_id(&container, &["sh", "-c", GIT_BRANCH_CMD])
        .await
        .ok()
        .map(|branch| branch.trim().to_owned())
        .filter(|branch| !branch.is_empty() && branch != "unknown");
    if row.branch.is_some() {
        row.pr = fetch_pr_info(docker, &container).await;
    }
    row
}

fn render_status(
    args: &StatusArgs,
    rows: &[HydratedInstance],
    output: &mut impl Write,
) -> anyhow::Result<()> {
    if args.output_format() == OutputFormat::Json {
        let envelope = match (&args.workspace, &args.instance_id) {
            (None, _) => {
                let mut workspaces: BTreeMap<&str, Vec<serde_json::Value>> = BTreeMap::new();
                for row in rows {
                    workspaces
                        .entry(row.workspace())
                        .or_default()
                        .push(row.json());
                }
                let workspaces: Vec<_> = workspaces.into_iter().map(|(workspace, instances)| {
                    serde_json::json!({"workspace": workspace, "instances": instances})
                }).collect();
                serde_json::json!({"schema_version": "v1", "workspaces": workspaces})
            }
            (Some(workspace), None) => serde_json::json!({
                "schema_version": "v1", "workspace": workspace,
                "instances": rows.iter().map(HydratedInstance::json).collect::<Vec<_>>()
            }),
            (Some(_), Some(_)) => serde_json::json!({
                "schema_version": "v1",
                "instances": rows.iter().map(HydratedInstance::json).collect::<Vec<_>>()
            }),
        };
        writeln!(output, "{}", serde_json::to_string_pretty(&envelope)?)?;
        return Ok(());
    }
    match (&args.workspace, &args.instance_id) {
        (None, _) => render_fleet(rows, output)?,
        (Some(workspace), None) => render_workspace(workspace, rows, output)?,
        (Some(_), Some(_)) => {
            if rows.is_empty() {
                writeln!(output, "No instances match the requested state.")?;
            }
            for row in rows {
                render_instance_detail(row, output)?;
            }
        }
    }
    if args.detail && args.instance_id.is_none() {
        for row in rows {
            render_instance_detail(row, output)?;
        }
    }
    Ok(())
}

fn render_fleet(rows: &[HydratedInstance], output: &mut impl Write) -> anyhow::Result<()> {
    write!(output, "{BANNER}")?;
    writeln!(output, "fleet status\n")?;
    if rows.is_empty() {
        writeln!(output, "No workspaces found.")?;
        return Ok(());
    }
    let mut workspaces: BTreeMap<&str, Vec<&HydratedInstance>> = BTreeMap::new();
    for row in rows {
        workspaces.entry(row.workspace()).or_default().push(row);
    }
    let width = workspaces
        .keys()
        .map(|name| name.len())
        .max()
        .unwrap_or(9)
        .max(9);
    writeln!(
        output,
        "  {:<width$}  {:<9}  state",
        "workspace", "instances"
    )?;
    writeln!(output, "  {}", "─".repeat(width + 43))?;
    for (workspace, instances) in &workspaces {
        let mut counts = BTreeMap::new();
        for row in instances {
            *counts.entry(state_key(&row.state)).or_insert(0_usize) += 1;
        }
        let states = counts
            .into_iter()
            .map(|(state, count)| format!("{count} {state}"))
            .collect::<Vec<_>>()
            .join(" · ");
        writeln!(
            output,
            "  {workspace:<width$}  {:<9}  {states}",
            instances.len()
        )?;
    }
    writeln!(
        output,
        "\n  {} workspaces, {} instances\n",
        workspaces.len(),
        rows.len()
    )?;
    writeln!(
        output,
        "  jackin status <workspace>           show instances"
    )?;
    writeln!(
        output,
        "  jackin status <workspace> <id>      show full detail"
    )?;
    Ok(())
}

fn render_workspace(
    workspace: &str,
    rows: &[HydratedInstance],
    output: &mut impl Write,
) -> anyhow::Result<()> {
    let running = rows
        .iter()
        .filter(|row| matches!(row.state, ContainerState::Running))
        .count();
    writeln!(
        output,
        "{workspace}   {} instances  ·  {running} running\n",
        rows.len()
    )?;
    let id_width = rows
        .iter()
        .map(|row| row.entry.instance_id.len())
        .max()
        .unwrap_or(11)
        .max(11);
    let role_width = rows
        .iter()
        .map(|row| row.entry.role_key.len())
        .max()
        .unwrap_or(4)
        .max(4);
    writeln!(
        output,
        "  {:<id_width$}  {:<role_width$}  {:<11}  pr",
        "instance", "role", "state"
    )?;
    writeln!(output, "  {}", "─".repeat(id_width + role_width + 29))?;
    for row in rows {
        let pr = row.pr.as_ref().map_or_else(
            || "—".to_owned(),
            |pr| format!("#{}  {}", pr.number, pr.title),
        );
        writeln!(
            output,
            "  {:<id_width$}  {:<role_width$}  {:<11}  {pr}",
            row.entry.instance_id,
            row.entry.role_key,
            row.state.short_label()
        )?;
    }
    writeln!(
        output,
        "\n  jackin status {workspace} <id>      show full detail"
    )?;
    Ok(())
}

fn render_instance_detail(row: &HydratedInstance, output: &mut impl Write) -> anyhow::Result<()> {
    writeln!(
        output,
        "\n{}   {} / {}   {}\n",
        row.entry.instance_id,
        row.workspace(),
        row.entry.role_key,
        row.state.inspect_label()
    )?;
    writeln!(
        output,
        "  branch   {}",
        row.branch.as_deref().unwrap_or("—")
    )?;
    if let Some(pr) = &row.pr {
        writeln!(output, "  pr       #{}  {}", pr.number, pr.title)?;
        writeln!(output, "  url      {}", pr.url)?;
        writeln!(output, "  ci       {}", pr.ci_display())?;
    } else {
        writeln!(output, "  pr       —\n  url      —\n  ci       —")?;
    }
    writeln!(output)?;
    if let AgentHydration::Ready(agents) = &row.agents {
        writeln!(
            output,
            "  {:<12}  {:<10}  {:<14}  {:<20}  {:<20}  status",
            "codename", "agent", "provider", "started", "exited"
        )?;
        writeln!(output, "  {}", "─".repeat(83))?;
        for agent in agents {
            writeln!(
                output,
                "  {:<12}  {:<10}  {:<14}  {:<20}  {:<20}  {}",
                agent.codename,
                agent.agent.as_deref().unwrap_or("shell"),
                agent.provider.as_deref().unwrap_or("—"),
                compact_ts(&agent.started_at),
                agent
                    .exited_at
                    .as_deref()
                    .map_or_else(|| "—".to_owned(), compact_ts),
                agent.status
            )?;
        }
    } else {
        writeln!(output, "  agents   {}", row.agents.status())?;
    }
    writeln!(output)?;
    Ok(())
}

#[cfg(test)]
mod hydration_tests;

// ── Helpers ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct PrInfo {
    number: u64,
    title: String,
    url: String,
    ci: CiResult,
}

impl PrInfo {
    fn ci_display(&self) -> String {
        match &self.ci {
            CiResult::Passing => "✓ passing".to_owned(),
            CiResult::Pending => "⏳ pending".to_owned(),
            CiResult::Failing(check) => check.as_ref().map_or_else(
                || "✗ failing".to_owned(),
                |check| format!("✗ failing — {check}"),
            ),
            CiResult::Unknown => "—".to_owned(),
        }
    }
}

async fn fetch_pr_info(docker: &impl DockerApi, container: &ContainerHandle) -> Option<PrInfo> {
    // Both exec failure (gh absent / no token) and parse failure mean "no PR info available";
    // .ok()? is intentional — these are expected, not bugs.
    let output = docker
        .exec_capture_by_id(container, &["sh", "-c", GH_PR_CMD])
        .await
        .ok()?;
    parse_pr_info(output.trim())
}

#[derive(Debug, PartialEq, Eq)]
enum CiResult {
    Passing,
    Pending,
    Failing(Option<String>),
    Unknown,
}

impl CiResult {
    fn status(&self) -> &'static str {
        match self {
            Self::Passing => "passing",
            Self::Pending => "pending",
            Self::Failing(_) => "failing",
            Self::Unknown => "—",
        }
    }

    fn failing_check(&self) -> Option<&str> {
        match self {
            Self::Failing(name) => name.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct GithubPr {
    number: u64,
    title: String,
    url: String,
    #[serde(rename = "statusCheckRollup", default)]
    checks: Option<Vec<GithubCheck>>,
}

/// GitHub's rollup is a union: commit statuses use state/context, check runs
/// use status/conclusion/name. Keep their fields distinct at the input boundary.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "__typename")]
enum GithubCheck {
    CheckRun {
        status: String,
        conclusion: Option<String>,
        name: Option<String>,
    },
    StatusContext {
        state: String,
        context: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

impl GithubCheck {
    fn result(&self) -> CiResult {
        match self {
            Self::CheckRun {
                status,
                conclusion,
                name,
            } => match status.as_str() {
                "COMPLETED" => match conclusion.as_deref() {
                    Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => CiResult::Passing,
                    Some(
                        "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED"
                        | "STARTUP_FAILURE" | "STALE",
                    ) => CiResult::Failing(name.clone().filter(|name| !name.trim().is_empty())),
                    _ => CiResult::Unknown,
                },
                "IN_PROGRESS" | "QUEUED" | "WAITING" | "PENDING" | "REQUESTED" => CiResult::Pending,
                _ => CiResult::Unknown,
            },
            Self::StatusContext { state, context } => match state.as_str() {
                "SUCCESS" => CiResult::Passing,
                "FAILURE" | "ERROR" => {
                    CiResult::Failing(context.clone().filter(|name| !name.trim().is_empty()))
                }
                "PENDING" | "EXPECTED" => CiResult::Pending,
                _ => CiResult::Unknown,
            },
            Self::Unknown => CiResult::Unknown,
        }
    }
}

fn parse_pr_info(output: &str) -> Option<PrInfo> {
    let pr: GithubPr = serde_json::from_str(output).ok()?;
    Some(PrInfo {
        number: pr.number,
        title: pr.title,
        url: pr.url,
        ci: aggregate_ci_status(pr.checks.as_deref().unwrap_or_default()),
    })
}

fn aggregate_ci_status(checks: &[GithubCheck]) -> CiResult {
    if checks.is_empty() {
        return CiResult::Unknown;
    }
    let mut any_failure = false;
    let mut failing_check = None;
    let mut any_pending = false;
    let mut any_unknown = false;
    for check in checks {
        match check.result() {
            CiResult::Failing(name) => {
                any_failure = true;
                if failing_check.is_none() {
                    failing_check = name;
                }
            }
            CiResult::Pending => any_pending = true,
            CiResult::Unknown => any_unknown = true,
            CiResult::Passing => {}
        }
    }
    if any_failure {
        CiResult::Failing(failing_check)
    } else if any_pending {
        CiResult::Pending
    } else if any_unknown {
        CiResult::Unknown
    } else {
        CiResult::Passing
    }
}

/// Compact ISO 8601 timestamp for table display: `2026-06-04 10:15:02`.
fn compact_ts(ts: &str) -> String {
    ts.trim_end_matches('Z').replace('T', " ")
}

#[cfg(test)]
mod tests {
    use super::{CiResult, GithubCheck, aggregate_ci_status, parse_pr_info};
    use serde_json::{Value, json};

    fn aggregate(checks: Value) -> CiResult {
        let checks: Vec<GithubCheck> = serde_json::from_value(checks).unwrap();
        aggregate_ci_status(&checks)
    }

    fn check_run(status: &str, conclusion: Option<&str>) -> Value {
        json!({
            "__typename": "CheckRun",
            "status": status,
            "conclusion": conclusion,
            "name": "build"
        })
    }

    fn commit_status(state: &str) -> Value {
        json!({"__typename": "StatusContext", "state": state, "context": "legacy CI"})
    }

    #[test]
    fn ci_rollup_handles_both_github_union_members() {
        for (state, expected) in [
            ("SUCCESS", CiResult::Passing),
            ("PENDING", CiResult::Pending),
            ("EXPECTED", CiResult::Pending),
            ("FAILURE", CiResult::Failing(Some("legacy CI".to_owned()))),
            ("ERROR", CiResult::Failing(Some("legacy CI".to_owned()))),
        ] {
            assert_eq!(
                aggregate(json!([
                    check_run("COMPLETED", Some("SUCCESS")),
                    commit_status(state)
                ])),
                expected,
                "commit status {state}"
            );
        }
    }

    #[test]
    fn ci_rollup_classifies_all_check_run_states_and_conclusions() {
        for status in ["IN_PROGRESS", "QUEUED", "WAITING", "PENDING", "REQUESTED"] {
            assert_eq!(
                aggregate(json!([check_run(status, None)])),
                CiResult::Pending
            );
        }
        for conclusion in ["SUCCESS", "NEUTRAL", "SKIPPED"] {
            assert_eq!(
                aggregate(json!([check_run("COMPLETED", Some(conclusion))])),
                CiResult::Passing
            );
        }
        for conclusion in [
            "FAILURE",
            "ERROR",
            "TIMED_OUT",
            "CANCELLED",
            "ACTION_REQUIRED",
            "STARTUP_FAILURE",
            "STALE",
        ] {
            assert_eq!(
                aggregate(json!([check_run("COMPLETED", Some(conclusion))])),
                CiResult::Failing(Some("build".to_owned()))
            );
        }
    }

    #[test]
    fn ci_failure_does_not_require_a_check_name() {
        for failed in [
            json!({"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "FAILURE"}),
            json!({"__typename": "StatusContext", "state": "ERROR"}),
            json!({"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "FAILURE", "name": ""}),
            json!({"__typename": "StatusContext", "state": "ERROR", "context": "  "}),
        ] {
            assert_eq!(
                aggregate(json!([commit_status("PENDING"), failed])),
                CiResult::Failing(None)
            );
        }
    }

    #[test]
    fn ci_failure_precedes_pending_and_preserves_first_available_name() {
        assert_eq!(
            aggregate(json!([
                commit_status("PENDING"),
                {"__typename": "StatusContext", "state": "FAILURE"},
                {"__typename": "StatusContext", "state": "FAILURE", "context": "  "},
                check_run("COMPLETED", Some("FAILURE")),
                commit_status("ERROR")
            ])),
            CiResult::Failing(Some("build".to_owned()))
        );
    }

    #[test]
    fn unknown_or_empty_ci_never_implies_passing() {
        assert_eq!(aggregate(json!([])), CiResult::Unknown);
        for unknown in [
            check_run("NEW_STATE", Some("SUCCESS")),
            check_run("COMPLETED", Some("NEW_CONCLUSION")),
            check_run("COMPLETED", None),
            commit_status("NEW_STATE"),
            json!({"__typename": "FutureCheckType"}),
        ] {
            assert_eq!(
                aggregate(json!([commit_status("SUCCESS"), unknown])),
                CiResult::Unknown
            );
        }
    }

    #[test]
    fn pr_hydration_preserves_legacy_failure_in_json_and_human_output() {
        let output = json!({
            "number": 42,
            "title": "Fix CI",
            "url": "https://github.com/example/repo/pull/42",
            "statusCheckRollup": [commit_status("FAILURE")]
        });
        let pr = parse_pr_info(&output.to_string()).unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.title, "Fix CI");
        assert_eq!(pr.url, "https://github.com/example/repo/pull/42");
        assert_eq!(pr.ci.status(), "failing");
        assert_eq!(pr.ci.failing_check(), Some("legacy CI"));
        assert_eq!(pr.ci_display(), "✗ failing — legacy CI");
    }

    #[test]
    fn pr_hydration_handles_pending_unnamed_failure_and_absent_ci() {
        for (rollup, status, name, display) in [
            (
                json!([commit_status("PENDING")]),
                "pending",
                None,
                "⏳ pending",
            ),
            (
                json!([{"__typename": "StatusContext", "state": "FAILURE"}]),
                "failing",
                None,
                "✗ failing",
            ),
            (Value::Null, "—", None, "—"),
            (json!([]), "—", None, "—"),
        ] {
            let output =
                json!({"number": 1, "title": "PR", "url": "url", "statusCheckRollup": rollup});
            let pr = parse_pr_info(&output.to_string()).unwrap();
            assert_eq!(pr.ci.status(), status);
            assert_eq!(pr.ci.failing_check(), name);
            assert_eq!(pr.ci_display(), display);
        }
    }

    #[test]
    fn malformed_rollup_cannot_hydrate_a_passing_pr() {
        for malformed in [
            json!({}),
            json!([{"__typename": "StatusContext"}]),
            json!([{"__typename": "CheckRun", "status": false}]),
            json!([{"state": "SUCCESS"}]),
        ] {
            let output =
                json!({"number": 1, "title": "PR", "url": "url", "statusCheckRollup": malformed});
            assert!(parse_pr_info(&output.to_string()).is_none());
        }
    }
}
