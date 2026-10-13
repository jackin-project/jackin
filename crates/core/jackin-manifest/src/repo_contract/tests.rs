// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `repo_contract`.
use super::*;
use tempfile::tempdir;

#[test]
fn accepts_versioned_construct_with_alias() {
    let temp = tempdir().unwrap();
    let dockerfile = temp.path().join("Dockerfile");
    std::fs::write(
        &dockerfile,
        "FROM rust:1.95.0 AS builder\nRUN cargo build\n\n\
             FROM projectjackin/construct:0.1-trixie AS runtime\n\
             COPY --from=builder /app /workspace/app\n",
    )
    .unwrap();

    let validated = validate_agent_dockerfile(&dockerfile).unwrap();

    assert_eq!(
        validated.final_stage_image,
        "projectjackin/construct:0.1-trixie"
    );
    assert_eq!(validated.final_stage_alias.as_deref(), Some("runtime"));
    assert_eq!(validated.construct_version, "0.1-trixie");
}

#[test]
fn accepts_versioned_construct_without_alias() {
    let temp = tempdir().unwrap();
    let dockerfile = temp.path().join("Dockerfile");
    std::fs::write(&dockerfile, "FROM projectjackin/construct:0.2-trixie\n").unwrap();

    let validated = validate_agent_dockerfile(&dockerfile).unwrap();

    assert_eq!(validated.construct_version, "0.2-trixie");
}

#[test]
fn accepts_digest_pinned_versioned_construct() {
    let temp = tempdir().unwrap();
    let dockerfile = temp.path().join("Dockerfile");
    std::fs::write(
            &dockerfile,
            "FROM projectjackin/construct:0.1-trixie@sha256:0b076bfbc53d36794fe54b1a9cab670f85f831af86d78426b1a88a8ac192d445\n",
        )
        .unwrap();

    let validated = validate_agent_dockerfile(&dockerfile).unwrap();

    // construct_version carries only the version tag, not the digest
    assert_eq!(validated.construct_version, "0.1-trixie");
}

#[test]
fn rejects_floating_stable_tag() {
    let temp = tempdir().unwrap();
    let dockerfile = temp.path().join("Dockerfile");
    std::fs::write(&dockerfile, format!("FROM {CONSTRUCT_IMAGE}\n")).unwrap();

    let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

    assert!(matches!(
        error,
        RoleRepoValidationError::DockerfileMissingVersionPin
    ));
    let msg = error.to_string();
    assert!(msg.contains("floating tag"));
    assert!(
        msg.contains("Renovate"),
        "error must include Renovate guidance; got: {msg}"
    );
}

#[test]
fn rejects_empty_version_prefix() {
    let temp = tempdir().unwrap();
    let dockerfile = temp.path().join("Dockerfile");
    std::fs::write(
        &dockerfile,
        format!("FROM {CONSTRUCT_REGISTRY_IMAGE}:-{CONSTRUCT_STABLE_TAG}\n"),
    )
    .unwrap();

    let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

    assert!(matches!(
        error,
        RoleRepoValidationError::DockerfileMissingVersionPin
    ));
}

#[test]
fn rejects_final_stage_on_other_image() {
    let temp = tempdir().unwrap();
    let dockerfile = temp.path().join("Dockerfile");
    std::fs::write(&dockerfile, "FROM debian:trixie\n").unwrap();

    let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

    assert!(error.to_string().contains("projectjackin/construct:trixie"));
}

#[test]
fn rejects_arg_indirection_in_final_from() {
    let temp = tempdir().unwrap();
    let dockerfile = temp.path().join("Dockerfile");
    std::fs::write(
        &dockerfile,
        r"ARG BASE=projectjackin/construct:trixie
FROM ${BASE}
",
    )
    .unwrap();

    let error = validate_agent_dockerfile(&dockerfile).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("literal FROM projectjackin/construct:trixie")
    );
}

#[test]
fn published_image_labels_use_canonical_keys() {
    assert_eq!(
        published_image_labels("0.14-trixie", "abcdef123456"),
        [
            "jackin.construct.version=0.14-trixie".to_owned(),
            "jackin.role.git.sha=abcdef123456".to_owned(),
        ]
    );
}

#[test]
fn published_image_repository_strips_tag_without_stripping_registry_port() {
    assert_eq!(
        published_image_repository("localhost:5000/projectjackin/example:latest"),
        "localhost:5000/projectjackin/example"
    );
    assert_eq!(
        published_image_repository("docker.io/projectjackin/example@sha256:abc123"),
        "docker.io/projectjackin/example"
    );
}
