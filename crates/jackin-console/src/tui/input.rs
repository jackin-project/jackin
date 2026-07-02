//! Console manager input handlers — key dispatch, modal routing,
//! and two-phase save flow.

pub mod auth;
pub mod dispatch;
pub mod editor;
pub mod global_mounts;
pub mod list;
pub mod mouse;
pub mod prelude;
pub mod save;

pub use global_mounts::{
    apply_op_picker_to_settings_auth_form_committed, apply_plain_text_to_settings_auth_form,
    settings_auth_can_generate_token,
};

/// Return type for all input handlers: continue processing or consume the event.
pub type InputOutcome = crate::tui::message::ConsoleInputOutcome<
    jackin_core::RoleSelector,
    jackin_core::Agent,
    crate::tui::message::ConsoleInstanceAction<jackin_core::Agent>,
    jackin_protocol::Provider,
>;

#[cfg(test)]
pub mod test_support;
