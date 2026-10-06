use super::local_version_override;

#[test]
fn local_build_defaults_to_package_version() {
    assert_eq!(
        local_version_override(None, false, "0.6.0-dev").as_deref(),
        Some("0.6.0-dev")
    );
}

#[test]
fn ci_build_keeps_git_stamp_path() {
    assert_eq!(local_version_override(None, true, "0.6.0-dev"), None);
}

#[test]
fn explicit_override_wins_in_ci_and_local_builds() {
    assert_eq!(
        local_version_override(Some("custom".to_owned()), false, "0.6.0-dev").as_deref(),
        Some("custom")
    );
    assert_eq!(
        local_version_override(Some("custom".to_owned()), true, "0.6.0-dev").as_deref(),
        Some("custom")
    );
}
