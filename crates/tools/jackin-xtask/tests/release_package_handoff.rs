// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use jackin_process::{ExecRequest, exec_sync};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};
use velnor_actions_contract::{
    PACKAGE_RELEASE_PLAN_SCHEMA_V1, PackageReleasePlan, PackageReleasePlanProduct,
    canonical_json_bytes,
    config::RustBinaryReleaseCompanion,
    package_release::{PackageReleaseChannel, PackageReleasePayloadFormat},
    preview_production::{
        RELEASE_PRODUCTION_SCHEMA_V1, ReleaseProductionContext, ReleaseProductionRequest,
        ReleaseProductionStage, expected_release_payload_pairs, release_plan_sha256,
    },
};

fn product(id: &str, bins: &[&str], targets: &[&str]) -> PackageReleasePlanProduct {
    PackageReleasePlanProduct {
        id: id.to_owned(),
        package: id.to_owned(),
        bins: bins.iter().map(|bin| (*bin).to_owned()).collect(),
        targets: targets.iter().map(|target| (*target).to_owned()).collect(),
        format: PackageReleasePayloadFormat::TarGz,
    }
}

fn request(channel: PackageReleaseChannel) -> Result<ReleaseProductionRequest> {
    let linux = ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"];
    let plan = PackageReleasePlan {
        schema: PACKAGE_RELEASE_PLAN_SCHEMA_V1.to_owned(),
        channel,
        repository: "jackin-project/jackin".to_owned(),
        default_branch: "main".to_owned(),
        base_version: env!("CARGO_PKG_VERSION").to_owned(),
        products: vec![
            product(
                "jackin",
                &["jackin", "jackin-role", "jackin-usage-broker"],
                &[
                    "aarch64-apple-darwin",
                    linux[0],
                    "x86_64-apple-darwin",
                    linux[1],
                ],
            ),
            product("jackin-capsule", &["jackin-capsule"], &linux),
        ],
        companions: vec![RustBinaryReleaseCompanion {
            id: "capsule-manifest".to_owned(),
            path: "capsule-manifest.json".to_owned(),
            asset_name: "capsule-manifest.json".to_owned(),
        }],
    };
    let pair = expected_release_payload_pairs(&plan)
        .context("fixture plan must derive the full payload inventory")?
        .into_iter()
        .next()
        .context("fixture plan must contain a payload pair")?;
    Ok(ReleaseProductionRequest {
        schema: RELEASE_PRODUCTION_SCHEMA_V1.to_owned(),
        policy_sha256: "c".repeat(64),
        plan_sha256: release_plan_sha256(&plan)
            .context("fixture plan must have a canonical digest")?,
        plan,
        context: ReleaseProductionContext {
            repository_id: 1,
            head_repository_id: 1,
            source_sha: "a".repeat(40),
            source_ref: "refs/heads/main".to_owned(),
            workflow_authority_sha: "a".repeat(40),
            run_id: 2,
            run_attempt: 1,
            ci_run_id: 1,
            ci_run_attempt: 1,
            commit_count: if channel == PackageReleaseChannel::Stable {
                0
            } else {
                42
            },
            version: if channel == PackageReleaseChannel::Stable {
                env!("CARGO_PKG_VERSION").to_owned()
            } else {
                format!("{}-preview.42+aaaaaaa", env!("CARGO_PKG_VERSION"))
            },
        },
        stage: ReleaseProductionStage::Payload { pair },
    })
}

fn tool_traps(root: &Path) -> Result<std::path::PathBuf> {
    let tools = root.join("tools");
    fs::create_dir(&tools).context("create private build-tool trap directory")?;
    for tool in [
        "cargo",
        "cargo-zigbuild",
        "zig",
        "mbx",
        "mise",
        "git",
        "tar",
    ] {
        let path = tools.join(tool);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf invoked > '{}'\nexit 97\n",
                root.join("tool-called").display()
            ),
        )
        .context("write build-tool trap script")?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .context("make build-tool trap executable")?;
    }
    Ok(tools)
}

fn invocation(root: &Path, request: &ReleaseProductionRequest) -> Result<ExecRequest> {
    let native = root.join("velnor");
    let output = root.join("output");
    fs::create_dir(&native).context("create native handoff fixture directory")?;
    fs::create_dir(&output).context("create producer output fixture directory")?;
    fs::write(output.join("sentinel"), b"preserve")
        .context("make producer output deliberately nonempty")?;
    let path = native.join("release-task-request.json");
    fs::write(
        &path,
        canonical_json_bytes(&serde_json::json!({
            "schema": "velnor.release-task.v1", "request": request, "inputs": [],
        }))
        .context("serialize the canonical full native request")?,
    )
    .context("write the explicit native request file")?;
    let bindings = vec![
        ("PATH", tool_traps(root)?.into_os_string()),
        ("VELNOR_RELEASE_TASK_REQUEST_FILE", path.into_os_string()),
        (
            "VELNOR_RELEASE_OUTPUT_MANIFEST",
            output.join("package-output.json").into_os_string(),
        ),
        ("VELNOR_RELEASE_OUTPUT_DIRECTORY", output.into_os_string()),
        (
            "VELNOR_RELEASE_SOURCE_COMMIT",
            request.context.source_sha.clone().into(),
        ),
        (
            "VELNOR_RELEASE_SOURCE_REF",
            request.context.source_ref.clone().into(),
        ),
        (
            "VELNOR_RELEASE_VERSION",
            request.context.version.clone().into(),
        ),
        (
            "VELNOR_RELEASE_COMMIT_COUNT",
            request.context.commit_count.to_string().into(),
        ),
    ];
    Ok(
        ExecRequest::new(env!("CARGO_BIN_EXE_jackin-xtask"), ["release-package"])
            .cwd(root)
            .env_clear()
            .envs(bindings)
            .timeout(Duration::from_secs(5))
            .output_limits(64 * 1024, 64 * 1024),
    )
}

#[test]
fn explicit_native_request_reaches_output_guard_without_runner_temp_or_build_tools() {
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let request = request(channel).unwrap();
        request.validate().unwrap();
        let output = exec_sync(&invocation(&root, &request).unwrap()).unwrap();
        assert!(!output.success && !output.timed_out);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("producer output must be empty"), "{stderr}");
        assert_eq!(fs::read(root.join("output/sentinel")).unwrap(), b"preserve");
        assert!(!root.join("tool-called").exists());
    }
}

#[test]
fn explicit_native_request_reports_missing_binding_and_rejects_conflicting_source() {
    for missing in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut command =
            invocation(&root, &request(PackageReleaseChannel::Preview).unwrap()).unwrap();
        let key = "VELNOR_RELEASE_SOURCE_COMMIT";
        command.env.retain(|(name, _)| name != key);
        if !missing {
            command = command.envs([(key, "b".repeat(40))]);
        }
        let output = exec_sync(&command).unwrap();
        assert!(!output.success && !output.timed_out);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let expected = if missing {
            "missing native environment variable VELNOR_RELEASE_SOURCE_COMMIT"
        } else {
            "source handoff mismatch"
        };
        assert!(stderr.contains(expected), "{stderr}");
        assert!(!root.join("tool-called").exists());
    }
}

#[test]
fn legacy_request_environment_is_not_a_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let mut command = invocation(&root, &request(PackageReleaseChannel::Preview).unwrap()).unwrap();
    let path = command
        .env
        .iter()
        .find(|(key, _)| key == "VELNOR_RELEASE_TASK_REQUEST_FILE")
        .unwrap()
        .1
        .clone();
    command
        .env
        .retain(|(key, _)| key != "VELNOR_RELEASE_TASK_REQUEST_FILE");
    command = command.envs([("VELNOR_PREVIEW_REQUEST_FILE", path)]);
    let output = exec_sync(&command).unwrap();
    assert!(!output.success && !output.timed_out);
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("missing native environment variable VELNOR_RELEASE_TASK_REQUEST_FILE")
    );
    assert!(!root.join("tool-called").exists());
}

#[test]
fn legacy_preview_command_is_not_an_alias() {
    let command = ExecRequest::new(
        env!("CARGO_BIN_EXE_jackin-xtask"),
        ["release-preview-package"],
    )
    .env_clear()
    .timeout(Duration::from_secs(5))
    .output_limits(64 * 1024, 64 * 1024);
    let output = exec_sync(&command).unwrap();
    assert!(!output.success && !output.timed_out);
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
}
