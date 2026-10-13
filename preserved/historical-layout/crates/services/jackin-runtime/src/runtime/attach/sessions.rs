// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `AgentSession` inventory inspection and parsing.

use jackin_core::ContainerHandle;
use jackin_docker::docker_client::DockerApi;

use super::ContainerState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionInventory {
    NotRunning,
    Unavailable(String),
    Sessions(Vec<AgentSession>),
}

pub async fn inspect_agent_sessions(
    docker: &impl DockerApi,
    container: &ContainerHandle,
    state: &ContainerState,
) -> AgentSessionInventory {
    if matches!(state, ContainerState::InspectUnavailable(_)) {
        return AgentSessionInventory::Unavailable(
            "container state unavailable; skipping session query".to_owned(),
        );
    }
    if !matches!(state, ContainerState::Running) {
        return AgentSessionInventory::NotRunning;
    }

    let status_command = jackin_core::jackin_status_command(
        jackin_protocol::capsule_transport::CONTROL_PROTOCOL_MAJOR,
    );
    match docker
        .exec_capture_by_id(container, &["sh", "-c", &status_command])
        .await
    {
        Ok(output) => match parse_jackin_sessions(&output) {
            Ok(sessions) => AgentSessionInventory::Sessions(sessions),
            Err(reason) => AgentSessionInventory::Unavailable(reason),
        },
        Err(error) => AgentSessionInventory::Unavailable(error.to_string()),
    }
}

/// Parse session list from `jackin-capsule status` output.
///
/// The output starts with `Sessions: <N>` followed by N lines shaped
/// `  [<id>] <label> (<agent>) state=<state> active=<bool>`. The
/// header is required: without it, the function returns `Err` so
/// callers can route to `Unavailable` instead of silently treating
/// "no `[` lines" as "zero sessions". A cosmetic change to the
/// capsule's status print therefore surfaces immediately as an
/// operator-visible "sessions unavailable" rather than a wrong
/// auto-cleanup.
///
/// `take(expected)` consumes only the first N `[`-prefixed lines
/// after the header so a future trailing footer (totals row, debug
/// summary) or a label whose `Display` impl emits a non-`[` second
/// line does not flip the parse to `Unavailable`. Pre-header
/// `[`-prefixed lines are dropped by the `skip_while` synchronisation
/// on the header — that matches the capsule's print order, where the
/// header is always the first non-blank line.
pub(crate) fn parse_jackin_sessions(output: &str) -> Result<Vec<AgentSession>, String> {
    let expected = jackin_core::parse_session_count(output).ok_or_else(|| {
        "jackin-capsule status emitted no parsable `Sessions: N` header — daemon may be unreachable".to_owned()
    })?;

    let sessions: Vec<AgentSession> = output
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Sessions:"))
        .skip(1)
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || !trimmed.starts_with('[') {
                return None;
            }
            // Strip from ` state=` onward, then strip the last
            // ` (<agent>)` block — what remains is the label. `rfind`
            // tolerates labels that themselves contain `(`.
            let after_id = trimmed.split(']').nth(1)?.trim_start();
            let head = after_id
                .rfind(" state=")
                .map_or(after_id, |idx| &after_id[..idx]);
            let name = head.rfind(" (").map_or(head, |idx| &head[..idx]);
            Some(AgentSession {
                name: name.to_owned(),
            })
        })
        .take(expected)
        .collect();

    if sessions.len() < expected {
        return Err(format!(
            "jackin-capsule status header claims {expected} sessions but only {} `[`-prefixed lines parsed",
            sessions.len()
        ));
    }
    Ok(sessions)
}

/// Builder for `docker inspect`-failure operator messages. `clause`
/// is the verb + target phrase (e.g. ``"inspect container `foo`"``,
/// ``"claim container name `foo`"``); the tail is the shared
/// reason-suffix every call site needs.
pub fn docker_unavailable_msg(clause: &str, reason: &str) -> String {
    format!(
        "cannot {clause} because Docker is unavailable or returned an unexpected response: {reason}"
    )
}

pub(crate) fn inspect_unavailable_message(container_name: &str, reason: &str) -> String {
    docker_unavailable_msg(&format!("inspect container `{container_name}`"), reason)
}
