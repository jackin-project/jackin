// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod dispatch;
mod manager;
mod presence;
pub use dispatch::{
    ConsoleInputDispatchFacts, ConsoleInputDispatchPlan, ConsolePaintedInputOwner,
    ConsolePaintedInputOwnerFacts, ConsoleStageModalFacts, console_input_dispatch_plan,
    console_painted_input_owner_plan, console_usage_surface_painted,
};
pub use manager::{
    ConsoleManagerStage, ConsoleManagerStageRoute, ConsoleManagerStageState, apply_manager_stage,
};
pub use presence::{
    ConsoleAnimationTick, ConsoleCreatePreludeModalPresence, ConsoleEditorFooterHeight,
    ConsoleEditorModalPresence, ConsoleManagerInputPresence, ConsolePendingDriftCheck,
    ConsolePendingIsolationCleanup, ConsolePendingOpCommit, ConsolePendingOpCommitOrigin,
    ConsolePendingOpCommitResolution, ConsolePendingRoleLoad, ConsoleSettingsFooterHeight,
    ConsoleSettingsModalPresence,
};
