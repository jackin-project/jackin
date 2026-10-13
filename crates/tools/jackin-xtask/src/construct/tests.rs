use super::*;

#[test]
fn inspect_success_means_already_published() {
    assert_eq!(classify_inspect(true, ""), VersionStatus::AlreadyPublished);
}

#[test]
fn inspect_known_absent_markers_mean_unpublished() {
    for stderr in [
        "ERROR: manifest unknown: manifest unknown not found",
        "MANIFEST_UNKNOWN: manifest unknown",
        "requested tag does not exist",
        "NAME_UNKNOWN: repository name not known",
    ] {
        assert_eq!(
            classify_inspect(false, stderr),
            VersionStatus::Unpublished,
            "{stderr}"
        );
    }
}

#[test]
fn inspect_other_failure_is_unknown_error() {
    assert_eq!(
        classify_inspect(false, "unauthorized: authentication required"),
        VersionStatus::UnknownError
    );
}

#[test]
fn published_version_skips_manifest_publish() {
    assert_eq!(manifest_action(true), ManifestAction::SkipAlreadyPublished);
}

#[test]
fn unpublished_version_publishes_manifest() {
    assert_eq!(manifest_action(false), ManifestAction::Publish);
}

#[test]
fn find_digest_reads_nested_target() {
    let value = serde_json::json!({
        "construct-publish": { "containerimage.digest": "sha256:abc123" }
    });
    assert_eq!(find_digest(&value).as_deref(), Some("sha256:abc123"));
}

#[test]
fn find_digest_ignores_empty_and_missing() {
    assert_eq!(
        find_digest(&serde_json::json!({"t": {"containerimage.digest": ""}})),
        None
    );
    assert_eq!(find_digest(&serde_json::json!({"t": {"other": "x"}})), None);
}
