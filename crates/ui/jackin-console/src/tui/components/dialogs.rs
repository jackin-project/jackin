// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod confirm;
mod hints;
mod popups;
mod save_discard;
mod text_input;
pub use confirm::{ConfirmKind, ConfirmState, render_confirm_dialog};
pub use hints::{
    confirm_hint_spans, error_popup_hint_spans, save_discard_hint_spans, text_input_hint_spans,
};
pub use popups::{ErrorPopupState, StatusPopupState, render_error_dialog, render_status_popup};
pub use save_discard::{SaveDiscardChoice, SaveDiscardState, render_save_discard_dialog};
pub use text_input::{TextInputState, render_text_input};
