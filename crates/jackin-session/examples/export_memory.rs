// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use jackin_session::{DEFAULT_EXPORT_ENTRY_BYTES, ExportError};
use jackin_testkit::{Rig, default_limits};
use tokio_util::sync::CancellationToken;

#[expect(dead_code, reason = "the probe uses part of the shared tap")]
#[path = "../tests/common/tap.rs"]
mod tap;

#[expect(dead_code, reason = "the probe uses part of the pack builder")]
#[path = "../tests/export/pack.rs"]
mod pack;

const ZEROS: usize = 1 << 20;

fn zero_stream(total: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    let chunk = vec![0_u8; ZEROS];
    let mut left = total;
    while left > 0 {
        let take = usize::try_from(left.min(ZEROS as u64))?;
        encoder.write_all(&chunk[..take])?;
        left -= take as u64;
    }
    Ok(encoder.finish()?)
}

fn lying(honest: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut item = pack::typed_size(pack::BLOB, DEFAULT_EXPORT_ENTRY_BYTES);
    item.extend(zero_stream(2 * DEFAULT_EXPORT_ENTRY_BYTES)?);
    let header = &honest[..pack::header_len(honest)];
    Ok(pack::assemble(header, &[item], 1, pack::Hash::Sha1))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let mode = std::env::args()
        .nth(1)
        .ok_or("usage: export_memory honest|lying")?;
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    handle.git(&["checkout", "-q", "-b", "work"]).await?;
    handle.commit_in_project("w.txt", b"work", "work").await?;
    let path = handle.root().join("probe.bundle");
    let target = path.display().to_string();
    handle
        .git(&["bundle", "create", "--quiet", &target, "refs/heads/work"])
        .await?;
    let honest = std::fs::read(&path)?;
    let bytes = match mode.as_str() {
        "honest" => honest,
        "lying" => lying(&honest)?,
        other => return Err(format!("unknown mode {other}").into()),
    };
    let tap = tap::Tap::new(&rig);
    let session = tap.session(&rig, default_limits());
    tap.replace_next_download(bytes);
    let result = session
        .export(&rig.config, &handle.name(), CancellationToken::new())
        .await;
    let line = match result {
        Ok(report) => format!("{mode}: exported {} branches", report.branches.len()),
        Err(ExportError::BundleRefused(refusal)) => format!("{mode}: refused: {refusal}"),
        Err(other) => format!("{mode}: {other}"),
    };
    writeln!(std::io::stdout(), "{line}")?;
    Ok(())
}
