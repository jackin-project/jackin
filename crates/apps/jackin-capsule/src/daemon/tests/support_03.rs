// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn marker_row_on_screen(grid: &DamageGrid, rows: u16, cols: u16) -> Option<u16> {
    for row in 0..rows {
        for col in 0..cols {
            if grid
                .cell(row, col)
                .is_some_and(|c| c.contents() == "\u{25b8}")
            {
                return Some(row);
            }
        }
    }
    None
}

pub(super) fn subscribe_events(
    mux: &mut Multiplexer,
    session: Option<u64>,
) -> mpsc::UnboundedReceiver<ServerMsg> {
    let (tx, rx) = mpsc::unbounded_channel();
    handle_control_request(
        mux,
        ControlRequest {
            ctx: jackin_protocol::TelemetryContext::v1(),
            session_capability: None,
            msg: ClientMsg::Events { session },
            peer_uid: 0,
            reply: crate::attach_protocol::ControlReply::Stream(tx),
        },
    );
    rx
}

pub(super) fn next_event(
    rx: &mut mpsc::UnboundedReceiver<ServerMsg>,
) -> jackin_protocol::control::SessionEventRecord {
    match rx.try_recv().expect("an event was published") {
        ServerMsg::SessionEvent { event } => *event,
        other => panic!("expected a session event, got {}", other.kind()),
    }
}

pub(super) fn two_claude_mux() -> Multiplexer {
    let mut mux = test_mux(24, 80);
    mux.launch_env.launch_config.instances = vec!["claude-work".into(), "claude-personal".into()];
    mux.launch_env.launch_config.agents = BTreeMap::from([
        ("claude-work".into(), "claude".into()),
        ("claude-personal".into(), "claude".into()),
    ]);
    mux.launch_env.launch_config.accounts = BTreeMap::from([
        ("claude-work".into(), "work".into()),
        ("claude-personal".into(), "personal".into()),
    ]);
    mux.launch_env.launch_config.instance_home_dirs = BTreeMap::from([
        ("claude-work".into(), "/home/agent/.claude".into()),
        (
            "claude-personal".into(),
            "/home/agent/.claude-claude-personal".into(),
        ),
    ]);
    mux.launch_env.launch_config.instance_forwarded_dirs = BTreeMap::from([
        ("claude-work".into(), "/jackin/claude".into()),
        (
            "claude-personal".into(),
            "/jackin/claude-claude-personal".into(),
        ),
    ]);
    mux.launch_env.launch_config.auth_modes = BTreeMap::from([
        ("claude-work".into(), "sync".into()),
        ("claude-personal".into(), "sync".into()),
    ]);
    mux.launch_env.launch_config.instance_credential_files = BTreeMap::from([
        (
            "claude-work".into(),
            jackin_protocol::account_credentials_container_path("claude-work"),
        ),
        (
            "claude-personal".into(),
            jackin_protocol::account_credentials_container_path("claude-personal"),
        ),
    ]);
    mux.launch_env.launch_config.instance_mount_paths = BTreeMap::from([
        (
            "claude-work".into(),
            vec!["/home/agent/.claude".into(), "/jackin/claude".into()],
        ),
        (
            "claude-personal".into(),
            vec![
                "/home/agent/.claude-claude-personal".into(),
                "/jackin/claude-claude-personal".into(),
            ],
        ),
    ]);
    mux.launch_env.launch_config.instance_identities = BTreeMap::from([
        (
            "claude-work".into(),
            jackin_protocol::SessionIdentity {
                uid: 2_000,
                gid: 2_000,
            },
        ),
        (
            "claude-personal".into(),
            jackin_protocol::SessionIdentity {
                uid: 2_001,
                gid: 2_001,
            },
        ),
    ]);
    mux.launch_env.launch_config.shell_identity = Some(jackin_protocol::SessionIdentity {
        uid: 2_002,
        gid: 2_002,
    });
    mux.launch_env.launch_config.labels = BTreeMap::from([
        ("claude-work".into(), "Claude · Work".into()),
        ("claude-personal".into(), "Personal Claude".into()),
    ]);
    mux
}

pub(super) fn two_codex_mux() -> Multiplexer {
    let mut mux = test_mux(24, 80);
    mux.launch_env.launch_config.instances = vec!["codex-work".into(), "codex-personal".into()];
    mux.launch_env.launch_config.agents = BTreeMap::from([
        ("codex-work".into(), "codex".into()),
        ("codex-personal".into(), "codex".into()),
    ]);
    mux.launch_env.launch_config.models = BTreeMap::from([
        ("codex-work".into(), "k3".into()),
        ("codex-personal".into(), "glm-5.3".into()),
    ]);
    mux.launch_env.launch_config.efforts = BTreeMap::from([
        ("codex-work".into(), "max".into()),
        ("codex-personal".into(), "low".into()),
    ]);
    mux.launch_env.launch_config.instance_home_dirs = BTreeMap::from([
        ("codex-work".into(), "/home/agent/.codex".into()),
        (
            "codex-personal".into(),
            "/home/agent/.codex-codex-personal".into(),
        ),
    ]);
    mux.launch_env.launch_config.instance_forwarded_dirs = BTreeMap::from([
        ("codex-work".into(), "/jackin/codex".into()),
        (
            "codex-personal".into(),
            "/jackin/codex-codex-personal".into(),
        ),
    ]);
    mux.launch_env.launch_config.instance_identities = BTreeMap::from([
        (
            "codex-work".into(),
            jackin_protocol::SessionIdentity {
                uid: 2_000,
                gid: 2_000,
            },
        ),
        (
            "codex-personal".into(),
            jackin_protocol::SessionIdentity {
                uid: 2_001,
                gid: 2_001,
            },
        ),
    ]);
    // A stale process-wide value must not win over either slot's routing map.
    mux.launch_env.env_passthrough = vec![
        (
            jackin_core::CODEX_LANE_MODEL_ENV_NAME.into(),
            "wrong-global-model".into(),
        ),
        (
            jackin_core::CODEX_LANE_EFFORT_ENV_NAME.into(),
            "high".into(),
        ),
    ];
    mux
}
