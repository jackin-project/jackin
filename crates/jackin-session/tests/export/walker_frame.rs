// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::BundleRefusal;
use jackin_testkit::{TestResult, guard};

use super::lab::{Check, Lab, family, refusal};
use super::pack::{Hash, entries, header_len, resign};

#[tokio::test]
async fn a_pack_with_bad_framing_is_refused_before_any_host_import() -> TestResult {
    guard(async {
        let lab = Lab::armed().await?;
        let honest = lab.honest().await?;
        let hash = Hash::Sha1;
        let start = header_len(&honest);
        let first = entries(&honest, hash)?[0].start;
        let len = honest.len();
        let edit = |change: &dyn Fn(&mut Vec<u8>)| {
            let mut bytes = honest.clone();
            change(&mut bytes);
            resign(&mut bytes, hash);
            bytes
        };
        let cases: Vec<(&str, Vec<u8>, Check)> = vec![
            (
                "one trailing byte",
                [honest.clone(), vec![0]].concat(),
                refusal(BundleRefusal::TrailingBytes),
            ),
            (
                "20 trailing bytes",
                [honest.clone(), vec![7; 20]].concat(),
                refusal(BundleRefusal::TrailingBytes),
            ),
            (
                "cut inside the header",
                honest[..5].to_vec(),
                refusal(BundleRefusal::Header),
            ),
            (
                "cut at the PACK magic",
                honest[..start + 2].to_vec(),
                family(),
            ),
            (
                "cut mid-entry",
                honest[..first + 3].to_vec(),
                refusal(BundleRefusal::Truncated),
            ),
            (
                "one byte short",
                honest[..len - 1].to_vec(),
                refusal(BundleRefusal::Truncated),
            ),
            (
                "trailer cut",
                honest[..len - 20].to_vec(),
                refusal(BundleRefusal::Truncated),
            ),
            ("empty file", Vec::new(), refusal(BundleRefusal::Header)),
            (
                "last byte flipped",
                edit_raw(&honest, |bytes| bytes[len - 1] ^= 0xff),
                refusal(BundleRefusal::Trailer),
            ),
            (
                "magic changed",
                edit(&|bytes| bytes[start] = b'X'),
                refusal(BundleRefusal::PackMagic),
            ),
            (
                "version 3",
                edit(&|bytes| bytes[start + 7] = 3),
                refusal(BundleRefusal::PackVersion(3)),
            ),
            (
                "count too high",
                edit(&|bytes| bytes[start + 11] += 1),
                family(),
            ),
            (
                "count too low",
                edit(&|bytes| bytes[start + 11] -= 1),
                family(),
            ),
            (
                "entry type 0",
                edit(&|bytes| bytes[first] &= 0x8f),
                refusal(BundleRefusal::EntryType(0)),
            ),
            (
                "entry type 5",
                edit(&|bytes| bytes[first] = (bytes[first] & 0x8f) | 0x50),
                refusal(BundleRefusal::EntryType(5)),
            ),
        ];
        for (label, bytes, wanted) in &cases {
            lab.refuses(label, bytes.clone(), wanted).await?;
        }
        Ok(())
    })
    .await
}

fn edit_raw(honest: &[u8], change: impl Fn(&mut Vec<u8>)) -> Vec<u8> {
    let mut bytes = honest.to_vec();
    change(&mut bytes);
    bytes
}
