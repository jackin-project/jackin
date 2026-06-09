//! Settings-screen TUI message vocabulary.
//!
//! Root-crate settings messages still live in `src/console/manager/message.rs`
//! while they carry root-only config and credential types. This module is the
//! screen-local home for root-independent settings messages as the migration
//! continues.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMessage {
    FocusTabBar,
    FocusContent,
}
