// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Agent-status and provider-probe telemetry recording.

use crate::session::Session;

fn emit_agent_state_change(
    session: &Session,
    transition: &crate::session::StatusTransition,
    stuck: bool,
    flap: bool,
) {
    use jackin_telemetry::{Attr, FieldSet, Value};

    let Some(metric_attrs) = agent_status_metric_attrs(session, transition.effective) else {
        return;
    };
    let event_attrs = [
        metric_attrs[0],
        metric_attrs[1],
        metric_attrs[2],
        metric_attrs[3],
        Attr {
            key: jackin_telemetry::schema::attrs::AGENT_STATUS_STUCK,
            value: Value::Bool(stuck),
        },
    ];
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::AGENT_STATE_CHANGED,
        FieldSet::new(&event_attrs, None),
    );
    let _transition_result =
        jackin_telemetry::counter(&jackin_telemetry::metric::AGENT_STATE_TRANSITIONS)
            .add(1, &metric_attrs);
    if stuck {
        record_agent_stuck(&metric_attrs);
    }
    if flap {
        let _flap_result = jackin_telemetry::counter(&jackin_telemetry::metric::AGENT_STATE_FLAPS)
            .add(1, &metric_attrs);
    }
}

fn agent_status_metric_attrs(
    session: &Session,
    state: crate::protocol::AgentState,
) -> Option<[jackin_telemetry::Attr<'_>; 4]> {
    use jackin_telemetry::{Attr, Value};

    let agent = session.agent.as_deref()?;
    let source = match session.status.report(None).source {
        jackin_protocol::agent_status::AgentStatusSource::None => "none",
        jackin_protocol::agent_status::AgentStatusSource::VisibleScreen => "visible_screen",
        jackin_protocol::agent_status::AgentStatusSource::ShellIntegration => "shell_integration",
        jackin_protocol::agent_status::AgentStatusSource::ForegroundProcess => "foreground_process",
        jackin_protocol::agent_status::AgentStatusSource::Reported { .. } => "reported",
    };
    let confidence = match session.status.confidence {
        jackin_protocol::agent_status::AgentStatusConfidence::Unknown => "unknown",
        jackin_protocol::agent_status::AgentStatusConfidence::Weak => "weak",
        jackin_protocol::agent_status::AgentStatusConfidence::Strong => "strong",
        jackin_protocol::agent_status::AgentStatusConfidence::Authoritative => "authoritative",
    };
    Some([
        Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent),
        },
        Attr {
            key: jackin_telemetry::schema::attrs::AGENT_STATE,
            value: Value::Str(state.label()),
        },
        Attr {
            key: jackin_telemetry::schema::attrs::AGENT_STATUS_SOURCE,
            value: Value::Str(source),
        },
        Attr {
            key: jackin_telemetry::schema::attrs::AGENT_STATUS_CONFIDENCE,
            value: Value::Str(confidence),
        },
    ])
}

fn record_agent_stuck(attrs: &[jackin_telemetry::Attr<'_>]) {
    let _stuck_result =
        jackin_telemetry::counter(&jackin_telemetry::metric::AGENT_STATE_STUCK).add(1, attrs);
}

fn agent_status_cycle_attrs() -> [jackin_telemetry::Attr<'static>; 1] {
    [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::BACKGROUND_CYCLE_NAME,
        value: jackin_telemetry::Value::Str(
            jackin_telemetry::schema::enums::BackgroundCycleName::AgentStatus.as_str(),
        ),
    }]
}

fn record_skipped_agent_status() {
    let attrs = [
        agent_status_cycle_attrs()[0],
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::OutcomeValue::Skip.as_str(),
            ),
        },
    ];
    let _metric =
        jackin_telemetry::counter(&jackin_telemetry::metric::BACKGROUND_CYCLES).add(1, &attrs);
}

pub(crate) fn record_agent_status_tick(session: &Session, tick: crate::session::StatusTick) {
    if tick.transition.is_none() && !tick.stuck && !tick.flap {
        record_skipped_agent_status();
        return;
    }
    let cycle = jackin_telemetry::autonomous_cycle_operation(
        jackin_telemetry::schema::enums::BackgroundCycleName::AgentStatus,
    )
    .ok();
    let record_result = || {
        if let Some(transition) = tick.transition {
            emit_agent_state_change(session, &transition, tick.stuck, tick.flap);
        } else if let Some(attrs) = agent_status_metric_attrs(session, session.state) {
            record_agent_stuck(&attrs);
        }
    };
    if let Some(cycle) = cycle {
        cycle.span().in_scope(record_result);
        cycle.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    } else {
        record_result();
    }
}

fn provider_probe_attrs() -> [jackin_telemetry::Attr<'static>; 1] {
    [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::BACKGROUND_CYCLE_NAME,
        value: jackin_telemetry::Value::Str(
            jackin_telemetry::schema::enums::BackgroundCycleName::ProviderProbe.as_str(),
        ),
    }]
}

pub(crate) fn record_skipped_provider_probe() {
    let attrs = [
        provider_probe_attrs()[0],
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::OutcomeValue::Skip.as_str(),
            ),
        },
    ];
    let _metric =
        jackin_telemetry::counter(&jackin_telemetry::metric::BACKGROUND_CYCLES).add(1, &attrs);
}
