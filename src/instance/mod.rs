//! Instance shim — all code lives in `jackin-runtime`.
//!
//! Re-exports the public API from `jackin-runtime::instance` so that existing
//! `crate::instance::*` call sites in the binary continue to compile unchanged.

pub use jackin_runtime::instance::*;

pub mod manifest {
    pub use jackin_runtime::instance::manifest::*;
}
pub mod naming {
    pub use jackin_runtime::instance::naming::*;
}

#[cfg(test)]
mod tests;
