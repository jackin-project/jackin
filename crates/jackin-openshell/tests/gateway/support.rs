// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    AccountName, Agent, ImageId, ImageTag, LaunchId, PlannedLabels, ProviderName, RoleName,
    SandboxLabels, SandboxName, SandboxPath, WorkspaceName,
};
use jackin_openshell::{GatewayError, MainProcess, PolicyDocument, SandboxCreate};
use openshell_sdk::raw::proto::{ObjectMeta, Sandbox, SandboxPhase, SandboxStatus};

use super::sandbox_rpc::condition;
use super::server::State;

pub(crate) const LAUNCH: &str = "01hzy3k5m7p9r2t4v6w8x0abcd";
pub(crate) const IMAGE_DIGEST: &str =
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
pub(crate) const SANDBOX: &str = "jackin-ws-ab12";
pub(crate) const POLICY: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
    - /usr
    - /etc
  read_write:
    - /tmp
    - /sandbox
landlock:
  compatibility: best_effort
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  docker_engine:
    name: docker_engine
    endpoints:
      - host: host.openshell.internal
        port: 12376
        protocol: tcp
        tls: skip
    binaries:
      - path: /usr/local/bin/docker
";

pub(crate) fn sandbox_name(text: &str) -> SandboxName {
    SandboxName::parse(text).unwrap()
}

pub(crate) fn planned(workspace: &str, role: &str, account: &str, launch: &str) -> PlannedLabels {
    PlannedLabels {
        workspace: WorkspaceName::parse(workspace).unwrap(),
        role: RoleName::parse(role).unwrap(),
        agent: Agent::Codex,
        account: AccountName::parse(account).unwrap(),
        launch: LaunchId::parse(launch).unwrap(),
    }
}

pub(crate) fn labels_for(launch: &str) -> SandboxLabels {
    let image = ImageId::parse(&format!("sha256:{IMAGE_DIGEST}")).unwrap();
    planned("My.Project", "backend", "work", launch).complete(&image)
}

pub(crate) fn create_spec(name: &str, launch: &str) -> SandboxCreate {
    SandboxCreate::new(
        sandbox_name(name),
        ImageTag::parse("jackin/backend:0123456789abcdef").unwrap(),
        labels_for(launch),
        vec![ProviderName::parse("jackin-work-codex").unwrap()],
        PolicyDocument::parse_authored(POLICY).unwrap(),
        MainProcess {
            program: SandboxPath::parse("/usr/local/bin/jackin-start").unwrap(),
            args: vec!["codex".to_owned(), "--sandbox".to_owned()],
        },
    )
}

pub(crate) fn add_sandbox(
    state: &mut State,
    workspace: &str,
    name: &str,
    phase: SandboxPhase,
    exit_code: Option<i32>,
    labels: &[(&str, &str)],
) {
    let sandbox = Sandbox {
        metadata: Some(ObjectMeta {
            id: format!("id-{name}-{workspace}"),
            name: name.to_owned(),
            labels: labels
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            workspace: workspace.to_owned(),
            ..ObjectMeta::default()
        }),
        status: Some(SandboxStatus {
            phase: phase as i32,
            exit_code,
            ..SandboxStatus::default()
        }),
        ..Sandbox::default()
    };
    state
        .sandboxes
        .insert((workspace.to_owned(), name.to_owned()), sandbox);
}

pub(crate) fn set_conditions(
    state: &mut State,
    workspace: &str,
    name: &str,
    reasons: &[(&str, &str, &str, &str)],
) {
    let sandbox = state
        .sandboxes
        .get_mut(&(workspace.to_owned(), name.to_owned()))
        .unwrap();
    sandbox.status.as_mut().unwrap().conditions = reasons
        .iter()
        .map(|(kind, status, reason, message)| condition(kind, status, reason, message))
        .collect();
}

pub(crate) fn kind(error: &GatewayError) -> &'static str {
    match error {
        GatewayError::Unreachable { .. } => "Unreachable",
        GatewayError::Tls { .. } => "Tls",
        GatewayError::Unauthenticated => "Unauthenticated",
        GatewayError::PermissionDenied { .. } => "PermissionDenied",
        GatewayError::NotFound { .. } => "NotFound",
        GatewayError::AlreadyExists { .. } => "AlreadyExists",
        GatewayError::NotReady { .. } => "NotReady",
        GatewayError::MainProcessFailed { .. } => "MainProcessFailed",
        GatewayError::FailedPrecondition { .. } => "FailedPrecondition",
        GatewayError::ResourceExhausted { .. } => "ResourceExhausted",
        GatewayError::InvalidArgument { .. } => "InvalidArgument",
        GatewayError::Timeout { .. } => "Timeout",
        GatewayError::Cancelled { .. } => "Cancelled",
        GatewayError::Process(_) => "Process",
        GatewayError::Transfer { .. } => "Transfer",
        GatewayError::HostKeyMismatch => "HostKeyMismatch",
        GatewayError::TerminalRefused { .. } => "TerminalRefused",
        GatewayError::Protocol { .. } => "Protocol",
    }
}

pub(crate) const SHORT: Duration = Duration::from_secs(5);
