//! Canonical host-console product surface.
//!
//! This crate owns reusable console state, update/input planning, view
//! composition, components, pure product decisions, and effects-as-data. The
//! binary crate (`jackin`) remains responsible for CLI dispatch, host terminal
//! ownership, Docker/runtime/config IO, and interpreting effects into real
//! side effects.

pub mod github_mounts;
pub mod mount_diff;
pub mod mount_info;
pub mod mount_info_cache;
pub mod services;
pub mod tui;
pub mod workspace;

pub trait ConsoleHostTerminal: Send + Sync {
    fn begin_debug_buffering(&self);
    fn end_debug_buffering(&self);
    fn set_host_screen_owned(&self, owned: bool);
    fn host_screen_owned(&self) -> bool;
}
