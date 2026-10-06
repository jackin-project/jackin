// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn event<'a>(runtime: &'a str, event: &'a str) -> RuntimeEvent<'a> {
    RuntimeEvent { runtime, event }
}

pub(super) fn authority_state(effect: GateEffect) -> RawAgentState {
    match effect {
        GateEffect::Authority { state, .. } => state,
        other => panic!("expected authority effect, got {other:?}"),
    }
}

pub(super) fn canonical_turn(runtime: &str) -> &'static [&'static str] {
    match runtime {
        "opencode" => &[
            "session.status",
            "tool.execute.before",
            "permission.asked",
            "permission.replied",
            "tool.execute.after",
            "session.idle",
        ],
        "amp" => &[
            "agent.start",
            "tool.call",
            "permission-requested",
            "permission-resolved",
            "tool.result",
            "agent.end",
        ],
        other => panic!("missing recorded turn for {other}"),
    }
}
