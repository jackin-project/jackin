//! Reusable Ratatui components shared by jackin' terminal surfaces.

pub mod brand_header;
pub mod error_dialog;
pub mod filter_input;
pub mod hint_bar;
pub mod status_footer;

pub use brand_header::{BrandHeader, brand_header_line, render_brand_header};
pub use error_dialog::{ErrorPopupState, estimated_message_rows, render_error_dialog, required_height};
pub use filter_input::{FilterInput, filter_input_line, render_filter_input};
pub use hint_bar::{
    HintBar, line as hint_line, render_hint_bar, render_wrapped_hint_bar, wrapped_height,
};
pub use status_footer::{StatusFooter, render_status_footer};
