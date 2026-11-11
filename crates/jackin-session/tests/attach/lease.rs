// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Phase;
use jackin_openshell::{EndReason, InputLease, TerminalEvent};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{delivered, dropped, events, owner, until, viewer};

#[tokio::test]
async fn a_second_owner_is_read_only_until_the_first_leaves_and_a_viewer_never_writes() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;

        let mut first = owner(&rig, 1).await?;
        let mut second = owner(&rig, 1).await?;
        assert_eq!(
            events(&mut second, 2).await?,
            vec![
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::ReadOnly)
            ]
        );
        assert_eq!(delivered(&handle, &mut first, b"a").await?, b"a");
        assert!(dropped(&handle, &mut second, b"b").await?);

        first.detach().await?;
        until(|| handle.attachments() == 1).await?;
        second.input(b"c".to_vec()).await?;
        assert_eq!(
            events(&mut second, 1).await?,
            vec![TerminalEvent::Lease(InputLease::Owner)]
        );
        until(|| handle.main_input().ends_with(b"c")).await?;

        let mut watcher = viewer(&rig, 1).await?;
        assert_eq!(
            events(&mut watcher, 2).await?,
            vec![
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::ReadOnly)
            ]
        );
        second.detach().await?;
        until(|| handle.attachments() == 1).await?;
        assert!(dropped(&handle, &mut watcher, b"v").await?);
        let mut next = owner(&rig, 1).await?;
        assert_eq!(delivered(&handle, &mut next, b"d").await?, b"d");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn ctrl_c_on_a_read_only_attachment_ends_only_that_attachment() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let mut first = owner(&rig, 1).await?;
        let mut second = owner(&rig, 1).await?;
        events(&mut second, 2).await?;

        second.input(vec![0x03]).await?;
        assert_eq!(
            events(&mut second, 1).await?,
            vec![TerminalEvent::Ended(EndReason::StreamClosed)]
        );
        until(|| handle.attachments() == 1).await?;
        assert_eq!(delivered(&handle, &mut first, b"x").await?, b"x");
        assert_eq!(handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}
