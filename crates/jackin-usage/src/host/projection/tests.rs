// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn sorted(locale: &str, labels: &[&str]) -> Vec<String> {
    let locale = locale.parse::<Locale>().expect("test locale");
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Secondary);
    let collator = Collator::try_new(locale.into(), options).expect("test collator");
    let mut labels = labels.iter().map(ToString::to_string).collect::<Vec<_>>();
    labels.sort_by(|left, right| collator.compare(left, right));
    labels
}

#[test]
fn canonical_projection_icu_collation_goldens_are_pinned() {
    assert_eq!(
        sorted("und", &["Zulu", "Änne", "Ana", "Åke"]),
        ["Åke", "Ana", "Änne", "Zulu"]
    );
    assert_eq!(
        sorted("en", &["Zulu", "Änne", "Ana", "Åke"]),
        ["Åke", "Ana", "Änne", "Zulu"]
    );
    assert_eq!(
        sorted("tr", &["Jale", "İpek", "Işık", "Hale"]),
        ["Hale", "Işık", "İpek", "Jale"]
    );
    assert_eq!(
        sorted("vi", &["Bình", "Ân", "Ăn", "An"]),
        ["An", "Ăn", "Ân", "Bình"]
    );
}
