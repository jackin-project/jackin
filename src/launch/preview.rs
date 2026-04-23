use super::render::colors;
use super::state::WorkspaceChoice;
use crate::config::AppConfig;
use crate::selector::ClassSelector;
use crate::tui;
use crate::workspace::ResolvedWorkspace;

pub(super) fn resolve_selected_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    agent: &ClassSelector,
) -> anyhow::Result<ResolvedWorkspace> {
    crate::workspace::resolve_load_workspace(config, agent, cwd, choice.input.clone(), &[])
}

pub(super) fn build_agent_detail_lines(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    agent: Option<&ClassSelector>,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let mut detail_lines: Vec<Line<'static>> = Vec::new();

    let Some(agent) = agent else {
        detail_lines.push(Line::from(Span::styled(
            "No agents match the current filter.",
            Style::default().fg(colors::DIM_WHITE),
        )));
        return detail_lines;
    };

    detail_lines.push(Line::from(vec![
        Span::styled("agent    ", Style::default().fg(colors::BRIGHT_BLUE)),
        Span::styled(
            agent.key(),
            Style::default()
                .fg(colors::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    match resolve_selected_workspace(config, cwd, choice, agent) {
        Ok(workspace) => {
            let workspace_destinations = choice
                .workspace
                .mounts
                .iter()
                .map(|mount| mount.dst.as_str())
                .collect::<std::collections::HashSet<_>>();

            detail_lines.push(Line::from(vec![
                Span::styled("workdir  ", Style::default().fg(colors::BRIGHT_BLUE)),
                Span::styled(
                    tui::shorten_home(&workspace.workdir),
                    Style::default().fg(colors::WHITE),
                ),
            ]));

            if !workspace.mounts.is_empty() {
                detail_lines.push(Line::from(""));
                detail_lines.push(Line::from(Span::styled(
                    "resolved mounts",
                    Style::default()
                        .fg(colors::BRIGHT_BLUE)
                        .add_modifier(Modifier::BOLD),
                )));

                for mount in &workspace.mounts {
                    let src_short = tui::shorten_home(&mount.src);
                    let dst_short = tui::shorten_home(&mount.dst);
                    let ro = if mount.readonly { " (read-only)" } else { "" };
                    let global_tag = if workspace_destinations.contains(mount.dst.as_str()) {
                        ""
                    } else {
                        " [global]"
                    };

                    let mut spans = vec![Span::styled("  ", Style::default())];
                    if src_short == dst_short {
                        spans.push(Span::styled(src_short, Style::default().fg(colors::PATH)));
                    } else {
                        spans.push(Span::styled(src_short, Style::default().fg(colors::PATH)));
                        spans.push(Span::styled(
                            " mounted as ",
                            Style::default().fg(colors::DIM_WHITE),
                        ));
                        spans.push(Span::styled(
                            dst_short,
                            Style::default().fg(colors::PATH_DST),
                        ));
                    }
                    if !ro.is_empty() || !global_tag.is_empty() {
                        spans.push(Span::styled(
                            format!("{ro}{global_tag}"),
                            Style::default().fg(colors::DIM_WHITE),
                        ));
                    }
                    detail_lines.push(Line::from(spans));
                }
            }
        }
        Err(error) => {
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                "launch preview unavailable",
                Style::default()
                    .fg(colors::BRIGHT_BLUE)
                    .add_modifier(Modifier::BOLD),
            )));
            detail_lines.push(Line::from(Span::styled(
                error.to_string(),
                Style::default().fg(colors::ERROR),
            )));
        }
    }

    detail_lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::LoadWorkspaceInput;

    #[test]
    fn agent_preview_includes_selector_scoped_global_mounts() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let shared_dir = temp.path().join("shared-cache");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::create_dir_all(&shared_dir).unwrap();

        let selector = crate::selector::ClassSelector::new(Some("chainargos"), "agent-smith");

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            selector.key(),
            crate::config::AgentSource {
                git: "https://github.com/chainargos/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.add_mount(
            "shared-cache",
            crate::workspace::MountConfig {
                src: shared_dir.canonicalize().unwrap().display().to_string(),
                dst: "/cache".to_string(),
                readonly: true,
            },
            Some("chainargos/*"),
        );

        let project_dir = project_dir.canonicalize().unwrap();
        let choice = WorkspaceChoice {
            name: "Current directory".to_string(),
            workspace: crate::workspace::ResolvedWorkspace {
                label: project_dir.display().to_string(),
                workdir: project_dir.display().to_string(),
                mounts: vec![crate::workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: project_dir.display().to_string(),
                    readonly: false,
                }],
            },
            allowed_agents: vec![selector.clone()],
            default_agent: None,
            last_agent: None,
            global_mounts: vec![],
            input: LoadWorkspaceInput::CurrentDir,
        };

        let details = build_agent_detail_lines(&config, &project_dir, &choice, Some(&selector));
        let rendered = details
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("resolved mounts"));
        assert!(rendered.contains("/cache"));
        assert!(rendered.contains("[global]"));
    }
}
