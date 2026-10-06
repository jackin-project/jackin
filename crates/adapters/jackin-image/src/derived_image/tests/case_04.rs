// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn preserves_runtime_assets_when_repo_dockerignore_excludes_hidden_paths() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join(".dockerignore"),
        r".*
.jackin-runtime
",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(repo.path(), &validated, None, None).unwrap();
    let dockerignore = std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();

    assert!(dockerignore.contains("!.jackin-runtime/"));
    assert!(dockerignore.contains("!.jackin-runtime/entrypoint.sh"));
    assert!(dockerignore.contains("!.jackin-runtime/DerivedDockerfile"));
}

#[test]
fn stages_agent_status_reporter_assets_into_the_image() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(repo.path(), &validated, None, None).unwrap();

    assert!(
        build
            .context_dir
            .join(".jackin-runtime/agent-status/hooks/claude/report-hook.sh")
            .is_file()
    );
    assert!(
        build
            .context_dir
            .join(".jackin-runtime/agent-status/packs/kimi.toml")
            .is_file()
    );
    let dockerfile = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert!(
        dockerfile.contains(
            "COPY --link --chmod=0755 .jackin-runtime/agent-status /jackin/runtime/agent-status"
        ),
        "derived Dockerfile must COPY the agent-status assets"
    );
    let dockerignore = std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();
    assert!(dockerignore.contains("!.jackin-runtime/agent-status/"));
}

#[test]
fn uses_base_image_override_instead_of_workspace_dockerfile() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(
        repo.path(),
        &validated,
        Some("docker.io/myorg/my-role:latest"),
        None,
    )
    .unwrap();

    let contents = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert!(
        contents
            .starts_with("# syntax=docker/dockerfile:1.7\nFROM docker.io/myorg/my-role:latest\n")
    );
    assert!(!contents.contains("projectjackin/construct:"));
}

#[test]
fn base_image_override_context_excludes_unused_repo_files() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\nCOPY huge.txt /tmp/huge.txt\n",
    )
    .unwrap();
    std::fs::write(repo.path().join("huge.txt"), "unused by published base\n").unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(
        repo.path(),
        &validated,
        Some("docker.io/myorg/my-role:latest"),
        None,
    )
    .unwrap();

    assert!(!build.context_dir.join("Dockerfile").exists());
    assert!(!build.context_dir.join("huge.txt").exists());
    assert!(
        build
            .context_dir
            .join(".jackin-runtime/entrypoint.sh")
            .is_file()
    );
    assert!(build.dockerfile_path.is_file());
}

#[test]
fn base_image_override_context_keeps_only_declared_hooks() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("hooks")).unwrap();
    std::fs::write(repo.path().join("hooks/source.sh"), "#!/bin/sh\n").unwrap();
    std::fs::write(repo.path().join("hooks/setup-once.sh"), "#!/bin/sh\n").unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude"]

[claude]
plugins = []

[hooks]
source = "hooks/source.sh"
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(
        repo.path(),
        &validated,
        Some("docker.io/myorg/my-role:latest"),
        None,
    )
    .unwrap();
    let dockerignore = std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();

    assert!(build.context_dir.join("hooks/source.sh").is_file());
    assert!(!build.context_dir.join("hooks/setup-once.sh").exists());
    assert!(!build.context_dir.join("Dockerfile").exists());
    assert!(dockerignore.contains("!hooks/source.sh"));
    assert!(!dockerignore.contains("!hooks/setup-once.sh"));
}

#[test]
fn jackin_construct_image_override_no_alias() {
    let input = "FROM projectjackin/construct:0.1-trixie\nUSER agent\n";
    let result = apply_construct_image_override(input, "jackin-local/construct:trixie");
    assert!(
        result.starts_with("FROM jackin-local/construct:trixie\n"),
        "override without alias must not add trailing space; got:\n{result}"
    );
}

#[test]
fn jackin_construct_image_override_preserves_as_alias() {
    let input = "FROM projectjackin/construct:0.1-trixie AS runtime\nUSER agent\n";
    let result = apply_construct_image_override(input, "jackin-local/construct:trixie");
    assert!(
        result.starts_with("FROM jackin-local/construct:trixie AS runtime\n"),
        "override must replace the image but preserve the AS alias; got:\n{result}"
    );
}

#[test]
fn jackin_construct_image_override_handles_digest_pinned_from() {
    let input = "FROM projectjackin/construct:0.1-trixie@sha256:0b076bfbc53d36794fe54b1a9cab670f85f831af86d78426b1a88a8ac192d445 AS runtime\nUSER agent\n";
    let result = apply_construct_image_override(input, "jackin-local/construct:trixie");
    assert!(
        result.starts_with("FROM jackin-local/construct:trixie AS runtime\n"),
        "override must replace tag+digest and preserve AS alias; got:\n{result}"
    );
}

#[cfg(unix)]
#[test]
fn dereferences_contained_symlinks_in_repo_build_context() {
    // Contained symlinks (CLAUDE.md -> AGENTS.md repo conventions) copy
    // their target content instead of failing the build.
    let repo = tempdir().unwrap();
    minimal_role_repo(repo.path());
    std::fs::write(repo.path().join("AGENTS.md"), "agents\n").unwrap();
    symlink("AGENTS.md", repo.path().join("CLAUDE.md")).unwrap();
    std::fs::write(repo.path().join("shared.txt"), "hello\n").unwrap();
    symlink(
        repo.path().join("shared.txt"),
        repo.path().join("linked.txt"),
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(repo.path(), &validated, None, None).unwrap();

    assert_eq!(
        std::fs::read_to_string(build.context_dir.join("CLAUDE.md")).unwrap(),
        "agents\n"
    );
    assert_eq!(
        std::fs::read_to_string(build.context_dir.join("linked.txt")).unwrap(),
        "hello\n"
    );
    assert!(
        !std::fs::symlink_metadata(build.context_dir.join("CLAUDE.md"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn rejects_escaping_symlinks_in_repo_build_context() {
    let repo = tempdir().unwrap();
    minimal_role_repo(repo.path());
    let outside = tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret\n").unwrap();
    symlink(
        outside.path().join("secret.txt"),
        repo.path().join("linked.txt"),
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let error = create_derived_build_context(repo.path(), &validated, None, None)
        .expect_err("escaping symlinks should be rejected");

    assert!(error.to_string().contains("symlink"));
    assert!(error.to_string().contains("linked.txt"));
}

#[cfg(unix)]
#[test]
fn rejects_dangling_symlinks_in_repo_build_context() {
    let repo = tempdir().unwrap();
    minimal_role_repo(repo.path());
    symlink(
        repo.path().join("missing.txt"),
        repo.path().join("linked.txt"),
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let error = create_derived_build_context(repo.path(), &validated, None, None)
        .expect_err("dangling symlinks should be rejected");

    assert!(error.to_string().contains("symlink"));
    assert!(error.to_string().contains("linked.txt"));
}

#[cfg(unix)]
#[test]
fn dereferences_contained_hook_symlinks_in_build_context() {
    let repo = tempdir().unwrap();
    minimal_role_repo(repo.path());
    std::fs::create_dir_all(repo.path().join("hooks")).unwrap();
    std::fs::write(repo.path().join("hooks/shared.sh"), "#!/bin/sh\n").unwrap();
    symlink("shared.sh", repo.path().join("hooks/source.sh")).unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
source = "hooks/source.sh"
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context(repo.path(), &validated, None, None).unwrap();

    assert_eq!(
        std::fs::read_to_string(build.context_dir.join("hooks/source.sh")).unwrap(),
        "#!/bin/sh\n"
    );
}

#[test]
fn image_ref_validator_accepts_canonical_forms() {
    assert!(looks_like_valid_image_ref("ubuntu"));
    assert!(looks_like_valid_image_ref("ubuntu:24.04"));
    assert!(looks_like_valid_image_ref("ghcr.io/owner/img:1.2.3"));
    assert!(looks_like_valid_image_ref(
        "ghcr.io/owner/img:tag@sha256:abc123"
    ));
    assert!(looks_like_valid_image_ref("localhost:5000/foo/bar"));
}

#[test]
fn image_ref_validator_rejects_injection_vectors() {
    // The threats the allowlist guards against — a poisoned env
    // var must not inject extra Dockerfile instructions.
    assert!(!looks_like_valid_image_ref(""));
    assert!(!looks_like_valid_image_ref("foo bar"));
    assert!(!looks_like_valid_image_ref("foo\nFROM evil"));
    assert!(!looks_like_valid_image_ref("foo;rm -rf /"));
    assert!(!looks_like_valid_image_ref("foo$(whoami)"));
    assert!(!looks_like_valid_image_ref("foo`id`"));
    assert!(!looks_like_valid_image_ref("foo|sh"));
    assert!(!looks_like_valid_image_ref(&"x".repeat(257)));
}
