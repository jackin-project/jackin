// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn prewarm_metrics_use_only_bounded_job_and_outcome_dimensions() {
    let job = Attr {
        key: attrs::JOB_TYPE,
        value: Value::Str(schema::enums::JobType::ImagePrewarm.as_str()),
    };
    let outcome = Attr {
        key: attrs::OUTCOME,
        value: Value::Str(schema::enums::OutcomeValue::Failure.as_str()),
    };
    let error = Attr {
        key: attrs::std_attrs::ERROR_TYPE,
        value: Value::Str(schema::enums::ErrorType::LaunchFailed.as_str()),
    };

    validate_attributes(&PREWARM_JOBS, &[job]).unwrap();
    validate_attributes(&PREWARM_ACTIVE, &[job]).unwrap();
    validate_attributes(&PREWARM_DURATION, &[job, outcome, error]).unwrap();
    assert_eq!(
        validate_attributes(&PREWARM_DURATION, &[job]),
        Err(Rejection::InvalidValue)
    );
}

#[test]
fn standard_token_usage_requires_only_bounded_semantic_dimensions() {
    let dimensions = GEN_AI_CLIENT_TOKEN_USAGE.dimensions();
    assert_eq!(
        dimensions
            .iter()
            .map(|requirement| requirement.name)
            .collect::<Vec<_>>(),
        [
            attrs::GEN_AI_OPERATION_NAME,
            attrs::GEN_AI_PROVIDER_NAME,
            attrs::GEN_AI_TOKEN_TYPE,
        ]
    );
    assert!(
        dimensions
            .iter()
            .all(|requirement| requirement.requirement == schema::RequirementLevel::Required)
    );
    assert_eq!(GEN_AI_CLIENT_TOKEN_USAGE.unit(), "{token}");
    assert_eq!(
        GEN_AI_CLIENT_TOKEN_USAGE.boundaries(),
        [
            1.0, 4.0, 16.0, 64.0, 256.0, 1024.0, 4096.0, 16384.0, 65536.0, 262144.0
        ]
    );
}

#[test]
fn correlation_identities_are_never_metric_dimensions() {
    for key in [
        attrs::CLI_INVOCATION_ID,
        attrs::std_attrs::SESSION_ID,
        attrs::JOB_ID,
        attrs::UI_SCREEN_VISIT_ID,
        attrs::std_attrs::GEN_AI_CONVERSATION_ID,
    ] {
        assert_eq!(
            counter(&TELEMETRY_VALIDATE).add(
                1,
                &[Attr {
                    key,
                    value: Value::Str("opaque-correlation"),
                }],
            ),
            Err(Rejection::Cardinality),
            "identity key {key} must fail before disabled-meter short circuit"
        );
    }
}

#[test]
fn agent_state_metrics_require_the_governed_dimensions() {
    let attrs = [
        Attr {
            key: attrs::std_attrs::GEN_AI_AGENT_NAME,
            value: Value::Str("codex"),
        },
        Attr {
            key: attrs::AGENT_STATE,
            value: Value::Str("working"),
        },
        Attr {
            key: attrs::AGENT_STATUS_SOURCE,
            value: Value::Str("shell_integration"),
        },
        Attr {
            key: attrs::AGENT_STATUS_CONFIDENCE,
            value: Value::Str("strong"),
        },
    ];
    assert_eq!(
        validate_attributes(&AGENT_STATE_TRANSITIONS, &attrs),
        Ok(())
    );
    assert_eq!(
        validate_attributes(&AGENT_STATE_STUCK, &attrs[..3]),
        Err(Rejection::InvalidValue)
    );

    let mut unknown_agent = attrs;
    unknown_agent[0].value = Value::Str("unknown-agent");
    assert_eq!(
        validate_attributes(&AGENT_STATE_FLAPS, &unknown_agent),
        Err(Rejection::InvalidValue)
    );
}
