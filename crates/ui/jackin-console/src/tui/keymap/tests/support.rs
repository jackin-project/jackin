// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn assert_bridged_matches_direct<A>(
    name: &str,
    map: &TermrockKeymap<A>,
    extra_chords: &[KeyChord],
) where
    A: Clone + Copy + PartialEq + std::fmt::Debug + 'static,
{
    let chords: Vec<KeyChord> = map
        .bindings()
        .iter()
        .flat_map(|binding| binding.chords().iter().copied())
        .chain(extra_chords.iter().copied())
        .collect();
    for chord in chords {
        let event = KeyEvent::new(chord.key, chord.mods);
        assert_eq!(
            bridged_keymap_action(map, event),
            map.dispatch(chord),
            "{name}: bridged dispatch must match direct dispatch for {chord:?}"
        );
    }
}

pub(super) fn hint_keys(spans: Vec<termrock::widgets::HintSpan<'static>>) -> Vec<String> {
    spans
        .iter()
        .filter_map(|span| match span {
            termrock::widgets::HintSpan::Key(key) => Some((*key).to_owned()),
            termrock::widgets::HintSpan::DynKey(key) => Some(key.clone()),
            _ => None,
        })
        .collect()
}
