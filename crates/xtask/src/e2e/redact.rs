// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::Secret;

const NEEDLE_MIN: usize = 16;

#[derive(Clone, Debug)]
pub struct Redactor {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    needles: Vec<Secret>,
    masker: jackin_core::Redactor,
}

impl Redactor {
    #[must_use]
    pub fn new<'a>(values: impl IntoIterator<Item = &'a str>) -> Redactor {
        let mut texts: Vec<String> = Vec::new();
        for value in values {
            if value.len() >= NEEDLE_MIN {
                texts.push(value.to_owned());
            }
            if value.matches('.').count() == 2 {
                texts.extend(
                    value
                        .split('.')
                        .filter(|segment| segment.len() >= NEEDLE_MIN)
                        .map(str::to_owned),
                );
            }
        }
        texts.sort();
        texts.dedup();
        let needles: Vec<Secret> = texts.into_iter().map(Secret::new).collect();
        let masker = jackin_core::Redactor::new(needles.iter());
        Redactor {
            inner: Arc::new(Inner { needles, masker }),
        }
    }

    #[must_use]
    pub fn none() -> Redactor {
        Redactor::new(std::iter::empty())
    }

    #[must_use]
    pub fn scrub(&self, text: &str) -> String {
        self.inner.masker.apply(text)
    }

    #[must_use]
    pub fn scan(&self, bytes: &[u8]) -> Option<usize> {
        self.inner
            .needles
            .iter()
            .filter_map(|needle| find(bytes, needle.expose().as_bytes()))
            .min()
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let first = needle[0];
    let mut from = 0;
    while let Some(offset) = haystack[from..=haystack.len() - needle.len()]
        .iter()
        .position(|byte| *byte == first)
    {
        let start = from + offset;
        if &haystack[start..start + needle.len()] == needle {
            return Some(start);
        }
        from = start + 1;
        if from + needle.len() > haystack.len() {
            break;
        }
    }
    None
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    out
}
