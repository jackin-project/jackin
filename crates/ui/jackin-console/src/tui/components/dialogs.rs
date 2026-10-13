// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-owned console dialog state composed from canonical `TermRock` widgets.
//!
//! Plan 010 step 1 reviewed the named upstream pairings at rev `29a16b5b` and
//! recorded behavior-preserving non-adoptions (commit-message carve-outs):
//! `alert_dialog` paints a fixed risk-anatomy body (`build_body_text`) with no
//! host free-text override; `error_state` is a centered inline/block anatomy
//! without modal chrome; `question_flow` paints an option list, not the
//! action-row choice body; `loading_overlay`'s `BusyBoundaryState` owns
//! dismissal, which the product status overlay keeps host-side.
mod confirm;
mod hints;
mod popups;
mod save_discard;
#[cfg(test)]
mod tests;
mod text_input;
pub use confirm::{ConfirmKind, ConfirmState, render_confirm_dialog};
pub use hints::{
    confirm_hint_spans, error_popup_hint_spans, save_discard_hint_spans, text_input_hint_spans,
};
pub use popups::{ErrorPopupState, StatusPopupState, render_error_dialog, render_status_popup};
pub use save_discard::{SaveDiscardChoice, SaveDiscardState, render_save_discard_dialog};
pub use text_input::{TextInputState, render_text_input};

#[cfg(test)]
pub(crate) use termrock::input::KeyEvent;
