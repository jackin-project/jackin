// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{HookKind, Phase, sandbox_layout};
use jackin_testkit::{ProgramScript, Rig, TestResult, guard, launch_id};

use super::support::Harness;

#[tokio::test]
async fn a_failing_preflight_hook_fails_the_load_with_clean_output_and_a_retry_completes()
-> TestResult {
    guard(async {
        let rig = Rig::builder().preflight_exit(7).build().await?;
        let hook = sandbox_layout::hook_path(HookKind::Preflight);
        rig.gateway().script_program(
            &hook,
            ProgramScript {
                exit_code: 7,
                stdout: b"\x1b[31mBOOM\x1b[0m\n".to_vec(),
                stderr: Vec::new(),
            },
        );
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        let id = launch_id(1).to_string();

        let ran = harness.run(&["load", "--launch-id", &id]).await?;
        assert_eq!(ran.exit, ExitClass::Failure, "{}", ran.stderr);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        let name = rig.sandbox_name(1)?;
        for needle in ["preflight", "7", "BOOM"] {
            assert!(ran.stderr.contains(needle), "{needle}: {}", ran.stderr);
        }
        let last = ran.stderr.trim_end().lines().last().ok_or("empty stderr")?;
        assert!(last.contains(name.as_str()) && last.contains(&id), "{last}");
        assert!(!ran.stderr.contains('\u{1b}'));
        assert_eq!(rig.handle(1).await?.phase(), Phase::Ready);

        rig.gateway().script_program(
            &hook,
            ProgramScript {
                exit_code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        harness.run(&["load", "--launch-id", &id]).await?.ok()?;
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);
        Ok(())
    })
    .await
}
