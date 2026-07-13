// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::HashSet;

#[test]
fn wordlist_has_enough_entries() {
    assert!(
        WORDLIST.len() >= 100,
        "wordlist has {} entries, need ≥100",
        WORDLIST.len()
    );
}

#[test]
fn wordlist_entries_are_unique() {
    let mut seen = HashSet::new();
    for word in WORDLIST {
        assert!(seen.insert(*word), "duplicate wordlist entry: {word}");
    }
}

#[test]
fn pick_codename_avoids_live_and_retired() {
    let live: HashSet<String> = ["badger".into()].into();
    let retired: HashSet<String> = ["crane".into()].into();
    let name = pick_codename(&live, &retired, 0);
    assert_ne!(name, "badger");
    assert_ne!(name, "crane");
}

#[test]
fn pick_codename_fallback_when_pool_exhausted() {
    let live: HashSet<String> = HashSet::new();
    let retired: HashSet<String> = WORDLIST.iter().map(ToString::to_string).collect();
    let name = pick_codename(&live, &retired, 0);
    assert!(
        name.contains('-'),
        "fallback name should contain '-': {name}"
    );
}
