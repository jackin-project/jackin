// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::{Clock, GroupFault, ProcessError, Program};
use jackin_openshell::{GatewayError, GatewayOp, TransferOp};
use jackin_session::ExportError;
use jackin_testkit::{FakeClock, Fault, Rig, TestResult, Trigger, default_limits, guard};
use tokio_util::sync::CancellationToken;

use super::cancels::CancelsOn;
use super::replies::RepliesOn;
use super::support::{bundles_left, export, exports, ref_of, snapshot, staging_left};
use super::tap::{RmRun, Tap};

#[tokio::test]
async fn an_interrupted_export_leaves_no_ref_and_no_staging_file_and_a_retry_succeeds() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("a.txt", b"a", "work").await?;
        let session = &rig.world.session;
        let before = snapshot(&rig).await?;

        let token = CancellationToken::new();
        token.cancel();
        let early = session.export(&rig.config, &name, token).await;
        assert!(
            matches!(early, Err(ExportError::Cancelled { group: None })),
            "{early:?}"
        );

        let mid = CancellationToken::new();
        rig.world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Download),
            token: mid.clone(),
        });
        let cancelled = session.export(&rig.config, &name, mid).await;
        assert!(
            matches!(cancelled, Err(ExportError::Cancelled { group: None })),
            "{cancelled:?}"
        );

        rig.world.gateway.inject(Fault::CorruptDownload);
        let corrupt = export(&rig, &name).await;
        assert!(
            matches!(
                corrupt,
                Err(ExportError::Gateway(GatewayError::Transfer {
                    op: TransferOp::Digest,
                    ..
                }))
            ),
            "{corrupt:?}"
        );

        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::Download),
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let failed = export(&rig, &name).await;
        assert!(
            matches!(
                failed,
                Err(ExportError::Gateway(GatewayError::Protocol { .. }))
            ),
            "{failed:?}"
        );

        let after = snapshot(&rig).await?;
        assert_eq!(after.rest, before.rest);
        assert!(exports(&after).is_empty());
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        assert_eq!(bundles_left(&handle), Vec::<String>::new());

        export(&rig, &name).await?;
        assert!(exports(&snapshot(&rig).await?).contains_key(&ref_of(&name, "main")));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_that_ends_a_host_git_call_is_cancelled_and_leaves_nothing() -> TestResult {
    guard(async {
        for word in ["index-pack", "update-ref"] {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            handle.commit_in_project("a.txt", b"a", "work").await?;
            let before = snapshot(&rig).await?;
            let token = CancellationToken::new();
            let session =
                rig.session_over(Arc::new(CancelsOn::new(&rig.world.runner, word, &token)));

            let cancelled = session.export(&rig.config, &name, token).await;

            assert!(
                matches!(cancelled, Err(ExportError::Cancelled { group: None })),
                "{word}: {cancelled:?}"
            );
            let after = snapshot(&rig).await?;
            assert_eq!(after.rest, before.rest, "{word}");
            assert!(exports(&after).is_empty(), "{word}");
            assert_eq!(staging_left(&rig)?, Vec::<String>::new(), "{word}");
            assert_eq!(bundles_left(&handle), Vec::<String>::new(), "{word}");

            export(&rig, &name).await?;
            assert!(exports(&snapshot(&rig).await?).contains_key(&ref_of(&name, "main")));
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cleanup_of_the_sandbox_bundle_has_its_own_short_bound_and_never_changes_the_result()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("a.txt", b"a", "work").await?;
        let limits = default_limits();
        assert_eq!(limits.upload, Duration::from_mins(10));
        let tap = Tap::new(&rig);
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&rig.world.clock);
        tap.stall_rm(clock);
        let session = tap.session(&rig, limits);
        let slept = || rig.world.clock.slept().iter().sum::<Duration>();

        let token = CancellationToken::new();
        rig.world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Download),
            token: token.clone(),
        });
        let before = slept();
        let cancelled = session.export(&rig.config, &name, token).await;
        assert!(
            matches!(cancelled, Err(ExportError::Cancelled { group: None })),
            "{cancelled:?}"
        );
        let first = slept().saturating_sub(before);
        assert!(
            first > Duration::ZERO && first <= Duration::from_secs(10),
            "{first:?}"
        );

        rig.world.gateway.inject(Fault::CorruptDownload);
        let before = slept();
        let corrupt = session
            .export(&rig.config, &name, CancellationToken::new())
            .await;
        assert!(
            matches!(
                corrupt,
                Err(ExportError::Gateway(GatewayError::Transfer {
                    op: TransferOp::Digest,
                    ..
                }))
            ),
            "{corrupt:?}"
        );
        let second = slept().saturating_sub(before);
        assert!(
            second > Duration::ZERO && second <= Duration::from_secs(10),
            "{second:?}"
        );

        let bound = RmRun {
            timeout: Duration::from_secs(10),
            cancelled: false,
        };
        assert_eq!(tap.rm_runs(), vec![bound, bound]);
        Ok(())
    })
    .await
}

fn still_alive() -> GroupFault {
    GroupFault::Alive {
        after: Duration::from_secs(5),
    }
}

const ALIVE_TEXT: &str = "processes of its group were still alive after 5s";

#[tokio::test]
async fn a_host_git_call_cancelled_with_a_group_fault_exports_a_cancel_that_carries_it()
-> TestResult {
    guard(async {
        for word in ["index-pack", "update-ref"] {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            handle.commit_in_project("a.txt", b"a", "work").await?;
            let before = snapshot(&rig).await?;
            let session =
                rig.session_over(Arc::new(RepliesOn::new(&rig.world.runner, word, || {
                    ProcessError::Cancelled {
                        program: Program::Git,
                        group: Some(still_alive()),
                    }
                })));

            let cancelled = session
                .export(&rig.config, &name, CancellationToken::new())
                .await;

            let Err(error) = cancelled else {
                return Err(format!("{word}: the export succeeded").into());
            };
            assert_eq!(
                error.to_string(),
                format!("cancelled; {ALIVE_TEXT}"),
                "{word}"
            );
            let ExportError::Cancelled { group } = error else {
                return Err(format!("{word}: {error:?}").into());
            };
            assert_eq!(group, Some(still_alive()), "{word}");
            let after = snapshot(&rig).await?;
            assert_eq!(after.rest, before.rest, "{word}");
            assert!(exports(&after).is_empty(), "{word}");
            assert_eq!(staging_left(&rig)?, Vec::<String>::new(), "{word}");
            assert_eq!(bundles_left(&handle), Vec::<String>::new(), "{word}");

            export(&rig, &name).await?;
            assert!(exports(&snapshot(&rig).await?).contains_key(&ref_of(&name, "main")));
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_gateway_cancel_or_timeout_with_a_group_fault_keeps_the_fault_through_the_export()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("a.txt", b"a", "work").await?;
        let before = snapshot(&rig).await?;

        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::Download),
            error: GatewayError::Cancelled {
                op: GatewayOp::Download,
                group: Some(still_alive()),
            },
        });
        match export(&rig, &name).await {
            Err(ExportError::Cancelled { group }) => assert_eq!(group, Some(still_alive())),
            other => return Err(format!("cancel: {other:?}").into()),
        }

        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::Download),
            error: GatewayError::Timeout {
                op: GatewayOp::Download,
                after: Duration::from_secs(9),
                group: Some(still_alive()),
            },
        });
        match export(&rig, &name).await {
            Err(ExportError::Gateway(GatewayError::Timeout { after, group, .. })) => {
                assert_eq!(after, Duration::from_secs(9));
                assert_eq!(group, Some(still_alive()));
            }
            other => return Err(format!("timeout: {other:?}").into()),
        }

        let after = snapshot(&rig).await?;
        assert_eq!(after.rest, before.rest);
        assert!(exports(&after).is_empty());
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        Ok(())
    })
    .await
}
