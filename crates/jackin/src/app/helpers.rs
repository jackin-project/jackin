// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared helper functions for the `app` command dispatcher.

use anyhow::Result;

use jackin_config::{AppConfig, WorkspaceConfig};
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;
use jackin_runtime::instance;
use jackin_runtime::runtime;

pub(super) async fn resolve_role_to_container(
    class: &RoleSelector,
    docker: &impl DockerApi,
) -> Result<String> {
    let candidates =
        runtime::matching_family(class, &runtime::list_managed_role_names(docker).await?);
    match candidates.len() {
        1 => Ok(candidates.into_iter().next().unwrap()),
        0 => anyhow::bail!("no managed container found for role `{}`", class.key()),
        _ => anyhow::bail!(
            "multiple containers found for role `{}`: {}; pass a specific container name",
            class.key(),
            candidates.join(", ")
        ),
    }
}

pub(super) fn resolve_instance_reference(
    paths: &JackinPaths,
    input: &str,
) -> Result<Option<String>> {
    let index = instance::InstanceIndex::read_or_rebuild(&paths.data_dir)?;
    let mut matches = Vec::new();
    for entry in index.instances {
        if entry.status == instance::InstanceStatus::Purged {
            continue;
        }
        if entry.container_base == input || entry.instance_id == input {
            matches.push(entry.container_base);
        }
    }
    matches.sort();
    matches.dedup();

    match matches.as_slice() {
        [] => Ok(None),
        [container] => Ok(Some(container.clone())),
        _ => anyhow::bail!(
            "instance reference {input:?} is ambiguous; pass the full container name instead"
        ),
    }
}

/// Render the `workspace show <name>` output as a string. Includes the info
/// table (name/workdir/allowed/default-role), and, when there are mounts, a
/// trailing mounts table with one row per mount. The mounts table renders the
/// canonical lowercase isolation name (`shared`/`worktree`/`clone`) so the output
/// matches TOML/CLI input verbatim.
pub(super) fn render_workspace_show(
    config: &AppConfig,
    name: &str,
    workspace: &WorkspaceConfig,
) -> String {
    use std::fmt::Write as _;
    use tabled::settings::Style;
    use tabled::{Table, Tabled};

    #[derive(Tabled)]
    struct MountRow {
        #[tabled(rename = "Mount")]
        mount: String,
        #[tabled(rename = "Mode")]
        mode: String,
        #[tabled(rename = "Isolation")]
        isolation: String,
        #[tabled(rename = "Type")]
        kind: String,
    }
    #[derive(Tabled)]
    struct GlobalMountRowWithScope {
        #[tabled(rename = "Scope")]
        scope: String,
        #[tabled(rename = "Name")]
        name: String,
        #[tabled(rename = "Mount")]
        mount: String,
        #[tabled(rename = "Mode")]
        mode: String,
    }
    #[derive(Tabled)]
    struct GlobalMountRow {
        #[tabled(rename = "Name")]
        name: String,
        #[tabled(rename = "Mount")]
        mount: String,
        #[tabled(rename = "Mode")]
        mode: String,
    }

    let allowed = if workspace.allowed_roles.is_empty() {
        "any role".to_owned()
    } else {
        workspace.allowed_roles.join(", ")
    };
    let default_role = workspace.default_role.as_deref().unwrap_or("none");
    let agent = workspace.resolved_agent().slug();

    let short_workdir = jackin_core::shorten_home(&workspace.workdir);
    let mut info: Vec<(&str, &str)> = vec![
        ("Name", name),
        ("Workdir", short_workdir.as_str()),
        ("Allowed Roles", allowed.as_str()),
        ("Default Role", default_role),
        ("Agent", agent),
    ];
    // Only surface keep_awake when opted in — disabled is the default and
    // shouldn't add noise. When enabled, the operator sees it here so a
    // mysteriously sleepless Mac traces back to the workspace.
    if workspace.keep_awake.enabled {
        info.push(("Keep Awake", "enabled (macOS only)"));
    }
    if workspace.git_pull_on_entry {
        info.push(("Git Pull", "on entry"));
    }
    let mut info_table = Table::builder(info.iter().map(|(k, v)| [*k, *v])).build();
    info_table
        .with(Style::modern())
        .with(tabled::settings::Remove::row(
            tabled::settings::object::Rows::first(),
        ));

    let mut out = String::new();
    let _unused = writeln!(out, "{info_table}");

    if !workspace.mounts.is_empty() {
        let mount_rows: Vec<MountRow> = workspace
            .mounts
            .iter()
            .map(|m| MountRow {
                mount: mount_display(&m.src, &m.dst),
                mode: mount_mode(m.readonly),
                isolation: m.isolation.as_str().to_owned(),
                kind: jackin_console::mount_info::inspect(&m.src).label(),
            })
            .collect();
        let mut mount_table = Table::new(mount_rows);
        mount_table.with(Style::modern());
        let _unused = writeln!(out);
        let _unused = writeln!(out, "Workspace mounts:");
        let _unused = writeln!(out, "{mount_table}");
    }

    let render_unscoped_table = |out: &mut String, rows: &[&jackin_config::GlobalMountRow]| {
        if rows.is_empty() {
            return;
        }
        let mut table = Table::new(rows.iter().map(|row| GlobalMountRow {
            name: row.name.clone(),
            mount: mount_display(&row.mount.src, &row.mount.dst),
            mode: mount_mode(row.mount.readonly),
        }));
        table.with(Style::modern());
        let _unused = writeln!(out);
        let _unused = writeln!(out, "Global mounts:");
        let _unused = writeln!(out, "{table}");
    };

    match config.workspace_applicable_mount_rows(workspace) {
        jackin_config::WorkspaceGlobalMountRows::Applicable { role, rows } => {
            if rows.is_empty() {
                return out;
            }
            let has_scoped_rows = rows.iter().any(|row| row.scope.is_some());
            if !has_scoped_rows {
                render_unscoped_table(&mut out, &rows.iter().collect::<Vec<_>>());
                return out;
            }
            let mut table = Table::new(rows.iter().map(|row| GlobalMountRowWithScope {
                scope: row.scope.as_deref().unwrap_or("global").to_owned(),
                name: row.name.clone(),
                mount: mount_display(&row.mount.src, &row.mount.dst),
                mode: mount_mode(row.mount.readonly),
            }));
            table.with(Style::modern());
            let _unused = writeln!(out);
            let _unused = writeln!(out, "Global mounts ({role}):");
            let _unused = writeln!(out, "{table}");
        }
        jackin_config::WorkspaceGlobalMountRows::Ambiguous { candidates } => {
            // Unscoped global mounts apply regardless of role — render
            // them even when the role is ambiguous. Only the scoped
            // subset depends on role selection.
            let all_rows = config.list_mount_rows();
            let unscoped: Vec<&jackin_config::GlobalMountRow> =
                all_rows.iter().filter(|row| row.scope.is_none()).collect();
            render_unscoped_table(&mut out, &unscoped);
            if all_rows.iter().any(|row| row.scope.is_some()) {
                let _unused = writeln!(out);
                let _unused = writeln!(
                    out,
                    "Role-scoped global mounts depend on selected role ({})",
                    candidates.join(", ")
                );
            }
        }
    }

    out
}

pub(super) fn mount_mode(readonly: bool) -> String {
    if readonly { "read-only" } else { "read-write" }.to_owned()
}

pub(super) fn mount_display(src: &str, dst: &str) -> String {
    let short_dst = jackin_core::shorten_home(dst);
    if src == dst {
        short_dst
    } else {
        format!("{}\nhost: {}", short_dst, jackin_core::shorten_home(src))
    }
}
