//! jackin-console: operator console state machine and screens.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`ConsoleApp`] — operator console application shell.

#![expect(
    single_use_lifetimes,
    reason = "MSRV 1.95 rejects anonymous lifetimes in RPIT for these role iterators"
)]
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
