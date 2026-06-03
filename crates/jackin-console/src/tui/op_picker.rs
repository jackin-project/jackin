//! 1Password vault/item/field picker modal — generic state and input handlers.
//!
//! The host crate (`jackin`) owns the load-execution layer that bridges these
//! types to the concrete `op` binary runner and the background-worker
//! subscription machinery. Test-runner injection lives in the binary's
//! `console/tui/op_picker/tests.rs` via a thread-local, keeping this crate
//! free of `jackin-env` dependencies.

pub mod input;
pub mod load;
pub mod state;

pub use state::{LoadResult, OpPickerState};

/// Concrete selection type for the picker: all five type parameters are bound
/// to `jackin-core` types already available in this crate.
pub type OpPickerSelection = crate::tui::components::op_picker::OpPickerSelection<
    jackin_core::OpRef,
    jackin_core::op_types::OpAccount,
    jackin_core::op_types::OpVault,
    jackin_core::op_types::OpItem,
    jackin_core::FieldTarget,
>;
