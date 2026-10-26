// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::OnceLock;

use crate::launch_progress::{PromptContextLine, WorktreeInspect};

pub trait StandaloneDialogSink: Send + Sync + std::fmt::Debug {
    fn error_popup(&self, title: &str, message: &str) -> anyhow::Result<()>;

    fn exit_dialog_with_inspect(
        &self,
        title: &str,
        context: &[PromptContextLine],
        options: Vec<String>,
        worktrees_per_record: &[Vec<WorktreeInspect>],
    ) -> anyhow::Result<usize>;
}

static GLOBAL_SINK: OnceLock<&'static dyn StandaloneDialogSink> = OnceLock::new();

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

pub fn error_popup(title: &str, message: &str) -> anyhow::Result<()> {
    match sink() {
        Some(s) => s.error_popup(title, message),
        None => Ok(()),
    }
}

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
