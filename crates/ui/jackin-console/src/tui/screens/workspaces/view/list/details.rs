// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Instance details pane rendering.

use crate::tui::screens::workspaces::view::{
    WorkspaceInstanceLivePaneFacts, WorkspaceInstanceLiveTabFacts, WorkspaceInstancePane,
    WorkspaceInstancePaneContent, WorkspaceInstanceSessionRow, workspace_instance_live_content,
    workspace_instance_pane, workspace_instance_session_content,
};

pub fn instance_details_pane(
    entry: &jackin_core::InstanceIndexEntry,
    sessions: &[jackin_core::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&jackin_protocol::InstanceSnapshot>,
    selected_pane: Option<u64>,
    preview_focused: bool,
) -> WorkspaceInstancePane {
    workspace_instance_pane(
        entry.instance_id.clone(),
        preview_focused,
        instance_details_content(sessions, session_load_error, snapshot, selected_pane),
    )
}

pub(crate) fn instance_details_content(
    sessions: &[jackin_core::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&jackin_protocol::InstanceSnapshot>,
    selected_pane: Option<u64>,
) -> WorkspaceInstancePaneContent {
    if let Some(snapshot) = snapshot {
        return workspace_instance_live_content(
            snapshot.active_tab as usize,
            selected_pane,
            snapshot
                .tabs
                .iter()
                .map(|tab| WorkspaceInstanceLiveTabFacts {
                    label: tab.label.clone(),
                    focused_pane: tab.focused_pane,
                    panes: tab
                        .panes
                        .iter()
                        .map(|pane| WorkspaceInstanceLivePaneFacts {
                            session_id: pane.session_id,
                            label: pane.label.clone(),
                            account_id: pane.account_id.clone(),
                            config_id: pane.agent.clone(),
                            state_label: pane.state.label().to_owned(),
                        })
                        .collect(),
                }),
        );
    }
    workspace_instance_session_content(
        session_load_error,
        sessions.iter().map(|session| WorkspaceInstanceSessionRow {
            name: session.tmux_name.clone(),
            agent_runtime: session.agent_runtime.clone(),
            account_id: session.account_id.clone(),
            config_id: session.instance.clone(),
        }),
    )
}
