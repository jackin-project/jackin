//! Feature-gated host-daemon spike.
//!
//! This is intentionally not a production daemon. It captures the proposed
//! host-side control-socket shape and proves the smallest first adapter:
//! consuming the existing runtime status authority for attention notifications.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use jackin_core::ContainerId;
use jackin_protocol::InstanceSnapshot;
use jackin_protocol::control::AgentState;
use serde::{Deserialize, Serialize};

pub const DAEMON_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonRequest {
    pub id: String,
    pub protocol_version: u16,
    #[serde(flatten)]
    pub kind: DaemonRequestKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequestKind {
    Hello,
    AttentionSnapshot {
        container_name: String,
        panes: Vec<AttentionPaneStatus>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionPaneStatus {
    pub session_id: u64,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub state: AgentState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonResponse {
    pub id: String,
    #[serde(flatten)]
    pub kind: DaemonResponseKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponseKind {
    Hello {
        protocol_version: u16,
        capabilities: Vec<String>,
    },
    AttentionAccepted {
        notifications: usize,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionNotification {
    pub container_name: String,
    pub session_id: u64,
    pub agent: Option<String>,
    pub label: String,
    pub state: AgentState,
}

pub trait AttentionNotifier {
    fn notify(&mut self, notification: &AttentionNotification) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct DiagnosticNotifier;

impl AttentionNotifier for DiagnosticNotifier {
    fn notify(&mut self, _notification: &AttentionNotification) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionKey {
    container_name: ContainerId,
    session_id: u64,
}

#[derive(Debug)]
pub struct AttentionAdapter<N> {
    notifier: N,
    last_seen: HashMap<SessionKey, AgentState>,
}

impl<N: AttentionNotifier> AttentionAdapter<N> {
    pub fn new(notifier: N) -> Self {
        Self {
            notifier,
            last_seen: HashMap::new(),
        }
    }

    pub fn ingest_snapshot(
        &mut self,
        container_name: &str,
        snapshot: &InstanceSnapshot,
    ) -> Result<usize> {
        let panes = snapshot
            .tabs
            .iter()
            .flat_map(|tab| tab.panes.iter())
            .map(|pane| AttentionPaneStatus {
                session_id: pane.session_id,
                label: pane.label.clone(),
                agent: pane.agent.clone(),
                state: pane.state,
            })
            .collect::<Vec<_>>();
        self.ingest_panes(container_name, &panes)
    }

    pub fn ingest_panes(
        &mut self,
        container_name: &str,
        panes: &[AttentionPaneStatus],
    ) -> Result<usize> {
        let container_id = ContainerId::parse(container_name)
            .context("validating attention snapshot container name")?;
        let mut sent = 0;
        for pane in panes {
            let key = SessionKey {
                container_name: container_id.clone(),
                session_id: pane.session_id,
            };
            let previous = self.last_seen.insert(key, pane.state);
            if previous == Some(pane.state) || !is_attention_state(pane.state) {
                continue;
            }
            self.notifier.notify(&AttentionNotification {
                container_name: container_name.to_owned(),
                session_id: pane.session_id,
                agent: pane.agent.clone(),
                label: pane.label.clone(),
                state: pane.state,
            })?;
            sent += 1;
        }
        Ok(sent)
    }

    pub fn into_notifier(self) -> N {
        self.notifier
    }
}

const fn is_attention_state(state: AgentState) -> bool {
    matches!(state, AgentState::Blocked | AgentState::Done)
}

pub fn default_socket_path(jackin_home: &Path) -> PathBuf {
    jackin_home.join("run").join("jackin-daemon.sock")
}

pub fn bind_control_socket(path: &Path) -> Result<UnixListener> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating daemon socket dir {}", parent.display()))?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("restricting daemon socket dir {}", parent.display()))?;
    }
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("removing stale daemon socket {}", path.display()))?;
    }
    UnixListener::bind(path).with_context(|| format!("binding daemon socket {}", path.display()))
}

pub fn serve_one<N: AttentionNotifier>(
    socket_path: &Path,
    adapter: &mut AttentionAdapter<N>,
) -> Result<()> {
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let listener = match bind_control_socket(socket_path) {
        Ok(listener) => listener,
        Err(error) => {
            jackin_telemetry::stream::complete_error(
                open,
                jackin_telemetry::schema::enums::ErrorType::IoError,
            );
            return Err(error);
        }
    };
    jackin_telemetry::stream::complete_success(open);
    let close = jackin_telemetry::stream::close_on_drop();
    let result = listener
        .accept()
        .context("accepting daemon client")
        .and_then(|(stream, _)| handle_connection(stream, adapter));
    match &result {
        Ok(()) => close.complete_success(),
        Err(_) => close.complete_error(jackin_telemetry::schema::enums::ErrorType::IoError),
    }
    result
}

pub fn handle_connection<N: AttentionNotifier>(
    mut stream: UnixStream,
    adapter: &mut AttentionAdapter<N>,
) -> Result<()> {
    let mut line = String::new();
    BufReader::new(stream.try_clone().context("cloning daemon stream")?)
        .read_line(&mut line)
        .context("reading daemon request")?;
    if line.trim().is_empty() {
        bail!("empty daemon request");
    }
    let response = handle_request_line(line.trim_end(), adapter);
    serde_json::to_writer(&mut stream, &response).context("writing daemon response")?;
    stream
        .write_all(b"\n")
        .context("terminating daemon response")
}

pub fn handle_request_line<N: AttentionNotifier>(
    line: &str,
    adapter: &mut AttentionAdapter<N>,
) -> DaemonResponse {
    match serde_json::from_str::<DaemonRequest>(line) {
        Ok(request) => handle_request(request, adapter),
        Err(error) => DaemonResponse {
            id: "unknown".to_owned(),
            kind: DaemonResponseKind::Error {
                message: format!("invalid request: {error}"),
            },
        },
    }
}

fn handle_request<N: AttentionNotifier>(
    request: DaemonRequest,
    adapter: &mut AttentionAdapter<N>,
) -> DaemonResponse {
    if request.protocol_version != DAEMON_PROTOCOL_VERSION {
        return DaemonResponse {
            id: request.id,
            kind: DaemonResponseKind::Error {
                message: format!(
                    "unsupported daemon protocol {}; expected {}",
                    request.protocol_version, DAEMON_PROTOCOL_VERSION
                ),
            },
        };
    }

    let id = request.id;
    match request.kind {
        DaemonRequestKind::Hello => DaemonResponse {
            id,
            kind: DaemonResponseKind::Hello {
                protocol_version: DAEMON_PROTOCOL_VERSION,
                capabilities: vec!["attention.snapshot".to_owned()],
            },
        },
        DaemonRequestKind::AttentionSnapshot {
            container_name,
            panes,
        } => match adapter.ingest_panes(&container_name, &panes) {
            Ok(notifications) => DaemonResponse {
                id,
                kind: DaemonResponseKind::AttentionAccepted { notifications },
            },
            Err(error) => DaemonResponse {
                id,
                kind: DaemonResponseKind::Error {
                    message: error.to_string(),
                },
            },
        },
    }
}

#[cfg(test)]
mod tests;
