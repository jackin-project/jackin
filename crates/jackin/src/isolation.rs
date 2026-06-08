//! Isolation shim — all code lives in `jackin-runtime`.
//!
//! Re-exports the public API from `jackin-runtime::isolation` so that existing
//! `crate::isolation::*` call sites in the binary continue to compile unchanged.

pub mod branch {
    pub use jackin_runtime::isolation::branch::*;
}
pub mod cleanup {
    pub use jackin_runtime::isolation::cleanup::*;
}
pub mod finalize {
    pub use jackin_runtime::isolation::finalize::*;
}
pub mod materialize {
    pub use jackin_runtime::isolation::materialize::*;
}
pub mod state {
    pub use jackin_runtime::isolation::state::*;
}

pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;

#[cfg(test)]
mod tests;
