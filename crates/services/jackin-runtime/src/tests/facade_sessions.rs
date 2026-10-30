// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{AgentSession, AgentSessionInventory, describe_agent_session_count};

#[test]
fn not_running_inventory_has_fixed_label() {
    assert_eq!(
        describe_agent_session_count(&AgentSessionInventory::NotRunning),
        "sessions:not_running"
    );
}

#[test]
fn unavailable_inventory_has_fixed_label() {
    assert_eq!(
        describe_agent_session_count(&AgentSessionInventory::Unavailable("gone".to_owned())),
        "sessions:unavailable"
    );
}

#[test]
fn session_list_reports_entry_count() {
    let inventory = AgentSessionInventory::Sessions(vec![
        AgentSession {
            name: "alpha".to_owned(),
        },
        AgentSession {
            name: "beta".to_owned(),
        },
    ]);
    assert_eq!(describe_agent_session_count(&inventory), "sessions:2");
}

#[test]
fn empty_session_list_reports_zero() {
    let inventory = AgentSessionInventory::Sessions(Vec::new());
    assert_eq!(describe_agent_session_count(&inventory), "sessions:0");
}
