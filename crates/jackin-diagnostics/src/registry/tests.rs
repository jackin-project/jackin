// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Fail-closed registry tests.

use super::{
    EVENT_DEFS, Outcome, RegistryError, is_prohibited_key, lookup, validate, validate_outcome,
};

#[test]
fn unknown_event_name_is_rejected() {
    let err = validate("not.a.real.event", &[], "body").unwrap_err();
    assert!(matches!(err, RegistryError::UnknownEvent(_)));
}

#[test]
fn unknown_attr_key_is_rejected() {
    let err = validate("process.execute", &[("totally.unknown", "x")], "body").unwrap_err();
    assert!(matches!(err, RegistryError::UnknownAttr { .. }));
}

#[test]
fn missing_required_attr_is_rejected() {
    // error.typed requires error.type
    let err = validate("error.typed", &[], "typed error").unwrap_err();
    assert!(matches!(err, RegistryError::MissingRequired { key, .. } if key == "error.type"));
}

#[test]
fn prohibited_keys_are_rejected() {
    for key in ["error_type", "log.category", "kind", "stage", "run_id"] {
        assert!(is_prohibited_key(key), "{key} should be prohibited");
        let err = validate("process.execute", &[(key, "x")], "body").unwrap_err();
        assert!(
            matches!(err, RegistryError::ProhibitedKey(ref k) if k == key),
            "expected ProhibitedKey for {key}, got {err:?}"
        );
    }
}

#[test]
fn bracket_prefixed_body_is_rejected() {
    let err = validate("process.execute", &[], "[jackin debug docker] secret").unwrap_err();
    assert!(matches!(err, RegistryError::BodyPolicy(_)));
}

#[test]
fn every_seeded_def_is_well_formed() {
    assert!(!EVENT_DEFS.is_empty());
    for def in EVENT_DEFS {
        assert!(!def.name.is_empty(), "empty name");
        assert!(
            def.name.contains('.'),
            "event name must be dotted: {}",
            def.name
        );
        assert!(
            !def.name.contains('_'),
            "event name must not contain underscore: {}",
            def.name
        );
        assert!(
            !def.outcomes.is_empty(),
            "event {} needs at least one outcome",
            def.name
        );
        assert!(!def.owner.is_empty(), "event {} needs owner", def.name);
        assert!(!def.body.is_empty(), "event {} needs body intent", def.name);
        // expected_shutdown must never appear
        for outcome in def.outcomes {
            assert_ne!(
                outcome.as_str(),
                "expected_shutdown",
                "prohibited outcome on {}",
                def.name
            );
        }
    }
}

#[test]
fn expected_shutdown_is_not_an_allowed_outcome_anywhere() {
    for def in EVENT_DEFS {
        assert!(
            !def.outcomes
                .iter()
                .any(|o| o.as_str() == "expected_shutdown"),
            "{}",
            def.name
        );
    }
    assert!(Outcome::parse("expected_shutdown").is_none());
    assert_eq!(
        Outcome::parse("expected_close"),
        Some(Outcome::ExpectedClose)
    );
}

#[test]
fn lookup_by_kind_and_name() {
    let by_kind = lookup("session_detach").expect("kind");
    let by_name = lookup("capsule.session.detach").expect("name");
    assert_eq!(by_kind.name, by_name.name);
    assert_eq!(by_kind.outcomes, &[Outcome::ExpectedClose]);
}

#[test]
fn process_execute_accepts_optional_attrs() {
    let def = validate(
        "process.execute",
        &[("process.command", "echo"), ("process.args_redacted", "hi")],
        "host process execute",
    )
    .expect("valid");
    assert_eq!(def.name, "process.execute");
    validate_outcome(def, Outcome::Success).expect("success allowed");
    validate_outcome(def, Outcome::ExpectedClose).expect_err("expected_close not allowed");
}

#[test]
fn stage_failed_defaults_to_failure_outcome() {
    let def = lookup("stage_failed").expect("registered");
    assert_eq!(def.name, "launch.stage.failed");
    assert!(def.outcomes.contains(&Outcome::Failure));
}
