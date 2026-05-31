//! Host console surface ownership and runtime helpers.

pub mod focus;
pub mod github_mounts;
pub mod list_row;
pub mod model;
pub mod mount_diff;
pub mod mount_info;
pub mod mount_info_cache;
pub mod op_cache;
pub mod provider_picker;
pub mod run;
pub mod split;
pub mod terminal;
pub mod widgets;
pub mod workspace;
pub mod workspace_summary;

pub trait ConsoleHostTerminal: Send + Sync {
    fn begin_debug_buffering(&self);
    fn end_debug_buffering(&self);
    fn set_host_screen_owned(&self, owned: bool);
    fn host_screen_owned(&self) -> bool;
}
