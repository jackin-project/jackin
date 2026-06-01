//! Host console surface ownership and runtime helpers.

pub mod editor;
pub mod focus;
pub mod github_mounts;
pub mod layout;
pub mod list_geometry;
pub mod mount_diff;
pub mod mount_display;
pub mod mount_info;
pub mod mount_info_cache;
pub mod op_breadcrumb;
pub mod op_cache;
pub mod op_reference;
pub mod provider_picker;
pub mod run;
pub mod settings;
pub mod sidebar_layout;
pub mod split;
pub mod terminal;
pub mod widgets;
pub mod workspace;
pub mod workspaces;

pub trait ConsoleHostTerminal: Send + Sync {
    fn begin_debug_buffering(&self);
    fn end_debug_buffering(&self);
    fn set_host_screen_owned(&self, owned: bool);
    fn host_screen_owned(&self) -> bool;
}
