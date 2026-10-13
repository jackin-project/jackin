use super::{CheckStatus, docker_version_command_failure_result, docker_version_report_result};

#[test]
fn docker_version_reporter_accepts_any_non_empty_version() {
    let result = docker_version_report_result("19.03.0\n");

    assert_eq!(result.name, "docker_version");
    assert_eq!(result.status, CheckStatus::Ok);
    assert_eq!(result.message, "Docker server 19.03.0");
    assert!(result.hint.is_none());
}

#[test]
fn docker_version_failure_hint_does_not_imply_a_floor() {
    let result = docker_version_command_failure_result(Some(1), "Cannot connect to daemon\n");

    assert_eq!(result.name, "docker_version");
    assert_eq!(result.status, CheckStatus::Warn);
    assert_eq!(
        result.message,
        "Could not read Docker server version: Cannot connect to daemon"
    );

    let hint = result.hint.as_deref().unwrap_or("");
    assert!(hint.contains("Docker CLI can reach the daemon"));
    assert!(!hint.contains("Upgrade Docker"));
    assert!(!hint.contains("latest stable"));
}
