//! Host console surface ownership and runtime helpers.

pub mod mount_info;
pub mod op_cache;
pub mod terminal;
pub mod widgets;
pub mod workspace;

pub trait ConsoleHostTerminal: Send + Sync {
    fn begin_debug_buffering(&self);
    fn end_debug_buffering(&self);
    fn set_host_screen_owned(&self, owned: bool);
    fn host_screen_owned(&self) -> bool;
}
