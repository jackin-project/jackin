//! Exit "still here" summary.
//!
//! When an operator exits a foreground session and other jackin' agents
//! are still running, this shows a brief rich screen listing who remains —
//! grouped workspace → agent → instance ids, with duplicate ids collapsed —
//! using the shared brand chrome. It dwells for a couple of seconds so the
//! operator can register "those agents are still in the construct, I can
//! reconnect", then returns to the shell. Non-rich terminals (and
//! `--no-rain`) fall back to a single plain line.

use std::time::Duration;

use crossterm::ExecutableCommand as _;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::console::widgets::{
    PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, render_brand_header,
};
use crate::instance::InstanceIndex;
use crate::paths::JackinPaths;
use crate::runtime::LoadOptions;

/// How long the summary stays on screen before returning to the shell.
const DWELL: Duration = Duration::from_millis(2600);
const AGENT_COL: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitAgent {
    pub agent: String,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitGroup {
    pub workspace: String,
    pub agents: Vec<ExitAgent>,
}

/// Group the still-running instances by workspace, then by agent, collapsing
/// duplicate instance ids. Only entries whose container is in
/// `running_bases` are included. Ordering is stable (workspace then agent
/// sorted) so the screen does not jitter between runs.
#[must_use]
pub fn group_running(running_bases: &[String], index: &InstanceIndex) -> Vec<ExitGroup> {
    use std::collections::{BTreeMap, HashSet};
    let running: HashSet<&str> = running_bases.iter().map(String::as_str).collect();
    let mut by_ws: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for entry in &index.instances {
        if !running.contains(entry.container_base.as_str()) {
            continue;
        }
        let workspace = if entry.workspace_label.trim().is_empty() {
            crate::tui::shorten_home(&entry.workdir)
        } else {
            entry.workspace_label.clone()
        };
        let ids = by_ws
            .entry(workspace)
            .or_default()
            .entry(entry.agent_runtime.clone())
            .or_default();
        if !ids.contains(&entry.instance_id) {
            ids.push(entry.instance_id.clone());
        }
    }
    by_ws
        .into_iter()
        .map(|(workspace, agents)| ExitGroup {
            workspace,
            agents: agents
                .into_iter()
                .map(|(agent, ids)| ExitAgent { agent, ids })
                .collect(),
        })
        .collect()
}

fn total_instances(groups: &[ExitGroup]) -> usize {
    groups
        .iter()
        .flat_map(|group| group.agents.iter())
        .map(|agent| agent.ids.len())
        .sum()
}

fn render(frame: &mut Frame<'_>, area: Rect, exited: &str, groups: &[ExitGroup]) {
    frame.render_widget(Clear, area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(3)])
        .split(area);
    render_brand_header(frame, rows[0], "exile");

    let total = total_instances(groups);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            format!(" Exiled {exited} · {total} still in the construct "),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rows[1]).inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    frame.render_widget(block, rows[1]);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(Span::styled(
            group.workspace.clone(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        )));
        for agent in &group.agents {
            let mut spans = vec![Span::styled(
                format!("  {:<AGENT_COL$}", agent.agent),
                Style::default().fg(PHOSPHOR_DIM),
            )];
            for (j, id) in agent.ids.iter().enumerate() {
                if j > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(Span::styled(id.clone(), Style::default().fg(PHOSPHOR_GREEN)));
            }
            lines.push(Line::from(spans));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Show the rich exit summary, dwell briefly, then return. Falls back to a
/// single plain line on non-rich terminals, when nothing is grouped, or
/// when `--no-rain` opts out of exit rituals.
pub async fn show(paths: &JackinPaths, running_bases: &[String], exited: &str, opts: &LoadOptions) {
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap_or(InstanceIndex {
        version: 0,
        instances: Vec::new(),
    });
    let groups = group_running(running_bases, &index);
    let total = if groups.is_empty() {
        running_bases.len()
    } else {
        total_instances(&groups)
    };

    let rich = !opts.no_rain
        && !opts.no_tui
        && !groups.is_empty()
        && super::progress::rich_terminal_supported();
    if !rich {
        eprintln!("Exiled {exited}; {total} jackin' session(s) still running.");
        return;
    }

    if let Err(error) = show_rich(exited, &groups).await {
        if let Some(run) = crate::diagnostics::active_run() {
            run.compact("exit_summary", &format!("rich exit summary failed: {error:#}"));
        }
        eprintln!("Exiled {exited}; {total} jackin' session(s) still running.");
    }
}

async fn show_rich(exited: &str, groups: &[ExitGroup]) -> anyhow::Result<()> {
    let owns_screen = !crate::tui::host_screen_owned();
    let mut stdout = std::io::stdout();
    if owns_screen {
        stdout.execute(EnterAlternateScreen)?;
    }
    stdout.execute(crossterm::cursor::Hide)?;
    crate::tui::set_rich_surface_active(true);
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;
    // Consume the CompletedFrame immediately (map to ()) so it does not hold
    // a borrow of `terminal` across the teardown below.
    let drawn: anyhow::Result<()> = terminal
        .draw(|frame| render(frame, frame.area(), exited, groups))
        .map(|_| ())
        .map_err(anyhow::Error::from);
    if drawn.is_ok() {
        tokio::time::sleep(DWELL).await;
    }
    crate::tui::set_rich_surface_active(false);
    let backend = terminal.backend_mut();
    let _ = backend.execute(crossterm::cursor::Show);
    if owns_screen {
        let _ = backend.execute(LeaveAlternateScreen);
    }
    let _ = std::io::Write::flush(&mut std::io::stdout());
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{InstanceIndexEntry, InstanceStatus};

    fn entry(id: &str, base: &str, ws: &str, workdir: &str, agent: &str) -> InstanceIndexEntry {
        InstanceIndexEntry {
            instance_id: id.to_string(),
            container_base: base.to_string(),
            workspace_name: Some(ws.to_string()),
            workspace_label: ws.to_string(),
            workdir: workdir.to_string(),
            role_key: "the-architect".to_string(),
            agent_runtime: agent.to_string(),
            status: InstanceStatus::Running,
            updated_at: "2026-05-25T00:00:00Z".to_string(),
        }
    }

    fn index(entries: Vec<InstanceIndexEntry>) -> InstanceIndex {
        InstanceIndex {
            version: 1,
            instances: entries,
        }
    }

    #[test]
    fn groups_by_workspace_then_agent() {
        let idx = index(vec![
            entry("aaa", "jk-aaa-app-arch", "app", "/app", "claude"),
            entry("bbb", "jk-bbb-app-arch", "app", "/app", "codex"),
            entry("ccc", "jk-ccc-other-arch", "other", "/other", "amp"),
        ]);
        let running = vec![
            "jk-aaa-app-arch".to_string(),
            "jk-bbb-app-arch".to_string(),
            "jk-ccc-other-arch".to_string(),
        ];
        let groups = group_running(&running, &idx);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].workspace, "app");
        assert_eq!(groups[0].agents.len(), 2);
        assert_eq!(groups[1].workspace, "other");
        assert_eq!(total_instances(&groups), 3);
    }

    #[test]
    fn collapses_duplicate_ids_for_one_agent() {
        let idx = index(vec![
            entry("aaa", "jk-aaa-app-arch", "app", "/app", "claude"),
            entry("ddd", "jk-ddd-app-arch", "app", "/app", "claude"),
        ]);
        let running = vec!["jk-aaa-app-arch".to_string(), "jk-ddd-app-arch".to_string()];
        let groups = group_running(&running, &idx);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].agents.len(), 1, "one agent row");
        assert_eq!(groups[0].agents[0].agent, "claude");
        assert_eq!(groups[0].agents[0].ids, vec!["aaa", "ddd"]);
    }

    #[test]
    fn excludes_instances_not_in_the_running_set() {
        let idx = index(vec![
            entry("aaa", "jk-aaa-app-arch", "app", "/app", "claude"),
            entry("zzz", "jk-zzz-app-arch", "app", "/app", "codex"),
        ]);
        let running = vec!["jk-aaa-app-arch".to_string()];
        let groups = group_running(&running, &idx);
        assert_eq!(total_instances(&groups), 1);
        assert_eq!(groups[0].agents[0].agent, "claude");
    }

    #[test]
    fn renders_groups_into_the_frame() {
        use ratatui::{Terminal, backend::TestBackend};
        let groups = vec![ExitGroup {
            workspace: "big-monorepo".to_string(),
            agents: vec![ExitAgent {
                agent: "claude".to_string(),
                ids: vec!["k7p9m2xq".to_string(), "a1b2c3d4".to_string()],
            }],
        }];
        let backend = TestBackend::new(80, 16);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), "the-architect", &groups))
            .unwrap();
        let buf = term.backend().buffer();
        let dump: String = format!("{buf:?}");
        assert!(dump.contains("Exiled the-architect"), "title missing");
        assert!(dump.contains("big-monorepo"), "workspace missing");
        assert!(dump.contains("claude"), "agent missing");
        assert!(dump.contains("k7p9m2xq"), "id missing");
        assert!(dump.contains("a1b2c3d4"), "second id missing");
    }
}
