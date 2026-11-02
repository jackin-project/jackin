// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Agent, FileMode, sandbox_layout};
use jackin_openshell::Gateway;
use jackin_testkit::SupervisorModel;

use super::support::{TestResult, converged, guard, has_file, launch_rig, path, ready};

#[tokio::test]
async fn a_sandbox_deleted_after_it_was_ready_is_recreated_and_gets_its_credentials_again()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway()
            .set_supervisor(SupervisorModel::NeverWritesState);
        let name = rig.sandbox_name(1)?;
        let peer = async {
            while !rig
                .world
                .clock
                .slept()
                .contains(&Duration::from_millis(500))
            {
                tokio::task::yield_now().await;
            }
            rig.gateway().set_supervisor(SupervisorModel::Cooperative);
            rig.gateway().delete_sandbox(&name).await
        };
        let (outcome, deleted) = tokio::join!(rig.launch_observed(1), peer);
        deleted?;
        ready(outcome?)?;

        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let handle = rig.handle(1).await?;
        let spec = Agent::Codex.spec();
        assert_eq!(
            handle.file_mode(&spec.auth_file.path),
            Some(FileMode(0o600))
        );
        assert!(has_file(
            &handle,
            &sandbox_layout::env_file(&super::support::launch_id(1))
        ));
        assert!(has_file(&handle, &path("/sandbox/project/src/main.rs")?));
        converged(&rig, &handle, 1)
    })
    .await
}
