// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parses_every_canonical_spelling_case_insensitively() {
    for effort in ReasoningEffort::ALL {
        assert_eq!(effort.as_str().parse::<ReasoningEffort>(), Ok(effort));
        assert_eq!(
            effort.as_str().to_uppercase().parse::<ReasoningEffort>(),
            Ok(effort)
        );
    }
}

#[test]
fn rejects_an_unknown_spelling_and_names_it() {
    let error = "maximum".parse::<ReasoningEffort>().unwrap_err();
    assert_eq!(error.input, "maximum");
    assert!(error.to_string().contains("maximum"));
}

#[test]
fn default_is_medium() {
    assert_eq!(ReasoningEffort::default(), ReasoningEffort::Medium);
}
