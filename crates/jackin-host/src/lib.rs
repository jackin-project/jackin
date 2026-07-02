//! Host OS integration for jackin❯: desktop, clipboard, caffeinate/keep-awake.
//!
//! Architecture Invariant: this crate is a **L2 infrastructure** crate.
//! Allowed workspace dependencies are the core ports/types, diagnostics,
//! Docker client trait, protocol messages, and shared TUI helpers. Domain crates
//! (L0) must not depend on this; presentation crates (L3) reach host-clipboard
//! through the `ContainerHost` port trait in `jackin-core`.

pub mod caffeinate;
pub mod host_clipboard;
pub mod host_desktop;
pub(crate) mod naming;
pub(crate) mod universe;
