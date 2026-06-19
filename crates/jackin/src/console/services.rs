//! Console side-effect adapters.

pub(super) mod agents;
pub(super) mod config;
pub(super) mod instances;
pub(super) mod op_picker {
    pub(crate) use jackin_console::tui::op_picker::start_load;
    #[cfg(test)]
    pub(in crate::console) use jackin_console::tui::op_picker::invalidate_cache_for_ref;
}
pub(super) mod role_load;
pub(super) mod workspace_save;
