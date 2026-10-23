// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

// Official 1.0.0/1.2.2 ManagedContainer field shapes; IDs and extra metadata are synthetic.
const APPLE_1_0_LISTING: &[u8] = br#"[
    {"id":"jk-10-running","configuration":{"id":"jk-10-running","hostname":"runner"},"status":{"state":"running","detail":"extra metadata"},"metadata":{"owner":"fixture"}},
    {"id":"jk-10-stopped","configuration":{"id":"jk-10-stopped"},"status":{"state":"stopped"}}
]"#;

const APPLE_1_2_LISTING: &[u8] = br#"[
    {"id":"jk-12-unknown","configuration":{"id":"jk-12-unknown","extra":{"accepted":true}},"status":{"state":"unknown","timestamp":"fixture"}},
    {"id":"jk-12-stopping","configuration":{"id":"jk-12-stopping"},"status":{"state":"stopping","progress":0.5}}
]"#;

#[test]
fn versioned_official_listing_fixtures_keep_typed_states_and_additive_metadata() {
    let apple_10 = parse_all_containers_json(APPLE_1_0_LISTING).unwrap();
    assert_eq!(apple_10.len(), 2);
    assert_eq!(apple_10[0].id, "jk-10-running");
    assert_eq!(apple_10[0].status, AppleContainerStatus::Running);
    assert!(apple_10[0].is_running());
    assert_eq!(apple_10[1].status, AppleContainerStatus::Stopped);
    assert!(!apple_10[1].is_running());

    let apple_12 = parse_all_containers_json(APPLE_1_2_LISTING).unwrap();
    assert_eq!(apple_12.len(), 2);
    assert_eq!(apple_12[0].status, AppleContainerStatus::Unknown);
    assert_eq!(apple_12[1].status, AppleContainerStatus::Stopping);
    assert!(apple_12.iter().all(|container| !container.is_running()));
}

#[test]
fn empty_array_is_the_only_valid_empty_inventory() {
    assert!(parse_all_containers_json(b"[]").unwrap().is_empty());
    let invalid_empty_or_partial: [&[u8]; 8] = [
        b"".as_slice(),
        b"   ",
        b"[",
        b"[] trailing",
        b"not json",
        br"[{}]",
        br"[null]",
        br#"[{"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"running"}},null]"#,
    ];
    for invalid_empty_or_partial in invalid_empty_or_partial {
        assert!(
            parse_all_containers_json(invalid_empty_or_partial).is_err(),
            "invalid inventory bytes must not prove absence"
        );
    }
}

#[test]
fn only_one_complete_json_array_is_accepted() {
    let invalid_shapes: [&[u8]; 4] = [
        br#"{"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"running"}}"#.as_slice(),
        br#"{"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"running"}}
{"id":"jk-b","configuration":{"id":"jk-b"},"status":{"state":"stopped"}}"#,
        br"listing failed
[]",
        br#"[["jk-a","running"]]"#,
    ];
    for invalid_shape in invalid_shapes {
        parse_all_containers_json(invalid_shape).unwrap_err();
    }
}

#[test]
fn non_utf8_inventory_is_rejected_without_lossy_replacement() {
    let invalid_utf8 = [b'[', 0xff, b']'];
    let error = parse_all_containers_json(&invalid_utf8).unwrap_err();
    assert!(error.to_string().contains("not valid UTF-8"));
}

#[test]
fn all_required_shapes_are_maps_and_duplicate_required_fields_fail() {
    let invalid_rows: [&[u8]; 6] = [
        br#"[["jk-a",{"id":"jk-a"},{"state":"running"}]]"#.as_slice(),
        br#"[{"id":"jk-a","id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"running"}}]"#,
        br#"[{"id":"jk-a","configuration":["jk-a"],"status":{"state":"running"}}]"#,
        br#"[{"id":"jk-a","configuration":{"id":"jk-a","id":"jk-a"},"status":{"state":"running"}}]"#,
        br#"[{"id":"jk-a","configuration":{"id":"jk-a"},"status":["running"]}]"#,
        br#"[{"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"running","state":"stopped"}}]"#,
    ];
    for invalid in invalid_rows {
        parse_all_containers_json(invalid).unwrap_err();
    }
}

#[test]
fn required_identity_and_state_fields_must_be_present_nonempty_and_consistent() {
    let invalid_rows: [&[u8]; 7] = [
        br#"[{"configuration":{"id":"jk-a"},"status":{"state":"running"}}]"#.as_slice(),
        br#"[{"id":"jk-a","status":{"state":"running"}}]"#,
        br#"[{"id":"jk-a","configuration":{"id":"jk-a"},"status":{}}]"#,
        br#"[{"id":"","configuration":{"id":""},"status":{"state":"running"}}]"#,
        br#"[{"id":"jk-a","configuration":{"id":"jk-other"},"status":{"state":"running"}}]"#,
        br#"[{"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":4}}]"#,
        br#"[{"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"not-running"}}]"#,
    ];
    for invalid in invalid_rows {
        parse_all_containers_json(invalid).unwrap_err();
    }

    let duplicate_ids = br#"[
        {"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"running"}},
        {"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"stopped"}}
    ]"#;
    parse_all_containers_json(duplicate_ids).unwrap_err();
}

#[test]
fn all_known_runtime_states_are_exact_enum_values() {
    let states = [
        ("running", AppleContainerStatus::Running),
        ("stopped", AppleContainerStatus::Stopped),
        ("unknown", AppleContainerStatus::Unknown),
        ("stopping", AppleContainerStatus::Stopping),
    ];
    for (state, expected) in states {
        let json = format!(
            r#"[{{"id":"jk-a","configuration":{{"id":"jk-a"}},"status":{{"state":"{state}"}}}}]"#
        );
        assert_eq!(
            parse_all_containers_json(json.as_bytes()).unwrap()[0].status,
            expected
        );
    }
}

#[test]
fn malformed_json_diagnostics_do_not_echo_untrusted_inventory_strings() {
    const CANARY: &str = "inventory-secret-canary-7bba";
    let invalid_shapes = [
        format!(r#"["{CANARY}"]"#),
        format!(r#"[{{"id":"jk-a","configuration":"{CANARY}","status":{{"state":"running"}}}}]"#),
        format!(r#"[{{"id":"jk-a","configuration":{{"id":"jk-a"}},"status":"{CANARY}"}}]"#),
        format!(
            r#"[{{"id":"jk-a","configuration":{{"id":"jk-a"}},"status":{{"state":"{CANARY}"}}}}]"#
        ),
    ];

    for invalid in invalid_shapes {
        let error = parse_all_containers_json(invalid.as_bytes()).unwrap_err();
        let display = format!("{error}");
        let alternate = format!("{error:#}");
        let debug = format!("{error:?}");
        assert!(!display.contains(CANARY), "Display leaked input: {display}");
        assert!(
            !alternate.contains(CANARY),
            "alternate Display leaked input: {alternate}"
        );
        assert!(!debug.contains(CANARY), "Debug leaked input: {debug}");
    }
}
