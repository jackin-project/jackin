//! Path alias for `jackin_protocol::control` so in-crate imports
//! resolve. The authoritative copy lives in the shared crate to keep
//! the host off jackin-capsule's tokio + PTY + terminal-model dependency tree;
//! anything added to the control channel goes there.

pub use jackin_protocol::control::*;
