// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Standalone-dialog port.
//!
//! `jackin-isolation::finalize` needs to surface an exit dialog and an
//! error popup at host teardown. The dialog rendering lives in
//! `jackin_launch` (presentation, L3); calling it directly would
//! make jackin-runtime (L1) reach into L3. Define the consumer trait
//! here in the domain layer (L0) and have `jackin_launch` install
//! the impl at startup. Application-layer code calls
//! [`error_popup`] / [`exit_dialog_with_inspect`] via the global sink —
//! the same Branch-by-Abstraction pattern the `build_log` sink uses.

use std::sync::OnceLock;

use crate::launch_progress::{PromptContextLine, WorktreeInspect};

/// Receives a one-shot error popup rendered in the same rich surface as
/// launch-time failures.
///
/// Implementors live in presentation/UI crates; the application layer
/// never calls presentation code directly. Architecture invariant: this
/// trait is the ONLY surface through which an L1 module can show a
/// user-facing dialog; the underlying render impl is owned by
/// `jackin_launch`.
pub trait StandaloneDialogSink: Send + Sync + std::fmt::Debug {
    /// Show a one-shot error popup with `title` and `message`.
    fn error_popup(&self, title: &str, message: &str) -> anyhow::Result<()>;

    /// Show the exit dialog with inspect support; returns the chosen option index.
    fn exit_dialog_with_inspect(
        &self,
        title: &str,
        context: &[PromptContextLine],
        options: Vec<String>,
        worktrees_per_record: &[Vec<WorktreeInspect>],
    ) -> anyhow::Result<usize>;
}

static GLOBAL_SINK: OnceLock<&'static dyn StandaloneDialogSink> = OnceLock::new();

/// Install the process-wide sink. Idempotent — first install wins so a
/// startup race cannot silently swap an installed impl for a later one.
pub fn set_global_dialog_sink(sink: &'static dyn StandaloneDialogSink) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "second initialization is a benign race; first install wins"
    )]
    let _ = GLOBAL_SINK.set(sink);
}

fn sink() -> Option<&'static dyn StandaloneDialogSink> {
    GLOBAL_SINK.get().copied()
}

/// Show a standalone error popup. Returns `Ok(())` if the sink is
/// installed and the dialog rendered; `Err` only bubbles from the sink
/// impl (a fallible render outcome the UI knows how to surface). When no
/// sink is installed (e.g. during a test that never installed one) the
/// call is a no-op success so the calling code does not have to know
/// about the install race.
pub fn error_popup(title: &str, message: &str) -> anyhow::Result<()> {
    match sink() {
        Some(s) => s.error_popup(title, message),
        None => Ok(()),
    }
}

/// D23/D24: show the standalone exit dialog with `I`-key inspect
/// support. See [`StandaloneDialogSink::exit_dialog_with_inspect`] for
/// the `worktrees_per_record` argument shape.
pub fn exit_dialog_with_inspect(
    title: &str,
    context: &[PromptContextLine],
    options: Vec<String>,
    worktrees_per_record: &[Vec<WorktreeInspect>],
) -> anyhow::Result<usize> {
    match sink() {
        Some(s) => s.exit_dialog_with_inspect(title, context, options, worktrees_per_record),
        None => Ok(usize::MAX),
    }
}

#[cfg(test)]
mod tests;
