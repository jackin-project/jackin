//! jackin-runtime-exit-summary: exit "still running" summary data.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`exit_summary::summary`] — headline + rows builder.
//!
//! Builds the compact, privacy-preserving summary shown when the
//! operator leaves one foreground session while other instances keep
//! running. Split out of `jackin-runtime` (S7 split 54); the old
//! `jackin_runtime::runtime::exit_summary::*` paths keep working
//! through a re-export shim.

pub mod exit_summary;
