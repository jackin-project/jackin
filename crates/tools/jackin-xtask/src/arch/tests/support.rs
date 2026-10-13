// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn members(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

pub(super) fn edges(pairs: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
    pairs
        .iter()
        .map(|(from, tos)| {
            (
                (*from).to_owned(),
                tos.iter().map(|t| (*t).to_owned()).collect(),
            )
        })
        .collect()
}

pub(super) fn tiers<'a>(pairs: &'a [(&'a str, u8)]) -> BTreeMap<&'a str, u8> {
    pairs.iter().copied().collect()
}
