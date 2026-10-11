// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const LAUNCH_WIRE_CHILD: &str = "JACKIN_LAUNCH_WIRE_CHILD";

pub(super) const PRIVATE_ROLE_OWNER: &str = "wire-private-role-owner";

pub(super) const PRIVATE_ROLE: &str = "wire-private-launch-role";

pub(super) const PRIVATE_ROLE_ID: &str = "wire-private-role-owner/wire-private-launch-role";

pub(super) const PRIVATE_ROLE_URL: &str =
    "https://wire-private-role-source.invalid/roles.git?token=wire-private-role-token";

pub(super) struct TestDiagnostics;

impl LaunchDiagnostics for TestDiagnostics {
    fn run_id(&self) -> &'static str {
        "test-run"
    }
    fn compact(&self, _kind: &str, _message: &str) {}
    fn error(&self, _kind: &str, _message: &str, _error_type: Option<&str>) {}
    fn stage(
        &self,
        _kind: &str,
        _stage: jackin_core::LaunchStage,
        _message: &str,
        _detail: Option<&str>,
    ) {
    }
}

pub(super) fn steps_with_progress(cancelled: bool) -> StepCounter {
    let progress = LaunchProgress::for_test(Arc::new(TestDiagnostics));
    if cancelled {
        progress.cancel_token().cancel();
    }
    let mut steps = StepCounter::new(
        "test-role",
        jackin_telemetry::schema::enums::LaunchTargetKind::Directory,
    );
    steps.start_progress(progress);
    steps
}

pub(super) fn resolve_private_role_source() -> anyhow::Result<()> {
    let selector = RoleSelector::new(Some(PRIVATE_ROLE_OWNER), PRIVATE_ROLE);
    assert_eq!(selector.key(), PRIVATE_ROLE_ID);
    let mut config = AppConfig::default();
    config.roles.insert(
        selector.key(),
        RoleSource {
            git: PRIVATE_ROLE_URL.to_owned(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );
    // Inlined from `failure::resolve_launch_role_source` (S7 split 72):
    // with no restore override that helper is a straight passthrough to
    // `AppConfig::resolve_role_source` plus `restore_override = false`.
    let (source, is_new) = config.resolve_role_source(&selector)?;
    let restore_override = false;
    assert_eq!(source.git, PRIVATE_ROLE_URL);
    assert!(!is_new);
    assert!(!restore_override);
    Ok(())
}

pub(super) fn assert_private_launch_values_absent(testbed: &jackin_otlp_testbed::Testbed) {
    let prohibited = [
        PRIVATE_ROLE_OWNER,
        PRIVATE_ROLE,
        PRIVATE_ROLE_ID,
        PRIVATE_ROLE_URL,
        "wire-private-role-source.invalid",
        "wire-private-role-token",
        "wire-private-launch-title",
        "wire-private-launch-summary",
        "wire-private-launch-detail",
        "wire-private-launch-next-step",
        "wire-private-skip-reason",
        "wire-private-stage-done",
    ];
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
}
