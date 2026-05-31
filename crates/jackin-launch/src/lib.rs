//! Launch progress surface model and UI ownership.
//!
//! This crate is the extraction target for the launch cockpit. The first
//! boundary is the public launch model used by runtime orchestration; render,
//! update, and event-loop pieces move here in follow-up slices.

pub mod state;

pub use state::{
    LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind, PromptResult, StageStatus,
};
