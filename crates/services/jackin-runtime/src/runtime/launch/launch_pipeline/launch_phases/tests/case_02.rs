// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::runtime::docker_profile::{DindGrant, WorkflowGrant};

fn loose_role_manifest(dir: &tempfile::TempDir) -> RoleManifest {
    std::fs::write(
        dir.path().join("jackin.role.toml"),
        "version = \"v1alpha6\"\ndockerfile = \"Dockerfile\"\nagents = [\"claude\"]\n\n[claude]\n\n[docker]\ndind = \"rootless\"\ncapabilities_add = [\"SYS_ADMIN\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    jackin_manifest::load_role_manifest(dir.path()).unwrap()
}

#[test]
fn workflow_grant_clamps_loose_role_grants_before_container_creation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let manifest_temp = tempdir().unwrap();
    let role_manifest = loose_role_manifest(&manifest_temp);
    let clamp = WorkflowGrant {
        cpu_max: Some(1.0),
        deny_caps: vec!["SYS_ADMIN".to_owned()],
        ..Default::default()
    };

    let clamped = validate_launch_grants(GrantPhaseInput {
        config: &config,
        workspace_label: "workspace",
        workspace_docker: None,
        opts_docker_profile: None,
        selector: &selector,
        role_manifest: &role_manifest,
        workflow_grant: Some(&clamp),
    })
    .expect("clamped launch valid");
    assert_eq!(
        clamped.effective_grants.dind,
        DindGrant::None,
        "workflow clamp must deny role-requested DinD"
    );
    assert!(
        !clamped
            .effective_grants
            .capabilities_add
            .iter()
            .any(|cap| cap == "SYS_ADMIN"),
        "workflow clamp must drop denied caps; got: {:?}",
        clamped.effective_grants.capabilities_add
    );
    assert!(
        !clamped.effective_grants.sudo,
        "workflow clamp must deny sudo"
    );
    assert!(
        clamped.effective_grants.no_new_privileges,
        "workflow clamp must enforce no-new-privileges"
    );
    assert_eq!(
        clamped.effective_grants.cpus,
        Some(1.0),
        "workflow clamp must lower the CPU maximum"
    );
}

#[test]
fn manual_launch_without_workflow_grant_keeps_role_grants() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let manifest_temp = tempdir().unwrap();
    let role_manifest = loose_role_manifest(&manifest_temp);

    let manual = validate_launch_grants(GrantPhaseInput {
        config: &config,
        workspace_label: "workspace",
        workspace_docker: None,
        opts_docker_profile: None,
        selector: &selector,
        role_manifest: &role_manifest,
        workflow_grant: None,
    })
    .expect("manual launch valid");
    assert_eq!(
        manual.effective_grants.dind,
        DindGrant::Rootless,
        "manual path must keep its existing meaning without a workflow grant"
    );
    assert!(
        manual
            .effective_grants
            .capabilities_add
            .iter()
            .any(|cap| cap == "SYS_ADMIN"),
        "manual path must keep role-requested caps; got: {:?}",
        manual.effective_grants.capabilities_add
    );
}
