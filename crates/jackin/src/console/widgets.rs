//! Compatibility re-exports for integration tests and benchmarks.
pub mod op_picker {
    pub use jackin_console::tui::components::op_picker::{OpLoadState, OpPickerStage};
}
