// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{BundleRefusal, EntryKind, ExportError, LaunchLimits};
use jackin_testkit::{TestResult, default_limits, guard};

use super::lab::{Check, Lab, family, refusal};
use super::pack::{
    BLOB, COMMIT, Hash, TAG, TREE, assemble, assemble_open, blob, copies, delta_stream, entry,
    header_len, ref_delta, zeros,
};

const CAP: u64 = 65_536;
const HASH: Hash = Hash::Sha1;

async fn lab_with(limits: LaunchLimits) -> Result<(Lab, Vec<u8>), Box<dyn std::error::Error>> {
    let mut lab = Lab::armed().await?;
    lab.advance().await?;
    let honest = lab.honest().await?;
    lab.limits(limits);
    let header = honest[..header_len(&honest)].to_vec();
    Ok((lab, header))
}

fn one(header: &[u8], item: Vec<u8>) -> Vec<u8> {
    assemble(header, &[item], 1, HASH)
}

fn too_large(kind: EntryKind, size: u64, limit: u64) -> Check {
    refusal(BundleRefusal::EntryTooLarge { kind, size, limit })
}

fn entry_cap() -> LaunchLimits {
    LaunchLimits {
        export_entry_bytes: CAP,
        ..default_limits()
    }
}

#[tokio::test]
async fn every_entry_kind_is_capped_at_the_entry_limit_and_its_size_must_be_honest() -> TestResult {
    guard(async {
        let (mut lab, header) = lab_with(entry_cap()).await?;
        let size = usize::try_from(CAP)?;
        for (kind, name, wanted) in [
            (BLOB, "blob", EntryKind::Blob),
            (TREE, "tree", EntryKind::Tree),
            (COMMIT, "commit", EntryKind::Commit),
            (TAG, "tag", EntryKind::Tag),
        ] {
            let over = one(&header, entry(kind, CAP + 1, &zeros(size + 1)));
            lab.refuses(
                &format!("{name} over"),
                over,
                &too_large(wanted, CAP + 1, CAP),
            )
            .await?;
            lab.passes_the_walker(&format!("{name} at the cap"), |header| {
                one(header, entry(kind, CAP, &zeros(size)))
            })
            .await?;
        }
        let mismatch =
            |declared: u64, inflated: usize| one(&header, entry(BLOB, declared, &zeros(inflated)));
        let lie: Check = Box::new(|error| {
            matches!(
                error,
                ExportError::BundleRefused(BundleRefusal::SizeMismatch { .. })
            )
        });
        lab.refuses(
            "declared 10, inflates to 1 MiB",
            mismatch(10, 1 << 20),
            &lie,
        )
        .await?;
        lab.refuses(
            "declared 60 KiB, inflates to 10",
            mismatch(60 * 1024, 10),
            &lie,
        )
        .await?;
        let corrupt = [typed(BLOB, 10), vec![0x78, 0x9c, 0xff, 0xff, 0xff, 0xff]].concat();
        lab.refuses(
            "corrupt zlib",
            one(&header, corrupt),
            &refusal(BundleRefusal::Inflate),
        )
        .await
    })
    .await
}

fn typed(kind: u8, size: u64) -> Vec<u8> {
    entry(kind, size, &[])[..1].to_vec()
}

#[tokio::test]
async fn a_delta_bomb_is_refused_by_its_declared_base_result_and_sum_before_any_host_import()
-> TestResult {
    guard(async {
        let (mut lab, header) = lab_with(entry_cap()).await?;
        let base = blob(&zeros(usize::try_from(CAP)?));
        let delta = |base_size: u64, result: u64, ops: &[u8]| {
            let stream = delta_stream(base_size, result, ops);
            ref_delta(HASH, stream.len() as u64, &stream)
        };
        let pack = |item: Vec<u8>| assemble(&header, &[base.clone(), item], 2, HASH);
        for (label, ops) in [("64 copies", 64), ("512 copies", 512)] {
            let result = CAP * ops as u64;
            let bomb = pack(delta(CAP, result, &copies(ops)));
            lab.refuses(label, bomb, &too_large(EntryKind::DeltaResult, result, CAP))
                .await?;
        }
        lab.refuses(
            "result cap + 1",
            pack(delta(CAP, CAP + 1, &[])),
            &too_large(EntryKind::DeltaResult, CAP + 1, CAP),
        )
        .await?;
        let packed =
            |header: &[u8], item: Vec<u8>| assemble(header, &[base.clone(), item], 2, HASH);
        lab.passes_the_walker("result at the cap", |header| {
            packed(header, delta(CAP, CAP, &copies(1)))
        })
        .await?;
        lab.refuses(
            "base cap + 1",
            pack(delta(CAP + 1, 1, &[])),
            &too_large(EntryKind::DeltaBase, CAP + 1, CAP),
        )
        .await?;
        lab.passes_the_walker("base at the cap", |header| {
            packed(header, delta(CAP, 1, &[]))
        })
        .await?;
        let huge = [vec![0xff; 10], vec![0x01]].concat();
        let stream = [vec![0], huge].concat();
        lab.refuses(
            "eleven-byte result varint",
            pack(ref_delta(HASH, stream.len() as u64, &stream)),
            &family(),
        )
        .await
    })
    .await
}

#[tokio::test]
async fn two_deltas_whose_results_overflow_the_sum_are_refused_by_a_checked_add() -> TestResult {
    guard(async {
        let (lab, header) = lab_with(LaunchLimits {
            export_entry_bytes: u64::MAX,
            export_total_bytes: u64::MAX,
            ..default_limits()
        })
        .await?;
        let stream = delta_stream(0, 1 << 63, &[]);
        let one_delta = ref_delta(HASH, stream.len() as u64, &stream);
        let bytes = assemble(&header, &[one_delta.clone(), one_delta], 2, HASH);
        let wanted: Check = Box::new(|error| {
            matches!(
                error,
                ExportError::BundleRefused(BundleRefusal::TooMuchContent { .. })
            )
        });
        lab.refuses("two results of 2^63", bytes, &wanted).await
    })
    .await
}

#[tokio::test]
async fn a_delta_instruction_stream_is_capped_and_must_match_its_declared_size() -> TestResult {
    guard(async {
        let (mut lab, header) = lab_with(entry_cap()).await?;
        let size = usize::try_from(CAP)?;
        let big = 16 * size;
        let over = one(&header, ref_delta(HASH, big as u64, &zeros(big)));
        lab.refuses(
            "stream 16 x cap",
            over,
            &too_large(EntryKind::DeltaStream, big as u64, CAP),
        )
        .await?;
        lab.passes_the_walker("stream at the cap", |header| {
            one(header, ref_delta(HASH, CAP, &zeros(size)))
        })
        .await?;
        let lie: Check = Box::new(|error| {
            matches!(
                error,
                ExportError::BundleRefused(BundleRefusal::SizeMismatch { .. })
            )
        });
        lab.refuses(
            "declared 8, inflates to 1 MiB",
            one(&header, ref_delta(HASH, 8, &zeros(1 << 20))),
            &lie,
        )
        .await
    })
    .await
}

#[tokio::test]
async fn the_object_count_is_checked_from_the_pack_header_before_any_entry() -> TestResult {
    guard(async {
        let limits = LaunchLimits {
            export_objects: 64,
            ..default_limits()
        };
        let (mut lab, header) = lab_with(limits).await?;
        let tiny: Vec<Vec<u8>> = (0..65).map(|_| blob(b"x")).collect();
        let count: Check = refusal(BundleRefusal::ObjectCount {
            count: 65,
            limit: 64,
        });
        lab.refuses("65 objects", assemble(&header, &tiny, 65, HASH), &count)
            .await?;
        lab.passes_the_walker("64 objects", |header| {
            assemble(header, &tiny[..64], 64, HASH)
        })
        .await?;
        let none = refusal(BundleRefusal::ObjectCount {
            count: u32::MAX,
            limit: 64,
        });
        lab.refuses(
            "4294967295 declared, none present",
            assemble(&header, &[], u32::MAX, HASH),
            &none,
        )
        .await?;
        lab.refuses(
            "64 declared, 3 present",
            assemble_open(&header, &tiny[..3], 64),
            &refusal(BundleRefusal::Truncated),
        )
        .await
    })
    .await
}

#[tokio::test]
async fn the_sum_of_declared_results_is_capped_for_blobs_and_for_deltas() -> TestResult {
    guard(async {
        let limits = LaunchLimits {
            export_total_bytes: 80_000,
            ..default_limits()
        };
        let (mut lab, header) = lab_with(limits).await?;
        let over: Check = refusal(BundleRefusal::TooMuchContent {
            total: 100_000,
            limit: 80_000,
        });
        let big = blob(&zeros(20_000));
        let five = vec![big.clone(); 5];
        lab.refuses("five blobs", assemble(&header, &five, 5, HASH), &over)
            .await?;
        lab.passes_the_walker("four blobs", |header| assemble(header, &five[..4], 4, HASH))
            .await?;
        let stream = delta_stream(0, 20_000, &[]);
        let small = ref_delta(HASH, stream.len() as u64, &stream);
        let mixed = [vec![big], vec![small; 4]].concat();
        lab.refuses(
            "one blob and four deltas",
            assemble(&header, &mixed, 5, HASH),
            &over,
        )
        .await?;
        lab.passes_the_walker("deltas at the limit", |header| {
            assemble(header, &mixed[1..], 4, HASH)
        })
        .await
    })
    .await
}
