// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Closed set of launch-stage dimensions used by progress telemetry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagnosticStage {
    Preflight,
    Image,
    Run,
    Attach,
    Cleanup,
    Prepare,
    DerivedImage,
    StartContainer,
    Hardline,
    Credentials,
    Building,
    Build,
    Plan,
    Restore,
    Sidecar,
    Op,
    Launch,
    Identity,
    Role,
    Construct,
    AgentBinaries,
    Workspace,
    Network,
    Capsule,
    Repo,
}

impl DiagnosticStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::Image => "image",
            Self::Run => "run",
            Self::Attach => "attach",
            Self::Cleanup => "cleanup",
            Self::Prepare => "prepare",
            Self::DerivedImage => "derived image",
            Self::StartContainer => "start container",
            Self::Hardline => "hardline",
            Self::Credentials => "credentials",
            Self::Building => "building",
            Self::Build => "build",
            Self::Plan => "plan",
            Self::Restore => "restore",
            Self::Sidecar => "sidecar",
            Self::Op => "op",
            Self::Launch => "launch",
            Self::Identity => "identity",
            Self::Role => "role",
            Self::Construct => "construct",
            Self::AgentBinaries => "agent binaries",
            Self::Workspace => "workspace",
            Self::Network => "network",
            Self::Capsule => "capsule",
            Self::Repo => "repo",
        }
    }

    #[must_use]
    pub const fn span_name(self) -> &'static str {
        match self {
            Self::Preflight => "launch.preflight",
            Self::Image => "launch.image",
            Self::Run => "launch.run",
            Self::Attach => "launch.attach",
            Self::Cleanup => "launch.cleanup",
            Self::Prepare => "launch.prepare",
            Self::DerivedImage => "launch.derived_image",
            Self::StartContainer => "launch.start_container",
            Self::Hardline => "launch.hardline",
            Self::Credentials => "launch.credentials",
            Self::Building => "launch.building",
            Self::Build => "launch.build",
            Self::Plan => "launch.plan",
            Self::Restore => "launch.restore",
            Self::Sidecar => "launch.sidecar",
            Self::Op => "launch.op",
            Self::Launch => "launch.launch",
            Self::Identity => "launch.identity",
            Self::Role => "launch.role",
            Self::Construct => "launch.construct",
            Self::AgentBinaries => "launch.agent_binaries",
            Self::Workspace => "launch.workspace",
            Self::Network => "launch.network",
            Self::Capsule => "launch.capsule",
            Self::Repo => "launch.repo",
        }
    }
}

impl From<jackin_core::LaunchStage> for DiagnosticStage {
    fn from(stage: jackin_core::LaunchStage) -> Self {
        match stage {
            jackin_core::LaunchStage::Identity => Self::Identity,
            jackin_core::LaunchStage::Role => Self::Role,
            jackin_core::LaunchStage::Credentials => Self::Credentials,
            jackin_core::LaunchStage::Construct => Self::Construct,
            jackin_core::LaunchStage::AgentBinaries => Self::AgentBinaries,
            jackin_core::LaunchStage::DerivedImage => Self::DerivedImage,
            jackin_core::LaunchStage::Workspace => Self::Workspace,
            jackin_core::LaunchStage::Network => Self::Network,
            jackin_core::LaunchStage::Sidecar => Self::Sidecar,
            jackin_core::LaunchStage::Capsule => Self::Capsule,
            jackin_core::LaunchStage::Hardline => Self::Hardline,
        }
    }
}
