// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{BundleRefusal, ObjectFormat};
use jackin_testkit::{RepoFixture, Rig, TestResult, guard};

use super::heads::crafted;
use super::lab::{Check, Lab, family, heads, refusal};
use super::pack::with_header;

fn header(lines: &[String]) -> Vec<u8> {
    let mut text = lines.join("\n");
    text.push_str("\n\n");
    text.into_bytes()
}

fn sig() -> String {
    "# v2 git bundle".to_owned()
}

fn head(tip: &str, name: &str) -> String {
    format!("{tip} {name}")
}

#[tokio::test]
async fn a_malformed_header_line_is_refused_before_any_host_import() -> TestResult {
    guard(async {
        let lab = Lab::armed().await?;
        let tip = lab.tip.as_str().to_owned();
        let work = head(&tip, "refs/heads/work");
        let honest = lab.honest().await?;
        let cases: Vec<(&str, Vec<String>, Check)> = vec![
            (
                "v1 signature",
                vec!["# v1 git bundle".into(), work.clone()],
                refusal(BundleRefusal::Header),
            ),
            (
                "v4 signature",
                vec!["# v4 git bundle".into(), work.clone()],
                refusal(BundleRefusal::Header),
            ),
            (
                "empty signature",
                vec![String::new(), work.clone()],
                refusal(BundleRefusal::Header),
            ),
            (
                "39 hex digits",
                vec![sig(), head(&tip[1..], "refs/heads/work")],
                refusal(BundleRefusal::Header),
            ),
            (
                "non-hex id",
                vec![sig(), head(&format!("z{}", &tip[1..]), "refs/heads/work")],
                refusal(BundleRefusal::Header),
            ),
            (
                "no space",
                vec![sig(), format!("{tip}refs/heads/work")],
                refusal(BundleRefusal::Header),
            ),
            (
                "empty name",
                vec![sig(), format!("{tip} ")],
                refusal(BundleRefusal::Header),
            ),
            (
                "duplicated head",
                vec![sig(), work.clone(), work.clone()],
                refusal(BundleRefusal::Header),
            ),
            (
                "filter capability",
                vec![
                    "# v3 git bundle".into(),
                    "@filter=blob:none".into(),
                    work.clone(),
                ],
                refusal(BundleRefusal::Capability),
            ),
            (
                "unknown capability",
                vec![
                    "# v3 git bundle".into(),
                    "@frobnicate=1".into(),
                    work.clone(),
                ],
                refusal(BundleRefusal::Capability),
            ),
            (
                "two capabilities",
                vec![
                    "# v3 git bundle".into(),
                    "@object-format=sha1".into(),
                    "@object-format=sha1".into(),
                    work.clone(),
                ],
                refusal(BundleRefusal::Capability),
            ),
            (
                "a prerequisite",
                vec![sig(), format!("-{tip} commit"), work.clone()],
                refusal(BundleRefusal::Prerequisite),
            ),
            (
                "sha256 on a sha1 host",
                vec![
                    "# v3 git bundle".into(),
                    "@object-format=sha256".into(),
                    work.clone(),
                ],
                refusal(BundleRefusal::ObjectFormat {
                    found: ObjectFormat::Sha256,
                    expected: ObjectFormat::Sha1,
                }),
            ),
            (
                "an extra head",
                vec![sig(), work.clone(), head(&tip, "refs/heads/extra")],
                heads(),
            ),
            ("a missing head", vec![sig()], heads()),
        ];
        for (label, lines, wanted) in &cases {
            lab.refuses(label, with_header(&honest, &header(lines)), wanted)
                .await?;
        }
        let mut cut = header(&[sig(), work.clone()]);
        cut.truncate(cut.len() - 1);
        lab.refuses("no blank line", cut, &family()).await?;
        lab.refuses(
            "no newline",
            b"# v2 git bundle".to_vec(),
            &refusal(BundleRefusal::Header),
        )
        .await?;
        lab.refuses("an empty file", Vec::new(), &refusal(BundleRefusal::Header))
            .await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_bundle_with_a_prerequisite_from_real_git_is_refused() -> TestResult {
    guard(async {
        let lab = Lab::armed().await?;
        let bundle = crafted(&lab.handle, &["refs/heads/work", "^refs/heads/main"]).await?;
        assert!(bundle.windows(2).any(|pair| pair == b"\n-"));
        lab.refuses(
            "real prerequisite",
            bundle,
            &refusal(BundleRefusal::Prerequisite),
        )
        .await
    })
    .await
}

fn padded(tip: &str, total: usize) -> Vec<u8> {
    let fixed = header(&[
        sig(),
        head(tip, "refs/heads/work"),
        head(tip, "refs/heads/"),
    ])
    .len();
    let name = "x".repeat(total - fixed);
    header(&[
        sig(),
        head(tip, "refs/heads/work"),
        head(tip, &format!("refs/heads/{name}")),
    ])
}

#[tokio::test]
async fn the_header_is_bounded_at_1_mib_and_1024_heads_before_the_head_set_is_compared()
-> TestResult {
    guard(async {
        let lab = Lab::armed().await?;
        let tip = lab.tip.as_str().to_owned();
        let honest = lab.honest().await?;
        let exact = padded(&tip, 1_048_576);
        assert_eq!(exact.len(), 1_048_576);
        lab.refuses("1 MiB header", with_header(&honest, &exact), &heads())
            .await?;
        let over = padded(&tip, 1_048_577);
        lab.refuses(
            "1 MiB + 1 header",
            with_header(&honest, &over),
            &refusal(BundleRefusal::Header),
        )
        .await?;
        for (label, count, wanted) in [
            ("1024 heads", 1023, heads()),
            ("1025 heads", 1024, refusal(BundleRefusal::Header)),
        ] {
            let mut lines = vec![sig(), head(&tip, "refs/heads/work")];
            lines.extend((0..count).map(|n| head(&tip, &format!("refs/heads/e{n:04}"))));
            lab.refuses(label, with_header(&honest, &header(&lines)), &wanted)
                .await?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_v2_header_against_a_sha256_host_and_the_reverse_name_both_formats() -> TestResult {
    guard(async {
        let repo = RepoFixture::new_sha256().await?;
        repo.file("src/main.rs", b"fn main() {}\n").await?;
        repo.commit().await?;
        let lab = Lab::armed_over(Rig::builder().repo(repo).build().await?).await?;
        let tip = lab.tip.as_str().to_owned();
        let honest = lab.honest().await?;
        let sha1_id = &tip[..40];
        let v2 = header(&[sig(), head(sha1_id, "refs/heads/work")]);
        let wanted = refusal(BundleRefusal::ObjectFormat {
            found: ObjectFormat::Sha1,
            expected: ObjectFormat::Sha256,
        });
        lab.refuses("v2 header on sha256", with_header(&honest, &v2), &wanted)
            .await?;
        let v3 = header(&[
            "# v3 git bundle".into(),
            "@object-format=sha1".into(),
            head(sha1_id, "refs/heads/work"),
        ]);
        lab.refuses(
            "explicit sha1 on sha256",
            with_header(&honest, &v3),
            &wanted,
        )
        .await?;
        Ok(())
    })
    .await
}
