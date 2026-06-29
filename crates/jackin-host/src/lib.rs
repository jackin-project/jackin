//! Host OS integration for jackin❯: desktop, clipboard, caffeinate/keep-awake.
//!
//! Architecture Invariant: this crate is a **L2 infrastructure** crate.
//! Allowed dependencies: `jackin-core`. Domain crates (L0) must not depend
//! on this; presentation crates (L3) reach host-clipboard through the
//! `ContainerHost` port trait in `jackin-core`.

pub mod caffeinate;
pub mod host_clipboard;
pub mod host_desktop;
