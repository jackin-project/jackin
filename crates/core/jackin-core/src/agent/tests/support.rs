// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn cli_binary(agent: Agent) -> &'static str {
    match agent {
        Agent::Antigravity => "agy",
        Agent::Cursor => "cursor-agent",
        _ => agent.slug(),
    }
}
