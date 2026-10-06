// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure launch-resolution helpers for the host console.
mod accounts;
mod committed;
mod dispatch;
#[cfg(test)]
mod tests;
mod workspace;
pub use accounts::{
    AccountChoice, LiveInstanceAdmission, account_choices, account_choices_for_instances,
    account_choices_for_live_instances, accounts_for_launch, admitted_account_choices,
};
pub use committed::{
    CommittedAgentLaunch, CommittedRoleLaunch, resolve_account_launch_workspace,
    resolve_committed_agent_launch, resolve_committed_role_launch,
};
pub use dispatch::{LaunchDispatchResolution, resolve_launch_dispatch};
pub use workspace::{WorkspaceChoice, build_workspace_choice};

pub(crate) use committed::resolve_selected_workspace;
#[cfg(test)]
pub(crate) use jackin_config::AccountConfig;
#[cfg(test)]
pub(crate) use jackin_config::LoadWorkspaceInput;
#[cfg(test)]
pub(crate) use jackin_config::ResolvedInstance;
#[cfg(test)]
pub(crate) use jackin_config::resolve_launch;
#[cfg(test)]
pub(crate) use jackin_core::RoleSelector;
