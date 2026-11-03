// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::{ProjectRoot, SandboxPath, sandbox_layout};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, ProgramRun, ProgramStdin};
use jackin_testkit::{FakeGateway, Fault, ProgramScript, Trigger};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, fake, guard, path, project, ready, sh, supervised};

const MARKER: &str = "/sandbox/project/README.md";

async fn upload(
    fake: Arc<FakeGateway>,
    name: jackin_core::SandboxName,
    source: ProjectRoot,
    cancel: CancellationToken,
) -> Result<(), GatewayError> {
    fake.upload_tree(&name, &source, &sandbox_layout::PROJECT, &cancel)
        .await
}

fn uploaded(handle: &jackin_testkit::SandboxHandle) -> bool {
    SandboxPath::parse(MARKER)
        .ok()
        .and_then(|marker| handle.file(&marker))
        .is_some()
}

#[tokio::test]
async fn a_held_upload_waits_at_the_hold_until_it_is_released_and_then_completes() -> TestResult {
    guard(async {
        let fake = Arc::new(fake()?);
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let (_dir, source) = project().await?;
        let trigger = Trigger::Op(GatewayOp::UploadTree);
        fake.inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        assert!(fake.held().is_empty());

        let work = tokio::spawn(upload(
            Arc::clone(&fake),
            handle.name(),
            source.clone(),
            CancellationToken::new(),
        ));
        assert!(fake.until_held(&trigger).await, "the upload never arrived");
        assert_eq!(fake.held(), std::slice::from_ref(&trigger));
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        assert!(!work.is_finished(), "the hold did not hold");
        assert!(!uploaded(&handle));

        assert!(fake.release(&trigger));
        work.await??;
        assert!(uploaded(&handle));
        assert!(fake.held().is_empty());
        assert!(!fake.release(&trigger), "a hold is released once");

        upload(
            Arc::clone(&fake),
            handle.name(),
            source,
            CancellationToken::new(),
        )
        .await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_release_before_the_call_arrives_lets_it_through_and_a_cancel_ends_a_held_call()
-> TestResult {
    guard(async {
        let fake = Arc::new(fake()?);
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let (_dir, source) = project().await?;
        let trigger = Trigger::Op(GatewayOp::UploadTree);
        fake.inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        assert!(fake.release(&trigger));
        upload(
            Arc::clone(&fake),
            handle.name(),
            source.clone(),
            CancellationToken::new(),
        )
        .await?;

        fake.inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        let cancel = CancellationToken::new();
        let work = tokio::spawn(upload(
            Arc::clone(&fake),
            handle.name(),
            source,
            cancel.clone(),
        ));
        assert!(fake.until_held(&trigger).await);
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), work).await??;
        assert!(
            matches!(
                result,
                Err(GatewayError::Cancelled {
                    op: GatewayOp::UploadTree
                })
            ),
            "{result:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_hold_on_a_program_stops_only_that_program() -> TestResult {
    guard(async {
        let fake = Arc::new(fake()?);
        let handle = ready(&fake, supervised()?).await?;
        let held = path("/usr/local/bin/held")?;
        for program in ["/usr/local/bin/held", "/usr/local/bin/other"] {
            fake.script_program(
                &path(program)?,
                ProgramScript {
                    exit_code: 0,
                    stdout: b"hi\n".to_vec(),
                    stderr: Vec::new(),
                },
            );
        }
        let trigger = Trigger::Program(held.clone());
        fake.inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        let run = |program: &str| ProgramRun {
            sandbox: handle.name(),
            program: path(program).unwrap(),
            args: vec!["hi".to_owned()],
            workdir: None,
            env: std::collections::BTreeMap::new(),
            stdin: ProgramStdin::Closed,
            touch_on_success: None,
            timeout: Duration::from_secs(30),
        };
        let other = fake
            .run_program(&run("/usr/local/bin/other"), &CancellationToken::new())
            .await?;
        assert_eq!(other.exit_code, 0);

        let waiting = tokio::spawn({
            let fake = Arc::clone(&fake);
            let request = run("/usr/local/bin/held");
            async move { fake.run_program(&request, &CancellationToken::new()).await }
        });
        assert!(fake.until_held(&trigger).await);
        assert!(!waiting.is_finished());
        fake.release(&trigger);
        let outcome = waiting.await??;
        assert_eq!(outcome.stdout, b"hi\n");
        Ok(())
    })
    .await
}
