//! jackin-runtime-naming: container names, labels, and display helpers.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`naming::matching_family`] — family name matching.
//!
//! Naming conventions, Docker label and filter constants, and
//! lightweight identifier helpers shared by launch, discovery, and
//! cleanup. Split out of `jackin-runtime` (S7 split 50); the old
//! `jackin_runtime::runtime::naming::*` paths keep working through
//! a re-export shim.

pub mod naming;
