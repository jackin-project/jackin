// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn instance_row_label(instance_id: &str, role_key: &str) -> InstanceRowLabel {
    InstanceRowLabel {
        instance_id: instance_id.to_owned(),
        role_key: role_key.to_owned(),
        status: InstanceStatus::Running,
    }
}

pub(super) fn display_row_facts(row: ManagerListRow) -> WorkspaceListDisplayRowFacts {
    WorkspaceListDisplayRowFacts {
        row,
        selected: false,
        hovered: false,
        current_dir_expanded: false,
        current_dir_has_instances: false,
    }
}
