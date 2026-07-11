//! Grant-validation helpers used by `run_launch_core` (plan 033 suite A floor).
//! Full `run_launch_core` LaunchCore fixture is residual: constructing every
//! field needs a dedicated builder; grant-path cleanup order stays covered via
//! LoadCleanup / FakeDockerClient tests elsewhere.

use super::*;
use crate::runtime::docker_profile::DockerGrants;

#[test]
fn tag_errors_prefixes_each_with_source_tag() {
    let out = tag_errors("workspace", vec!["root+sudo", "bad pids"]);
    assert_eq!(
        out,
        [
            "  - [workspace] root+sudo".to_owned(),
            "  - [workspace] bad pids".to_owned(),
        ]
    );
}

#[test]
fn tag_errors_empty_input_yields_empty() {
    assert!(tag_errors::<&str>("config", Vec::new()).is_empty());
}

#[test]
fn bail_on_grant_errors_ok_when_empty() {
    bail_on_grant_errors(Vec::new()).unwrap();
}

#[test]
fn bail_on_grant_errors_bails_when_present() {
    let err = bail_on_grant_errors(vec!["  - [config] x".to_owned()]).unwrap_err();
    assert!(
        err.to_string().contains("docker grants validation failed"),
        "bail message must name the failure: {err}"
    );
    assert!(err.to_string().contains("[config] x"));
}

#[test]
fn tagged_grant_errors_tags_layer_and_catches_root_and_sudo() {
    let grants = DockerGrants {
        user: Some("root".to_owned()),
        sudo: Some(true),
        ..Default::default()
    };
    let errs = tagged_grant_errors("role", &grants);
    assert_eq!(errs.len(), 1, "root + sudo is one validation error");
    assert!(
        errs[0].starts_with("  - [role] "),
        "error must carry its source tag: {:?}",
        errs[0]
    );
}

#[test]
fn tagged_grant_errors_clean_grant_yields_nothing() {
    assert!(tagged_grant_errors("config", &DockerGrants::default()).is_empty());
}
