// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ConsoleManagerStage` and the input-dispatch plan types.
mod dispatch;
mod manager;
mod presence;
pub use dispatch::{
    ConsoleInputDispatchFacts, ConsoleInputDispatchPlan, ConsoleStageModalFacts,
    console_input_dispatch_plan,
};
pub use manager::{
    ConsoleManagerStage, ConsoleManagerStageRoute, ConsoleManagerStageState, apply_manager_stage,
};
pub use presence::{
    ConsoleAnimationTick, ConsoleCreatePreludeModalPresence, ConsoleEditorFooterHeight,
    ConsoleEditorModalPresence, ConsoleManagerModalBlockPresence, ConsolePendingDriftCheck,
    ConsolePendingIsolationCleanup, ConsolePendingOpCommit, ConsolePendingOpCommitOrigin,
    ConsolePendingOpCommitResolution, ConsolePendingRoleLoad, ConsoleSettingsFooterHeight,
    ConsoleSettingsModalPresence,
};
