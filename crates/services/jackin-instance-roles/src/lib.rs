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
