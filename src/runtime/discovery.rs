use crate::docker::CommandRunner;

use super::naming::{FILTER_MANAGED, FILTER_ROLE_AGENT, format_role_display};

pub fn list_running_agent_names(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<String>> {
    list_role_names(runner, false)
}

pub fn list_managed_role_names(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<String>> {
    list_role_names(runner, true)
}

pub(super) fn capture_managed_container_rows(
    runner: &mut impl CommandRunner,
    include_stopped: bool,
    format: &str,
) -> anyhow::Result<String> {
    if include_stopped {
        runner.capture(
            "docker",
            &["ps", "-a", "--filter", FILTER_MANAGED, "--format", format],
            None,
        )
    } else {
        runner.capture(
            "docker",
            &["ps", "--filter", FILTER_MANAGED, "--format", format],
            None,
        )
    }
}

fn list_legacy_managed_role_names(
    runner: &mut impl CommandRunner,
    include_stopped: bool,
) -> anyhow::Result<Vec<String>> {
    let output = capture_managed_container_rows(
        runner,
        include_stopped,
        "{{.Names}}\t{{.Label \"jackin.role\"}}\t{{.Label \"jackin.kind\"}}",
    )?;

    Ok(output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let name = parts.next()?;
            let role = parts.next().unwrap_or("");
            let kind = parts.next().unwrap_or("");
            if name.is_empty() || !role.is_empty() || !kind.is_empty() {
                return None;
            }
            Some(name.to_string())
        })
        .collect())
}

// `pub(crate)` so workspace-edit drift detection in `config/workspaces.rs`
// can see the raw running-list before `ensure_role_not_running`'s wrapper
// applies. Tagged with `allow(clippy::redundant_pub_crate)` because clippy's
// nursery flags `pub(crate)` inside private modules even when the wider
// visibility is intentional.
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn list_role_names(
    runner: &mut impl CommandRunner,
    include_stopped: bool,
) -> anyhow::Result<Vec<String>> {
    let role_output = if include_stopped {
        runner.capture(
            "docker",
            &[
                "ps",
                "-a",
                "--filter",
                FILTER_ROLE_AGENT,
                "--format",
                "{{.Names}}",
            ],
            None,
        )?
    } else {
        runner.capture(
            "docker",
            &[
                "ps",
                "--filter",
                FILTER_ROLE_AGENT,
                "--format",
                "{{.Names}}",
            ],
            None,
        )?
    };

    let mut names: Vec<String> = role_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect();
    names.extend(list_legacy_managed_role_names(runner, include_stopped)?);
    Ok(names)
}

/// List running roles with human-friendly display names.
///
/// Returns display names like "The Architect" or "The Architect (Clone 2)".
/// Falls back to the raw container name if no display label is present.
pub fn list_running_agent_display_names(
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<String>> {
    let output = runner.capture(
        "docker",
        &[
            "ps",
            "--filter",
            FILTER_ROLE_AGENT,
            "--format",
            "{{.Names}}\t{{.Label \"jackin.display_name\"}}",
        ],
        None,
    )?;

    let mut names: Vec<String> = output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            let container_name = parts[0];
            let display_name = parts.get(1).unwrap_or(&"");
            format_role_display(container_name, display_name)
        })
        .collect();

    let legacy_output = capture_managed_container_rows(
        runner,
        false,
        "{{.Names}}\t{{.Label \"jackin.display_name\"}}\t{{.Label \"jackin.role\"}}\t{{.Label \"jackin.kind\"}}",
    )?;
    names.extend(legacy_output.lines().filter_map(|line| {
        let mut parts = line.splitn(4, '\t');
        let container_name = parts.next()?;
        let display_name = parts.next().unwrap_or("");
        let role = parts.next().unwrap_or("");
        let kind = parts.next().unwrap_or("");
        if container_name.is_empty() || !role.is_empty() || !kind.is_empty() {
            return None;
        }
        Some(format_role_display(container_name, display_name))
    }));

    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::FakeRunner;
    use super::*;

    #[test]
    fn list_managed_agent_names_excludes_dind_sidecars() {
        let mut runner = FakeRunner::with_capture_queue(["jackin-agent-smith".to_string()]);

        let names = list_managed_role_names(&mut runner).unwrap();

        assert_eq!(names, vec!["jackin-agent-smith"]);
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.kind=agent --format {{.Names}}"
        }));
    }

    #[test]
    fn list_managed_agent_names_includes_legacy_agents_without_role_label() {
        let mut runner =
            FakeRunner::with_capture_queue([String::new(), "jackin-agent-smith\t\t".to_string()]);

        let names = list_managed_role_names(&mut runner).unwrap();

        assert_eq!(names, vec!["jackin-agent-smith"]);
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps -a --filter label=jackin.managed=true --format {{.Names}}\t{{.Label \"jackin.role\"}}\t{{.Label \"jackin.kind\"}}"
        }));
    }

    #[test]
    fn list_running_agent_display_names_excludes_dind_sidecars() {
        let mut runner =
            FakeRunner::with_capture_queue(["jackin-agent-smith\tAgent Smith".to_string()]);

        let names = list_running_agent_display_names(&mut runner).unwrap();

        assert_eq!(names, vec!["Agent Smith"]);
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps --filter label=jackin.kind=agent --format {{.Names}}\t{{.Label \"jackin.display_name\"}}"
        }));
    }
}
