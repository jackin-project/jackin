// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use super::support::{TestResult, guard};

#[tokio::test(start_paused = true)]
async fn the_suite_guard_lets_two_minutes_of_work_finish_and_stops_three_minutes_of_work()
-> TestResult {
    guard(async {
        tokio::time::sleep(Duration::from_secs(120)).await;
        Ok(())
    })
    .await?;

    let hung = guard(async {
        tokio::time::sleep(Duration::from_secs(180)).await;
        Ok(())
    })
    .await;
    let message = hung.expect_err("the work outlives the guard").to_string();
    assert!(message.contains("the test hung"), "{message}");
    Ok(())
}
