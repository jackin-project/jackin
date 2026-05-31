pub use jackin_console::widgets::confirm_save::{
    ConfirmSaveFocus, ConfirmSaveState as GenericConfirmSaveState, SaveChoice, prepare_for_render,
    render, required_height,
};

pub type ConfirmSaveState = GenericConfirmSaveState<crate::workspace::MountConfig>;
