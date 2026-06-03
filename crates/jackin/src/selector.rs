//! Role and container selector types — re-exported from `jackin-core`.
//!
//! `RolePickerState` is a type alias defined here because it binds
//! `RoleSelector` (from core) to `RolePickerState<R>` (from jackin-console),
//! which requires both crates to be in scope.
//!
//! `impl RoleChoice for RoleSelector` lives in `jackin-console` (where the
//! `RoleChoice` trait is defined) to satisfy the orphan rule.

pub use jackin_core::{RoleSelector, Selector, SelectorError};

pub type RolePickerState =
    jackin_console::tui::components::role_picker::RolePickerState<RoleSelector>;

#[cfg(test)]
mod tests;
