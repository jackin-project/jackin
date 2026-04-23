use std::sync::atomic::{AtomicBool, Ordering};

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_debug_mode(enabled: bool) {
    DEBUG_MODE.store(enabled, Ordering::Relaxed);
}

// ── Shared color palette ─────────────────────────────────────────────────

const WHITE: (u8, u8, u8) = (255, 255, 255);

const PHOSPHOR_GREEN: (u8, u8, u8) = (0, 255, 65);
const PHOSPHOR_DIM: (u8, u8, u8) = (0, 140, 30);
const PHOSPHOR_DARK: (u8, u8, u8) = (0, 80, 18);

const fn rgb(color: (u8, u8, u8)) -> owo_colors::Rgb {
    owo_colors::Rgb(color.0, color.1, color.2)
}

pub mod animation;
pub mod output;
pub mod prompt;

pub use animation::{intro_animation, outro_animation, simple_outro};
pub use output::{
    clear_screen, deprecation_warning, fatal, hint, print_config_table, print_deploying,
    print_logo, set_terminal_title, shorten_home, step_fail, step_quiet, step_shimmer,
};
pub use prompt::{prompt_choice, require_interactive_stdin, spin_wait};
