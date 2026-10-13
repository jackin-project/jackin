// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch plan emit helpers extracted.

use jackin_diagnostics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchPlan {
    AttachExisting,
    StartStopped,
    CreateFromValidImage,
    BuildAndCreate,
    PrewarmOnly,
}

impl LaunchPlan {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AttachExisting => "AttachExisting",
            Self::StartStopped => "StartStopped",
            Self::CreateFromValidImage => "CreateFromValidImage",
            Self::BuildAndCreate => "BuildAndCreate",
            Self::PrewarmOnly => "PrewarmOnly",
        }
    }
}

pub(crate) fn emit_launch_plan(plan: LaunchPlan, reason: &str, container: Option<&str>) {
    if let Some(run) = jackin_diagnostics::active_run() {
        emit_launch_plan_for_run(&run, plan, reason, container);
    }
}

pub(crate) fn emit_launch_plan_for_run(
    run: &jackin_diagnostics::RunDiagnostics,
    plan: LaunchPlan,
    reason: &str,
    container: Option<&str>,
) {
    let plan = plan.as_str();
    let detail = serde_json::json!({
        "plan": plan,
        "reason": reason,
        "container": container,
    })
    .to_string();
    run.stage(
        "launch_plan",
        jackin_diagnostics::DiagnosticStage::Restore,
        &format!("selected launch plan {plan}"),
        Some(&detail),
    );
}

pub(crate) fn emit_prewarm_launch_plan(reason: &str) {
    emit_launch_plan(LaunchPlan::PrewarmOnly, reason, None);
}

pub(crate) fn emit_image_materialization_plan(
    image_reused: bool,
    reason: &str,
    restoring: bool,
    container: &str,
) {
    if image_reused {
        let base_reason = if restoring {
            "restore_container_missing_valid_image"
        } else {
            "no_restore_candidate_valid_image"
        };
        let plan_reason = if reason == "recipe_hash_match" {
            base_reason.to_owned()
        } else {
            format!("{base_reason}:{reason}")
        };
        emit_launch_plan(
            LaunchPlan::CreateFromValidImage,
            &plan_reason,
            Some(container),
        );
    } else {
        emit_launch_plan(LaunchPlan::BuildAndCreate, reason, Some(container));
    }
}

pub(crate) fn emit_rejected_launch_plan_for_run(
    run: &jackin_diagnostics::RunDiagnostics,
    plan: LaunchPlan,
    reason: &str,
    container: Option<&str>,
    state: Option<&str>,
) {
    let plan = plan.as_str();
    let detail = serde_json::json!({
        "plan": plan,
        "reason": reason,
        "container": container,
        "state": state,
    })
    .to_string();
    run.stage(
        "launch_plan_rejected",
        jackin_diagnostics::DiagnosticStage::Restore,
        &format!("rejected launch plan {plan}"),
        Some(&detail),
    );
}
