//! Rendering facade for the shared 1Password picker modal.

use jackin_console_oppicker::TextInputState as OppickerTextInputState;
use ratatui::text::Line;

pub use crate::tui::op_picker::model::*;

mod lines;
mod render;
mod render_state;

pub use lines::{
    account_lines, fatal_body_lines, field_lines, item_choice_lines, loading_descriptor,
    loading_title_stage, section_lines, sentinel_line, vault_lines,
};
pub use render::{render_fatal, render_picker};

pub trait OpPickerRenderState {
    fn stage(&self) -> OpPickerStage;
    fn load_state(&self) -> &OpLoadState;
    fn filter_buffer(&self) -> &str;
    fn account_count(&self) -> usize;
    fn selected_account_email(&self) -> &str;
    fn selected_vault_name(&self) -> &str;
    fn selected_item_name(&self) -> &str;
    fn selected_item_subtitle(&self) -> &str;
    fn naming_stage_input(&self) -> Option<&OppickerTextInputState<'static>>;
    fn account_lines(&self) -> Vec<Line<'static>>;
    fn vault_lines(&self) -> Vec<Line<'static>>;
    fn item_lines(&self) -> Vec<Line<'static>>;
    fn section_lines(&self) -> Vec<Line<'static>>;
    fn field_lines(&self) -> Vec<Line<'static>>;
    fn selected_index(&self) -> Option<usize>;
}

#[cfg(test)]
mod tests;
