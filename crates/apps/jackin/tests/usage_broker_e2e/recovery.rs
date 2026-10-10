// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use super::*;
use jackin_protocol::usage_broker::{
    USAGE_BROKER_PROTOCOL_VERSION, UsageCatalogEntry, UsageProjectionRefreshStateV1,
    UsageProjectionSchemaV1, UsageProjectionV1,
};
use jackin_usage::coordinator::{
    AccountStateStore, FileAccountStateStore, FileProjectionStateStore, ProjectionStateEnvelope,
};

const RECOVERY_CLIENTS: usize = 8;

#[test]
fn usage_broker_killed_owner_recovers_once_without_a_herd() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    let executable = std::env::current_exe()?;
    seed_authoritative_catalog(root)?;
    let owner_request = child_request(&executable, root, "owner", "owner", None);
    let mut owner = jackin_process::spawn_sync(&owner_request)?;
    wait_until(Duration::from_secs(10), || {
        root.join("owner-active").exists()
    });
    owner.kill()?;
    let _owner_status = owner.wait()?;

    // Backdate both persisted timestamps together so this process-level
    // recovery test does not sleep for Claude's full five-minute attempt
    // floor. They use the same fake-clock premise, while production keeps the
    // provider invocation timestamp as the floor authority. Unit tests cover
    // the real restart floor boundary without changing persisted time.
    let recovery_now = epoch_now();
    let account_store = FileAccountStateStore::under_data_dir(&root.join("data"));
    let mut abandoned = account_store
        .load(&capability(), recovery_now)?
        .context("killed owner left no persisted account state")?;
    assert_eq!(abandoned.generation, 1);
    assert_eq!(abandoned.phase, UsageRefreshPhase::Updating);
    let original_start = abandoned
        .started_at_epoch
        .context("active owner state lacked a start timestamp")?;
    assert!(original_start >= recovery_now.saturating_sub(300));
    let original_invocation = abandoned
        .provider_invoked_at_epoch
        .context("active owner state lacked a provider invocation timestamp")?;
    assert!(original_invocation >= original_start);
    assert!(original_invocation >= recovery_now.saturating_sub(300));
    assert!(original_invocation <= recovery_now);

    let expired_attempt_at = recovery_now.saturating_sub(301);
    abandoned.started_at_epoch = Some(expired_attempt_at);
    abandoned.provider_invoked_at_epoch = Some(expired_attempt_at);
    assert_eq!(
        abandoned.started_at_epoch,
        abandoned.provider_invoked_at_epoch
    );
    account_store.store(&abandoned, recovery_now)?;

    let mut recovery = Vec::new();
    for child in 0..RECOVERY_CLIENTS {
        let name = format!("recovery-{child}");
        let request = child_request(&executable, root, &name, "recovery", Some(RECOVERY_CLIENTS));
        recovery.push(jackin_process::spawn_sync(&request)?);
    }
    wait_until(Duration::from_secs(10), || {
        entries_with_prefix(root, "ready-") == RECOVERY_CLIENTS
    });
    fs::write(root.join("go"), b"go\n")?;
    for mut child in recovery {
        assert!(child.wait()?.success());
    }
    assert_eq!(entries_with_prefix(root, "provider-call-"), 2);
    Ok(())
}

fn seed_authoritative_catalog(root: &Path) -> Result<()> {
    let broker_instance_id = "e2e-recovery-fixture".to_owned();
    let catalog_revision = format!("e2e-catalog-{USAGE_BROKER_PROTOCOL_VERSION}");
    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: format!("{broker_instance_id}:empty"),
        generated_at_epoch: epoch_now(),
        discovery_revision: catalog_revision.clone(),
        broker_instance_id: broker_instance_id.clone(),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    };
    let envelope = ProjectionStateEnvelope {
        schema_version: 2,
        projection,
        aliases: Vec::new(),
        catalog_revision,
        catalog: vec![UsageCatalogEntry {
            capability: capability(),
            revision: format!("e2e-shared-account-{USAGE_BROKER_PROTOCOL_VERSION}"),
        }],
        retry_deadline_epoch: None,
        success_deadline_epoch: None,
        broker_instance_id,
    };
    FileProjectionStateStore::under_data_dir(&root.join("data")).store(&envelope)?;
    Ok(())
}

fn child_request(
    executable: &Path,
    root: &Path,
    child: &str,
    mode: &str,
    expected: Option<usize>,
) -> jackin_process::ExecRequest {
    let mut envs: Vec<(std::ffi::OsString, std::ffi::OsString)> = vec![
        (CHILD_ENV.into(), child.into()),
        (ROOT_ENV.into(), root.as_os_str().to_owned()),
        (MODE_ENV.into(), mode.into()),
    ];
    if let Some(expected) = expected {
        envs.push((EXPECTED_ENV.into(), expected.to_string().into()));
    }
    jackin_process::ExecRequest::new(executable, ["--exact", "usage_broker_child", "--nocapture"])
        .envs(envs)
        .stdout_mode(jackin_process::StdioMode::Inherit)
        .stderr_mode(jackin_process::StdioMode::Inherit)
}
