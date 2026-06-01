//! Transitional root-console render/widget facades.

pub mod auth_kind;
pub mod components;
mod create;
pub(crate) mod input;
pub mod render;

pub(crate) use crate::console::manager::{effects, file_browser, message, state};
