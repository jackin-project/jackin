// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn base_snapshot() -> EvidenceSnapshot {
    EvidenceSnapshot {
        authority: None,
        osc: OscEvidence::default(),
        screen: ScreenEvidence::default(),
        process: ProcessEvidence {
            child_alive: true,
            foreground_is_agent: true,
            ..ProcessEvidence::default()
        },
        activity: ActivityEvidence::default(),
        subagents_active: 0,
    }
}

pub(super) fn authority(
    state: RawAgentState,
    pending_permission: bool,
    last_event: Instant,
) -> AuthorityEvidence {
    AuthorityEvidence {
        source_id: "hook-claude-1".to_owned(),
        grade: AuthorityGrade::Partial,
        mapped_state: state,
        pending_permission,
        last_event,
        notes: Vec::new(),
    }
}
