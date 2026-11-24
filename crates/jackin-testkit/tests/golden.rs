// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{TestResult, assert_golden};
use uuid::Uuid;

#[test]
fn a_golden_hides_generated_ids_and_names_the_first_differing_line() -> TestResult {
    let dir = tempfile::tempdir()?;
    let golden = dir.path().join("sample.txt");
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    std::fs::write(
        &golden,
        "id a0000001-0000-4000-8000-000000000001 short a0000001\nnext a0000002-0000-4000-8000-000000000002 #a0000002\nend\n",
    )?;
    let path = golden.to_str().ok_or("golden path")?;
    let actual = |tail: &str| {
        format!(
            "id {first} short {}\nnext {second} #{}\n{tail}\n",
            &first.to_string()[..8],
            &second.to_string()[..8]
        )
    };
    assert_golden(path, &actual("end"), &[first, second])?;

    let error = assert_golden(path, &actual("done"), &[first, second])
        .err()
        .ok_or("a different text must fail")?
        .to_string();
    assert!(error.contains("line 3"), "{error}");
    assert!(
        error.contains("\"end\"") && error.contains("\"done\""),
        "{error}"
    );

    let unseeded = assert_golden(path, &actual("end"), &[first])
        .err()
        .ok_or("an unredacted id must fail")?
        .to_string();
    assert!(unseeded.contains("line 2"), "{unseeded}");

    let missing = dir.path().join("absent.txt");
    let error = assert_golden(missing.to_str().ok_or("path")?, "x\n", &[])
        .err()
        .ok_or("a missing golden must fail")?
        .to_string();
    assert!(error.contains("JACKIN_BLESS=1"), "{error}");
    Ok(())
}
