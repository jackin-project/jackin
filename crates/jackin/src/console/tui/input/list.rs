//! Thin adapter shell — all list-stage dispatch lives in jackin-console.

pub(super) use jackin_console::tui::input::list::{
    handle_inline_agent_picker, handle_inline_provider_picker, handle_inline_role_picker,
    handle_launch_provider_picker, handle_list_key, handle_list_modal, handle_new_session_picker,
};
