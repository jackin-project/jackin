// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
struct TestDiagnostics;

impl LaunchDiagnostics for TestDiagnostics {
    fn run_id(&self) -> &'static str {
        "test-run"
    }
    fn compact(&self, _kind: &str, _message: &str) {}
    fn error(&self, _kind: &str, _message: &str, _error_type: Option<&str>) {}
    fn stage(&self, _kind: &str, _stage: LaunchStage, _message: &str, _detail: Option<&str>) {}
}

#[derive(Default)]
struct RecordingDiagnostics {
    stage: std::sync::Mutex<Option<(String, String, Option<String>)>>,
}

impl LaunchDiagnostics for RecordingDiagnostics {
    fn run_id(&self) -> &'static str {
        "test-run"
    }
    fn compact(&self, _kind: &str, _message: &str) {}
    fn error(&self, _kind: &str, _message: &str, _error_type: Option<&str>) {}
    fn stage(&self, kind: &str, _stage: LaunchStage, message: &str, detail: Option<&str>) {
        *self.stage.lock().unwrap() = Some((
            kind.to_owned(),
            message.to_owned(),
            detail.map(str::to_owned),
        ));
    }
}

fn test_progress() -> LaunchProgress {
    LaunchProgress::for_test(Arc::new(TestDiagnostics))
}

fn test_diagnostics() -> Arc<RunDiagnostics> {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    RunDiagnostics::start(
        &paths,
        false,
        "load",
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )
    .unwrap()
}

fn dummy_failure() -> LaunchFailure {
    LaunchFailure {
        title: "boom".to_owned(),
        summary: "it failed".to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::Network,
    }
}

