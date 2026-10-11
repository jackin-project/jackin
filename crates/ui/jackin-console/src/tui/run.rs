// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared helpers for the host console event-loop shell.
mod console;
mod debug;
mod letter;
mod mouse;
mod quit;
mod stage;
#[cfg(test)]
mod tests;
pub use console::{
    is_on_main_screen, letter_input_state_for_console, no_modal_open, quit_confirm_area,
    quit_intercept_state_for_console, screen_of, startup_error_dismissed,
    startup_error_modal_active_for_console,
};
pub use debug::{
    debug_chip_row, debug_invocation_id_label, should_debug_log_mouse, split_debug_area,
};
pub use letter::{
    LetterInputModalKind, LetterInputState, QuitInterceptState, consumes_letter_input,
    letter_input_modal_kind, letter_input_state_for_route,
};
pub use mouse::{
    ConsoleClickStageFacts, ConsoleClickabilityFacts, ConsoleModalMouseFacts,
    ConsoleModalMouseLayerFacts, ConsoleModalMouseLayerPlan, console_clickable_at,
    console_pointer_shape, debug_chip_activation_allowed, modal_mouse_layer_consumes,
    modal_mouse_layer_plan, should_dismiss_list_modal_for_outside_click,
};
pub use quit::{
    ModalBlockState, QuitConfirmPlan, no_modal_blocks_base_surface, quit_confirm_plan,
    quit_confirm_state, should_open_keyboard_help, should_open_quit_confirm,
    startup_error_modal_active, startup_error_was_dismissed,
};
pub use stage::{
    ConsoleChromeHover, ConsoleScreenStage, MainScreenState, console_screen_stage_for_route,
    diagnostics_screen_for_stage, is_main_screen, is_main_screen_for_route,
};
