// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use uuid::Uuid;

use crate::rig::TestResult;

const BLESS: &str = "JACKIN_BLESS";

fn fake_id(position: usize) -> String {
    let number = position + 1;
    format!("a{number:07}-0000-4000-8000-{number:012}")
}

fn redact(actual: &str, ids: &[Uuid]) -> String {
    let pairs: Vec<(String, String)> = ids
        .iter()
        .enumerate()
        .map(|(position, id)| (id.hyphenated().to_string(), fake_id(position)))
        .collect();
    let mut text = actual.to_owned();
    for (real, fake) in &pairs {
        text = text.replace(real, fake);
    }
    for (real, fake) in &pairs {
        text = text.replace(&real[..8], &fake[..8]);
    }
    text
}

fn first_difference(expected: &str, actual: &str) -> String {
    let mut wanted = expected.split('\n');
    let mut found = actual.split('\n');
    let mut line = 1_usize;
    loop {
        match (wanted.next(), found.next()) {
            (Some(left), Some(right)) if left == right => line += 1,
            (None, None) => return "the texts are equal".to_owned(),
            (left, right) => {
                return format!(
                    "line {line}\n  expected: {:?}\n  actual:   {:?}",
                    left.unwrap_or("<end of file>"),
                    right.unwrap_or("<end of file>")
                );
            }
        }
    }
}

pub fn assert_golden(relative: &str, actual: &str, ids: &[Uuid]) -> TestResult {
    let root = std::env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?;
    let path = PathBuf::from(root).join(relative);
    let actual = redact(actual, ids);
    if std::env::var_os(BLESS).is_some_and(|value| value == "1") {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, actual)?;
        return Ok(());
    }
    let expected = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "golden {} cannot be read ({error}); write it with {BLESS}=1",
            path.display()
        )
    })?;
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "golden {} differs at {}",
            path.display(),
            first_difference(&expected, &actual)
        )
        .into())
    }
}
