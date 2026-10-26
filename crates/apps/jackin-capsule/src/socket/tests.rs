// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::protocol::control::ClientMsg;

mod support;
use support::*;
mod case_01;
mod frame_deadline;

#[tokio::test(flavor = "current_thread")]
async fn real_uds_frames_share_one_deadline_and_release_connections() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(30), frame_deadline::run())
        .await
        .context("real UDS frame-deadline scenario exceeded 30 seconds")??;
    Ok(())
}
