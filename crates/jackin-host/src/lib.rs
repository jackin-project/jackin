//! jackin-host: host desktop integration (clipboard, open, reveal).
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`HostClipboard`] — host clipboard integration.

pub mod caffeinate;
pub mod host_clipboard;
pub mod host_desktop;
pub(crate) mod naming;
mod process_telemetry;
pub(crate) mod universe;
