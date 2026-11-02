// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_testkit::{RepoFixture, Rig, TestResult, guard};

use super::lab::Lab;
use super::pack::{Hash, OFS_DELTA, REF_DELTA, kinds};
use super::support::{exports, ref_of, snapshot};

async fn committed(sha256: bool) -> Result<RepoFixture, Box<dyn Error>> {
    let repo = if sha256 {
        RepoFixture::new_sha256().await?
    } else {
        RepoFixture::new().await?
    };
    repo.file("src/main.rs", b"fn main() {}\n").await?;
    repo.commit().await?;
    Ok(repo)
}

async fn delta_rich(lab: &Lab, repack: &[&str]) -> TestResult {
    for round in 0..12_usize {
        let mut text = String::new();
        for line in 0..3000_usize {
            let value = if line % 100 == round { round } else { 0 };
            text.push_str(&format!("line {line} {value}\n"));
        }
        lab.handle
            .commit_in_project("big.txt", text.as_bytes(), &format!("round {round}"))
            .await?;
    }
    lab.handle.git(repack).await?;
    Ok(())
}

async fn lands(lab: &Lab, label: &str) -> TestResult {
    let report = lab
        .export()
        .await
        .map_err(|error| format!("{label}: {error:?}"))?;
    assert_eq!(report.branches.len(), 1, "{label}");
    let landed = exports(&snapshot(&lab.rig).await?);
    let host_ref = ref_of(&lab.name, "work");
    assert_eq!(
        landed.get(&host_ref).map(String::as_str),
        Some(lab.handle.git(&["rev-parse", "work"]).await?.trim()),
        "{label}"
    );
    lab.rig
        .repo
        .git(&["rev-list", "--objects", &host_ref])
        .await?;
    assert!(lab.sees.ran("fetch"), "{label}: the host imported nothing");
    Ok(())
}

fn at_least_one(kinds: &std::collections::BTreeMap<u8, usize>, kind: u8, label: &str) {
    assert!(
        kinds.get(&kind).copied().unwrap_or(0) > 0,
        "{label}: the bundle holds no entry of type {kind}: {kinds:?}"
    );
}

#[tokio::test]
async fn bundles_with_ofs_delta_and_bitmap_ref_delta_entries_are_exported_in_both_hash_formats()
-> TestResult {
    guard(async {
        let cases: [(&str, bool, Hash, u8, &[&str]); 3] = [
            (
                "ofs delta, sha1",
                false,
                Hash::Sha1,
                OFS_DELTA,
                &["repack", "-a", "-d", "-f"],
            ),
            (
                "bitmap ref delta, sha1",
                false,
                Hash::Sha1,
                REF_DELTA,
                &[
                    "-c",
                    "repack.useDeltaBaseOffset=false",
                    "repack",
                    "-a",
                    "-d",
                    "-f",
                    "--window=50",
                    "--depth=50",
                    "-b",
                ],
            ),
            (
                "bitmap ref delta, sha256",
                true,
                Hash::Sha256,
                REF_DELTA,
                &[
                    "-c",
                    "repack.useDeltaBaseOffset=false",
                    "repack",
                    "-a",
                    "-d",
                    "-f",
                    "--window=50",
                    "--depth=50",
                    "-b",
                ],
            ),
        ];
        for (label, sha256, hash, kind, repack) in cases {
            let rig = Rig::builder()
                .repo(committed(sha256).await?)
                .build()
                .await?;
            let lab = Lab::over(rig).await?;
            delta_rich(&lab, repack).await?;
            let bundle = lab.honest().await?;
            at_least_one(&kinds(&bundle, hash)?, kind, label);
            lands(&lab, label).await?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_v3_header_naming_the_sha1_format_is_exported() -> TestResult {
    guard(async {
        let lab = Lab::new().await?;
        let bundle = lab.honest().await?;
        let first = bundle.iter().position(|byte| *byte == b'\n').unwrap() + 1;
        let mut changed = b"# v3 git bundle\n@object-format=sha1\n".to_vec();
        changed.extend_from_slice(&bundle[first..]);
        lab.tap.replace_next_download(changed);
        lands(&lab, "v3 sha1 header").await
    })
    .await
}
