//! Host console TUI effect vocabulary.
//!
//! Effects describe non-TUI work requested by console update code. The root
//! application layer executes them because it owns config, runtime paths, and
//! service adapters.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleEffect {
    RequestActiveMountInfoRefresh,
    RequestInstanceRefresh,
    SaveSettings,
}
