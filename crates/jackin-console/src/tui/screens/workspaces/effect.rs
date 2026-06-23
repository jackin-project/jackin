//! Workspaces-screen TUI effect vocabulary.
//!
//! Runtime instance refresh, launch, and config persistence are executed by
//! root-crate effect adapters. Root-independent workspace-list effects belong
//! here as they are introduced.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspacesEffect {}
