use crate::docker::CommandRunner;

use super::naming::{FILTER_KIND_ROLE, format_role_display};

pub fn list_running_agent_names(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<String>> {
    list_role_names(runner, false)
}

pub fn list_managed_role_names(runner: &mut impl CommandRunner) -> anyhow::Result<Vec<String>> {
    list_role_names(runner, true)
}

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
                FILTER_KIND_ROLE,
                "--format",
                "{{.Names}}",
            ],
            None,
        )?
    } else {
        runner.capture(
            "docker",
            &["ps", "--filter", FILTER_KIND_ROLE, "--format", "{{.Names}}"],
            None,
        )?
    };

    Ok(role_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect())
}

/// List running roles with human-friendly display names.
///
/// Returns display names like "The Architect (k7p9m2xq)". Falls back
/// to the raw container name when no display label is present.
pub fn list_running_agent_display_names(
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<String>> {
    let output = runner.capture(
        "docker",
        &[
            "ps",
            "--filter",
            FILTER_KIND_ROLE,
            "--format",
            "{{.Names}}\t{{.Label \"jackin.display_name\"}}",
        ],
        None,
    )?;

    Ok(output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            let container_name = parts[0];
            let display_name = parts.get(1).unwrap_or(&"");
            format_role_display(container_name, display_name)
        })
        .collect())
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
            call == "docker ps -a --filter label=jackin.kind=role --format {{.Names}}"
        }));
    }

    #[test]
    fn list_running_agent_display_names_excludes_dind_sidecars() {
        let mut runner =
            FakeRunner::with_capture_queue(["jackin-agentsmith-k7p9m2xq\tAgent Smith".to_string()]);

        let names = list_running_agent_display_names(&mut runner).unwrap();

        // Instance ID is appended so concurrent sessions render distinctly.
        assert_eq!(names, vec!["Agent Smith (k7p9m2xq)"]);
        assert!(runner.recorded.iter().any(|call| {
            call == "docker ps --filter label=jackin.kind=role --format {{.Names}}\t{{.Label \"jackin.display_name\"}}"
        }));
    }
}
