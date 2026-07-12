use clap::Args;
use std::collections::HashMap;

/// Poll futures sequentially and collect results.
///
/// Sequential is adequate for small fleet sizes (< ~20 containers); each Docker
/// inspect round-trip is fast on localhost. Replace with `tokio::task::JoinSet`
/// if fleet size grows large enough to make serial inspects a bottleneck.
async fn poll_sequential<F, T>(futs: impl IntoIterator<Item = F>) -> Vec<T>
where
    F: Future<Output = T>,
{
    let mut results = Vec::new();
    for fut in futs {
        results.push(fut.await);
    }
    results
}

use crate::cli::BANNER;
use crate::cli::format::OutputFormat;
use jackin_core::JackinPaths;
use jackin_docker::docker_client::{BollardDockerClient, ContainerState, DockerApi};
use jackin_runtime::instance::manifest::InstanceIndex;

/// Command string for querying the agent registry over the capsule socket.
const JACKIN_AGENTS_CMD: &str =
    "test -S /jackin/run/jackin.sock && /jackin/runtime/jackin-capsule agents --format json";

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
    /// Show agent counts at Level 0 (requires in-container queries per instance)
    #[arg(long)]
    pub detail: bool,
    /// Filter by instance state
    #[arg(long, value_name = "STATE")]
    pub state: Option<String>,
    /// Filter by agent type (e.g. `--filter agent=claude`)
    #[arg(long, value_name = "KEY=VALUE")]
    pub filter: Option<String>,
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
    let format = args.output_format();

    match (&args.workspace, &args.instance_id) {
        (None, _) => run_level0(args, paths, &docker, format).await,
        (Some(ws), None) => run_level1(ws, args, paths, &docker, format).await,
        (Some(ws), Some(id)) => run_level2(ws, id, paths, &docker, format).await,
    }
}

// ── Level 0 — workspace summary ─────────────────────────────────────────────

#[allow(clippy::too_many_lines, reason = "documented residual allow; prefer expect when site is lint-true")]
async fn run_level0(
    args: &StatusArgs,
    paths: &JackinPaths,
    docker: &impl DockerApi,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir)?;

    // Group by workspace name/label.
    let mut workspaces: HashMap<
        String,
        Vec<&jackin_runtime::instance::manifest::InstanceIndexEntry>,
    > = HashMap::new();
    for entry in &index.instances {
        let key = entry
            .workspace_name
            .clone()
            .unwrap_or_else(|| entry.workspace_label.clone());
        workspaces.entry(key).or_default().push(entry);
    }

    if format == OutputFormat::Json {
        // Full JSON always has complete data — gather running state per instance.
        let mut workspace_data = Vec::new();
        for (name, entries) in &workspaces {
            let mut instances = Vec::new();
            for entry in entries {
                let state = docker.inspect_container_state(&entry.container_base).await;
                instances.push(serde_json::json!({
                    "instance_id": entry.instance_id,
                    "container_base": entry.container_base,
                    "role": entry.role_key,
                    "state": state.short_label(),
                }));
            }
            workspace_data.push(serde_json::json!({
                "workspace": name,
                "instances": instances,
            }));
        }
        let envelope = serde_json::json!({
            "schema_version": "v1",
            "workspaces": workspace_data,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    // Human output.
    let mut sorted_ws: Vec<(
        String,
        Vec<&jackin_runtime::instance::manifest::InstanceIndexEntry>,
    )> = workspaces.into_iter().collect();
    sorted_ws.sort_by(|a, b| a.0.cmp(&b.0));

    print!("{BANNER}");
    println!("fleet status\n");

    // Query container states for each workspace (sequential; fast enough for small fleets).
    let mut workspace_rows: Vec<(String, usize, usize, usize)> = Vec::new(); // (name, total, running, stopped)
    for (ws_name, entries) in &sorted_ws {
        let states = poll_sequential(
            entries
                .iter()
                .map(|e| docker.inspect_container_state(&e.container_base)),
        )
        .await;
        let running = states
            .iter()
            .filter(|s| matches!(s, ContainerState::Running))
            .count();
        let stopped = entries.len() - running;
        workspace_rows.push((ws_name.clone(), entries.len(), running, stopped));
    }

    // Apply --state filter.
    let state_filter = args.state.as_deref();
    let filtered: Vec<_> = workspace_rows
        .iter()
        .filter(|(_, _, running, stopped)| match state_filter {
            Some("running") => *running > 0,
            Some("stopped") => *stopped > 0,
            _ => true,
        })
        .collect();

    if filtered.is_empty() {
        println!("No workspaces found.");
        return Ok(());
    }

    // Column widths.
    let ws_width = filtered
        .iter()
        .map(|(n, _, _, _)| n.len())
        .max()
        .unwrap_or(9)
        .max(9);
    println!("  {:<ws_width$}  {:<9}  state", "workspace", "instances");
    println!("  {}", "─".repeat(ws_width + 2 + 9 + 2 + 30));

    for (ws_name, total, running, stopped) in &filtered {
        let state = if *running > 0 && *stopped > 0 {
            format!("{running} running · {stopped} stopped")
        } else if *running > 0 {
            format!("{running} running")
        } else {
            format!("{stopped} stopped")
        };
        println!("  {ws_name:<ws_width$}  {total:<9}  {state}");
    }

    let _total_instances: usize = filtered.iter().map(|(_, t, _, _)| t).sum();
    let total_running: usize = filtered.iter().map(|(_, _, r, _)| r).sum();
    let total_stopped: usize = filtered.iter().map(|(_, _, _, s)| s).sum();
    println!();
    print!(
        "  {} workspace{}, {} instance{} running",
        filtered.len(),
        if filtered.len() == 1 { "" } else { "s" },
        total_running,
        if total_running == 1 { "" } else { "s" },
    );
    if total_stopped > 0 {
        print!(", {total_stopped} stopped");
    }
    println!("\n");
    println!("  jackin status <workspace>           show instances");
    println!("  jackin status <workspace> <id>      show full detail");
    if !args.detail {
        println!("  jackin status --detail              include per-instance agent counts");
    }

    Ok(())
}

// ── Level 1 — instance list for a workspace ──────────────────────────────────

async fn run_level1(
    workspace: &str,
    args: &StatusArgs,
    paths: &JackinPaths,
    docker: &impl DockerApi,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir)?;
    let instances: Vec<_> = index
        .instances
        .iter()
        .filter(|e| {
            e.workspace_name.as_deref() == Some(workspace) || e.workspace_label == workspace
        })
        .collect();

    if instances.is_empty() {
        anyhow::bail!("no instances found for workspace {workspace:?}");
    }

    // Gather state for each instance.
    let states = poll_sequential(
        instances
            .iter()
            .map(|e| docker.inspect_container_state(&e.container_base)),
    )
    .await;

    // Apply state filter.
    let state_filter = args.state.as_deref();
    let rows: Vec<_> = instances
        .iter()
        .zip(states.iter())
        .filter(|(_, s)| match state_filter {
            Some("running") => matches!(s, ContainerState::Running),
            Some("stopped") => !matches!(s, ContainerState::Running),
            _ => true,
        })
        .collect();

    if format == OutputFormat::Json {
        let json_rows: Vec<_> = rows
            .iter()
            .map(|(e, s)| {
                serde_json::json!({
                    "instance_id": e.instance_id,
                    "workspace": workspace,
                    "role": e.role_key,
                    "state": s.short_label(),
                })
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": "v1",
            "workspace": workspace,
            "instances": json_rows,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    let running = rows
        .iter()
        .filter(|(_, s)| matches!(s, ContainerState::Running))
        .count();
    println!(
        "{workspace}   {} instance{}  ·  {running} running\n",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    );

    let id_width = rows
        .iter()
        .map(|(e, _)| e.instance_id.len())
        .max()
        .unwrap_or(11)
        .max(11);
    let role_width = rows
        .iter()
        .map(|(e, _)| e.role_key.len())
        .max()
        .unwrap_or(4)
        .max(4);

    println!(
        "  {:<id_width$}  {:<role_width$}  {:<8}  pr",
        "instance", "role", "state"
    );
    println!(
        "  {}",
        "─".repeat(id_width + 2 + role_width + 2 + 8 + 2 + 10)
    );

    for (entry, state) in &rows {
        // "—" for pr: Level 2 detail query needed for that
        println!(
            "  {:<id_width$}  {:<role_width$}  {:<8}  —",
            entry.instance_id,
            entry.role_key,
            state.short_label(),
        );
    }

    println!();
    println!("  jackin status {workspace} <id>      show full detail");

    Ok(())
}

// ── Level 2 — full instance detail ───────────────────────────────────────────

#[allow(clippy::too_many_lines, reason = "documented residual allow; prefer expect when site is lint-true")]
async fn run_level2(
    workspace: &str,
    instance_id: &str,
    paths: &JackinPaths,
    docker: &impl DockerApi,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir)?;
    let entry = index.instances.iter().find(|e| {
        e.instance_id == instance_id
            && (e.workspace_name.as_deref() == Some(workspace) || e.workspace_label == workspace)
    });

    let entry = entry.ok_or_else(|| {
        anyhow::anyhow!("instance {instance_id:?} not found in workspace {workspace:?}")
    })?;

    let container_name = &entry.container_base;
    let state = docker.inspect_container_state(container_name).await;
    let is_running = matches!(state, ContainerState::Running);

    // Fetch agents registry (only when running).
    #[allow(clippy::option_if_let_else, reason = "documented residual allow; prefer expect when site is lint-true")]
    let agents_json: Option<Vec<jackin_protocol::control::AgentRegistryEntry>> = if is_running {
        match docker
            .exec_capture(container_name, &["sh", "-c", JACKIN_AGENTS_CMD])
            .await
        {
            Err(_) => None, // socket not yet up or container exec failed — expected transient
            Ok(s) => match serde_json::from_str(&s) {
                Ok(v) => Some(v),
                Err(e) => {
                    // Exec succeeded but output is not valid JSON — protocol or version mismatch.
                    eprintln!("warning: agents registry parse error for {container_name}: {e:#}");
                    None
                }
            },
        }
    } else {
        None
    };

    // Fetch git branch (only when running). .ok() is intentional: exec failure
    // is a transient or expected case (container just confirmed up but git not
    // available), and GIT_BRANCH_CMD already suppresses git errors with `2>/dev/null`.
    let branch: Option<String> = if is_running {
        docker
            .exec_capture(container_name, &["sh", "-c", GIT_BRANCH_CMD])
            .await
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty() && s != "unknown")
    } else {
        None
    };

    // Fetch PR info via gh (only when running and branch is known).
    let pr_info: Option<PrInfo> = if is_running && branch.is_some() {
        fetch_pr_info(docker, container_name).await
    } else {
        None
    };

    if format == OutputFormat::Json {
        let agents_value = match &agents_json {
            None => serde_json::Value::Null,
            Some(a) => serde_json::to_value(a)?,
        };
        let pr_value = pr_info.as_ref().map_or(serde_json::Value::Null, |p| {
            serde_json::json!({
                "number": p.number,
                "title": p.title,
                "url": p.url,
                "ci_status": p.ci_status,
                "ci_failing_check": p.ci_failing_check,
            })
        });
        let envelope = serde_json::json!({
            "schema_version": "v1",
            "instances": [{
                "instance_id": entry.instance_id,
                "workspace": workspace,
                "role": entry.role_key,
                "state": state.short_label(),
                "branch": branch,
                "pull_request": pr_value,
                "agents": agents_value,
            }],
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    // Human output.
    println!(
        "\n{}   {} / {}   {}",
        entry.instance_id,
        workspace,
        entry.role_key,
        state.short_label()
    );
    println!();

    // Branch / PR / CI block.
    println!("  branch   {}", branch.as_deref().unwrap_or("—"));
    if let Some(pr) = &pr_info {
        println!("  pr       #{}  {}", pr.number, pr.title);
        println!("  url      {}", pr.url);
        println!("  ci       {}", pr.ci_display());
    } else {
        println!("  pr       —");
        println!("  url      —");
        println!("  ci       —");
    }
    println!();

    // Agent table.
    if let Some(agents) = &agents_json {
        println!(
            "  {:<12}  {:<10}  {:<14}  {:<20}  {:<20}  status",
            "codename", "agent", "provider", "started", "exited"
        );
        println!("  {}", "─".repeat(83));

        let mut active: Vec<_> = agents.iter().filter(|a| a.status == "active").collect();
        let mut exited: Vec<_> = agents.iter().filter(|a| a.status != "active").collect();
        active.sort_by(|a, b| a.started_at.cmp(&b.started_at));
        exited.sort_by(|a, b| a.started_at.cmp(&b.started_at));

        for a in active.iter().chain(exited.iter()) {
            println!(
                "  {:<12}  {:<10}  {:<14}  {:<20}  {:<20}  {}",
                a.codename,
                a.agent.as_deref().unwrap_or("shell"),
                a.provider.as_deref().unwrap_or("—"),
                compact_ts(&a.started_at),
                a.exited_at
                    .as_deref()
                    .map_or_else(|| "—".to_owned(), compact_ts),
                a.status,
            );
        }
    } else if is_running {
        println!("  agents   pending");
    } else {
        println!("  agents   —");
    }
    println!();

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct PrInfo {
    number: u64,
    title: String,
    url: String,
    ci_status: String,
    ci_failing_check: Option<String>,
}

impl PrInfo {
    fn ci_display(&self) -> String {
        match self.ci_status.as_str() {
            "passing" | "success" => "✓ passing".to_owned(),
            "pending" => "⏳ pending".to_owned(),
            "failing" | "failure" | "error" => self.ci_failing_check.as_ref().map_or_else(
                || "✗ failing".to_owned(),
                |check| format!("✗ failing — {check}"),
            ),
            _ => "—".to_owned(),
        }
    }
}

async fn fetch_pr_info(docker: &impl DockerApi, container_name: &str) -> Option<PrInfo> {
    // Both exec failure (gh absent / no token) and parse failure mean "no PR info available";
    // .ok()? is intentional — these are expected, not bugs.
    let output = docker
        .exec_capture(container_name, &["sh", "-c", GH_PR_CMD])
        .await
        .ok()?;
    let value: serde_json::Value = serde_json::from_str(output.trim()).ok()?;
    let number = value["number"].as_u64()?;
    let title = value["title"].as_str()?.to_owned();
    let url = value["url"].as_str()?.to_owned();

    // Aggregate statusCheckRollup into a single ci_status.
    let (ci_status, ci_failing_check) = aggregate_ci_status(&value["statusCheckRollup"]);

    Some(PrInfo {
        number,
        title,
        url,
        ci_status,
        ci_failing_check,
    })
}

fn aggregate_ci_status(rollup: &serde_json::Value) -> (String, Option<String>) {
    let Some(checks) = rollup.as_array() else {
        return ("—".to_owned(), None);
    };
    if checks.is_empty() {
        return ("—".to_owned(), None);
    }
    let mut failing_check = None;
    let mut any_pending = false;
    let mut all_pass = true;
    for check in checks {
        let conclusion = check
            .get("conclusion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let status = check.get("status").and_then(|v| v.as_str()).unwrap_or("");
        match conclusion {
            "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" => {
                all_pass = false;
                if failing_check.is_none() {
                    failing_check = check
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                }
            }
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => {}
            _ => {
                if status == "IN_PROGRESS" || status == "QUEUED" || status == "WAITING" {
                    any_pending = true;
                }
            }
        }
    }
    if failing_check.is_some() {
        ("failing".to_owned(), failing_check)
    } else if any_pending {
        ("pending".to_owned(), None)
    } else if all_pass {
        ("passing".to_owned(), None)
    } else {
        ("—".to_owned(), None)
    }
}

/// Compact ISO 8601 timestamp for table display: `2026-06-04 10:15:02`.
fn compact_ts(ts: &str) -> String {
    ts.trim_end_matches('Z').replace('T', " ")
}
