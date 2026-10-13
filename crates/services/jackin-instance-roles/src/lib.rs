//! jackin-instance-roles: role-state orchestration for role containers.
//!
//! [`RoleState`] preparation dispatch across agents and bindings. Depends on
//! `jackin-instance-agents` (provisioners) and `jackin-instance-credentials`
//! (substrate); `jackin-instance` re-exports this API for compatibility.

mod prepare;
mod provision;
mod role_state;

pub use role_state::{PrepareResolvers, RoleState};

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use jackin_instance_agents::{
    InstanceAuthBinding, ProvisionedAuth, capture_selected_account_sources,
    emit_agent_auth_provision, slot_home_and_target, slot_suffixes,
    validate_selected_account_sources,
};
#[cfg(test)]
pub(crate) use jackin_instance_credentials::AuthProvisionOutcome;
