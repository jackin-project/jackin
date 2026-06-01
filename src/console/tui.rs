//! Transitional root-console render/widget facades.

pub mod auth_kind;
pub mod components;
mod create;
pub(crate) mod input;
pub mod message;
#[cfg(test)]
mod message_tests;
pub mod render;
pub mod state;

pub(crate) use crate::console::manager::{effects, file_browser};
