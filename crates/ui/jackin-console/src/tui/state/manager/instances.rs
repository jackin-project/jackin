// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{ManagerEffect, ManagerState, active_instances_matching};

impl ManagerState<'_> {
    pub fn request_effect(&mut self, effect: ManagerEffect) {
        self.pending_effects.push(effect);
    }

    pub fn drain_effects(&mut self) -> Vec<ManagerEffect> {
        std::mem::take(&mut self.pending_effects)
    }

    #[must_use]
    pub fn workspace_active_instances(
        &self,
        ws_idx: usize,
    ) -> Vec<&jackin_core::InstanceIndexEntry> {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let query = jackin_core::InstanceQuery {
            workspace_name: Some(ws.name.as_str()),
            workspace_label: ws.name.as_str(),
            workdir: ws.workdir.as_str(),
            role_key: None,
            agent_runtime: None,
        };
        active_instances_matching(&self.instances, query).collect()
    }

    #[must_use]
    pub fn has_active_instances(&self, ws_idx: usize) -> bool {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return false;
        };
        let query = jackin_core::InstanceQuery {
            workspace_name: Some(ws.name.as_str()),
            workspace_label: ws.name.as_str(),
            workdir: ws.workdir.as_str(),
            role_key: None,
            agent_runtime: None,
        };
        active_instances_matching(&self.instances, query)
            .next()
            .is_some()
    }

    #[must_use]
    pub fn has_current_dir_active_instances(&self) -> bool {
        let current_dir = self.current_dir.as_str();
        let query = jackin_core::InstanceQuery {
            workspace_name: None,
            workspace_label: current_dir,
            workdir: current_dir,
            role_key: None,
            agent_runtime: None,
        };
        active_instances_matching(&self.instances, query)
            .next()
            .is_some()
    }

    #[must_use]
    pub fn current_dir_active_instances(&self) -> Vec<&jackin_core::InstanceIndexEntry> {
        let current_dir = self.current_dir.as_str();
        let query = jackin_core::InstanceQuery {
            workspace_name: None,
            workspace_label: current_dir,
            workdir: current_dir,
            role_key: None,
            agent_runtime: None,
        };
        active_instances_matching(&self.instances, query).collect()
    }

    #[must_use]
    pub fn workspace_visible_instances(
        &self,
        ws_idx: usize,
    ) -> Vec<&jackin_core::InstanceIndexEntry> {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let query = jackin_core::InstanceQuery {
            workspace_name: Some(ws.name.as_str()),
            workspace_label: ws.name.as_str(),
            workdir: ws.workdir.as_str(),
            role_key: None,
            agent_runtime: None,
        };
        crate::tui::state::visible_instances_matching(&self.instances, query).collect()
    }

    #[must_use]
    pub fn has_visible_instances(&self, ws_idx: usize) -> bool {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return false;
        };
        let query = jackin_core::InstanceQuery {
            workspace_name: Some(ws.name.as_str()),
            workspace_label: ws.name.as_str(),
            workdir: ws.workdir.as_str(),
            role_key: None,
            agent_runtime: None,
        };
        crate::tui::state::visible_instances_matching(&self.instances, query)
            .next()
            .is_some()
    }

    #[must_use]
    pub fn has_current_dir_visible_instances(&self) -> bool {
        let current_dir = self.current_dir.as_str();
        let query = jackin_core::InstanceQuery {
            workspace_name: None,
            workspace_label: current_dir,
            workdir: current_dir,
            role_key: None,
            agent_runtime: None,
        };
        crate::tui::state::visible_instances_matching(&self.instances, query)
            .next()
            .is_some()
    }

    #[must_use]
    pub fn current_dir_visible_instances(&self) -> Vec<&jackin_core::InstanceIndexEntry> {
        let current_dir = self.current_dir.as_str();
        let query = jackin_core::InstanceQuery {
            workspace_name: None,
            workspace_label: current_dir,
            workdir: current_dir,
            role_key: None,
            agent_runtime: None,
        };
        crate::tui::state::visible_instances_matching(&self.instances, query).collect()
    }
}
