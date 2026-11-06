// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Clock;
use jackin_session::SystemClock;

const CHILD: &str = "JACKIN_CLOCK_CHILD";
const TEST: &str = "clock::the_system_clock_reports_the_local_offset_east_and_west_of_utc";

async fn offset_under(zone: &str) -> Result<String, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let output = tokio::process::Command::new("/bin/sh")
        .args(["-c", &format!("exec \"$0\" --exact {TEST} --nocapture")])
        .arg(exe)
        .env("TZ", zone)
        .env(CHILD, "1")
        .output()
        .await?;
    assert!(output.status.success(), "{output:?}");
    Ok(String::from_utf8(output.stdout)?)
}

#[tokio::test]
async fn the_system_clock_reports_the_local_offset_east_and_west_of_utc()
-> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os(CHILD).is_some() {
        println!("offset={}", SystemClock.utc_offset_secs());
        return Ok(());
    }
    assert!(
        offset_under("XXX-5:30").await?.contains("offset=19800"),
        "east of UTC"
    );
    assert!(
        offset_under("XXX5").await?.contains("offset=-18000"),
        "west of UTC"
    );
    assert!(
        offset_under("UTC").await?.contains("offset=0"),
        "UTC itself"
    );
    Ok(())
}
