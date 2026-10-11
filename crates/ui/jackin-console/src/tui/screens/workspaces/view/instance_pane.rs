// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace instance pane types.

use super::{instance_detail_lines, instance_sessions_empty_message, panel};
use ratatui::{Frame, layout::Rect, style::Style, widgets::Paragraph};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstancePane {
    pub instance_id: String,
    pub focused: bool,
    pub content: WorkspaceInstancePaneContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceInstancePaneContent {
    Live {
        tabs: Vec<WorkspaceInstanceTab>,
    },
    Sessions {
        rows: Vec<WorkspaceInstanceSessionRow>,
    },
    Empty {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceTab {
    pub index: usize,
    pub label: String,
    pub active: bool,
    pub panes: Vec<WorkspaceInstanceTabPane>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceTabPane {
    pub label: String,
    pub account_id: Option<String>,
    pub config_id: Option<String>,
    pub state_label: String,
    pub focused: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceSessionRow {
    pub name: String,
    pub agent_runtime: String,
    pub account_id: Option<String>,
    pub config_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceLiveTabFacts {
    pub label: String,
    pub focused_pane: u64,
    pub panes: Vec<WorkspaceInstanceLivePaneFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceLivePaneFacts {
    pub session_id: u64,
    pub label: String,
    pub account_id: Option<String>,
    pub config_id: Option<String>,
    pub state_label: String,
}

#[must_use]
pub fn workspace_instance_pane(
    instance_id: String,
    focused: bool,
    content: WorkspaceInstancePaneContent,
) -> WorkspaceInstancePane {
    WorkspaceInstancePane {
        instance_id,
        focused,
        content,
    }
}

#[must_use]
pub fn workspace_instance_live_content(
    active_tab: usize,
    selected_pane: Option<u64>,
    tabs: impl IntoIterator<Item = WorkspaceInstanceLiveTabFacts>,
) -> WorkspaceInstancePaneContent {
    WorkspaceInstancePaneContent::Live {
        tabs: tabs
            .into_iter()
            .enumerate()
            .map(|(tab_idx, tab)| WorkspaceInstanceTab {
                index: tab_idx,
                label: tab.label,
                active: tab_idx == active_tab,
                panes: tab
                    .panes
                    .into_iter()
                    .map(|pane| WorkspaceInstanceTabPane {
                        label: pane.label,
                        account_id: pane.account_id,
                        config_id: pane.config_id,
                        state_label: pane.state_label,
                        focused: pane.session_id == tab.focused_pane,
                        selected: selected_pane == Some(pane.session_id),
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[must_use]
pub fn workspace_instance_session_content(
    session_load_error: bool,
    sessions: impl IntoIterator<Item = WorkspaceInstanceSessionRow>,
) -> WorkspaceInstancePaneContent {
    let rows: Vec<_> = sessions.into_iter().collect();
    if rows.is_empty() {
        WorkspaceInstancePaneContent::Empty {
            message: instance_sessions_empty_message(session_load_error).to_owned(),
        }
    } else {
        WorkspaceInstancePaneContent::Sessions { rows }
    }
}

pub fn render_instance_details_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    pane: &WorkspaceInstancePane,
) {
    let instance_title = format!(" Instance: {} ", pane.instance_id);
    let theme = termrock::style::DesignSystem::default();
    let block = panel(&theme, Some(&instance_title), pane.focused).block();
    let lines = instance_detail_lines(&pane.content);
    frame.render_widget(
        Paragraph::new(lines).block(block).style(
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default()),
        ),
        area,
    );
}
