// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

use opentelemetry::{KeyValue, metrics::Meter};

use crate::{
    event::{Attr, Rejection, Value},
    health, limits, privacy, schema,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstrumentKind {
    Counter,
    UpDownCounter,
    Histogram,
}

#[derive(Clone, Copy, Debug)]
pub struct InstrumentDef {
    pub name: &'static str,
    pub unit: &'static str,
    pub kind: InstrumentKind,
}

pub const CLI_INVOCATIONS: InstrumentDef = InstrumentDef {
    name: "cli.invocations",
    unit: "{invocation}",
    kind: InstrumentKind::Counter,
};
pub const CLI_DURATION: InstrumentDef = InstrumentDef {
    name: "cli.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const CLI_FAILURES: InstrumentDef = InstrumentDef {
    name: "cli.failures",
    unit: "{failure}",
    kind: InstrumentKind::Counter,
};
pub const UI_TRANSITIONS: InstrumentDef = InstrumentDef {
    name: "ui.transitions",
    unit: "{transition}",
    kind: InstrumentKind::Counter,
};
pub const UI_ACTIONS: InstrumentDef = InstrumentDef {
    name: "ui.actions",
    unit: "{action}",
    kind: InstrumentKind::Counter,
};
pub const UI_DWELL: InstrumentDef = InstrumentDef {
    name: "ui.screen.dwell",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const UI_FOCUS_DURATION: InstrumentDef = InstrumentDef {
    name: "ui.focus.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const UI_RENDER_DURATION: InstrumentDef = InstrumentDef {
    name: "ui.render.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const LAUNCH_STAGE_DURATION: InstrumentDef = InstrumentDef {
    name: "launch.stage.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const LAUNCH_CACHE_REUSE: InstrumentDef = InstrumentDef {
    name: "launch.cache.reuse",
    unit: "{reuse}",
    kind: InstrumentKind::Counter,
};
pub const PREWARM_JOBS: InstrumentDef = InstrumentDef {
    name: "prewarm.jobs",
    unit: "{job}",
    kind: InstrumentKind::Counter,
};
pub const PREWARM_ACTIVE: InstrumentDef = InstrumentDef {
    name: "prewarm.active",
    unit: "{job}",
    kind: InstrumentKind::UpDownCounter,
};
pub const PREWARM_DURATION: InstrumentDef = InstrumentDef {
    name: "prewarm.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const BACKGROUND_CYCLES: InstrumentDef = InstrumentDef {
    name: "background.cycles",
    unit: "{cycle}",
    kind: InstrumentKind::Counter,
};
pub const BACKGROUND_CYCLE_DURATION: InstrumentDef = InstrumentDef {
    name: "background.cycle.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const CONNECTION_ATTEMPTS: InstrumentDef = InstrumentDef {
    name: "connection.attempts",
    unit: "{attempt}",
    kind: InstrumentKind::Counter,
};
pub const CONNECTION_ACTIVE: InstrumentDef = InstrumentDef {
    name: "connection.active",
    unit: "{connection}",
    kind: InstrumentKind::UpDownCounter,
};
pub const CONNECTION_DURATION: InstrumentDef = InstrumentDef {
    name: "connection.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const RPC_REQUESTS: InstrumentDef = InstrumentDef {
    name: "rpc.requests",
    unit: "{request}",
    kind: InstrumentKind::Counter,
};
pub const RPC_ACTIVE: InstrumentDef = InstrumentDef {
    name: "rpc.active",
    unit: "{request}",
    kind: InstrumentKind::UpDownCounter,
};
pub const RPC_DURATION: InstrumentDef = InstrumentDef {
    name: "rpc.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const AGENT_STATE_TRANSITIONS: InstrumentDef = InstrumentDef {
    name: "agent.state.transitions",
    unit: "{transition}",
    kind: InstrumentKind::Counter,
};
pub const AGENT_STATE_STUCK: InstrumentDef = InstrumentDef {
    name: "agent.state.stuck",
    unit: "{event}",
    kind: InstrumentKind::Counter,
};
pub const AGENT_STATE_FLAPS: InstrumentDef = InstrumentDef {
    name: "agent.state.flaps",
    unit: "{event}",
    kind: InstrumentKind::Counter,
};
pub const TERMINAL_BYTES: InstrumentDef = InstrumentDef {
    name: "terminal.io.bytes",
    unit: "By",
    kind: InstrumentKind::Counter,
};
pub const TERMINAL_CURSOR_MOVES: InstrumentDef = InstrumentDef {
    name: "terminal.cursor.moves",
    unit: "{move}",
    kind: InstrumentKind::Counter,
};
pub const TERMINAL_RENDER_CELLS: InstrumentDef = InstrumentDef {
    name: "terminal.render.cells",
    unit: "{cell}",
    kind: InstrumentKind::Counter,
};
pub const TERMINAL_RENDER_DURATION: InstrumentDef = InstrumentDef {
    name: "terminal.render.duration",
    unit: "s",
    kind: InstrumentKind::Histogram,
};
pub const TERMINAL_RENDER_FRAMES: InstrumentDef = InstrumentDef {
    name: "terminal.render.frames",
    unit: "{frame}",
    kind: InstrumentKind::Counter,
};
pub const TERMINAL_INPUT_MOUSE: InstrumentDef = InstrumentDef {
    name: "terminal.input.mouse",
    unit: "{event}",
    kind: InstrumentKind::Counter,
};
pub const TELEMETRY_REJECTIONS: InstrumentDef = InstrumentDef {
    name: "telemetry.rejections",
    unit: "{rejection}",
    kind: InstrumentKind::Counter,
};
pub const TELEMETRY_VALIDATE: InstrumentDef = InstrumentDef {
    name: "telemetry.validate",
    unit: "{validation}",
    kind: InstrumentKind::Counter,
};

static METER: OnceLock<Meter> = OnceLock::new();
static SERIES: OnceLock<Mutex<HashMap<&'static str, HashSet<String>>>> = OnceLock::new();

pub fn install(meter: &Meter) {
    drop(METER.set(meter.clone()));
}

fn key_values(attrs: &[Attr<'_>]) -> Result<Vec<KeyValue>, Rejection> {
    if attrs.len() > limits::MAX_METRIC_ATTRIBUTES {
        return Err(Rejection::SizeLimit);
    }
    attrs
        .iter()
        .map(|attr| {
            privacy::validate_key(attr.key)?;
            limits::validate_value(&attr.value)?;
            let value = match attr.value {
                Value::Str(v) => opentelemetry::Value::String(v.to_owned().into()),
                Value::Bool(v) => opentelemetry::Value::Bool(v),
                Value::I64(v) => opentelemetry::Value::I64(v),
                Value::U64(v) => opentelemetry::Value::I64(i64::try_from(v).unwrap_or(i64::MAX)),
                Value::F64(v) => opentelemetry::Value::F64(v),
                Value::StrArray(v) => opentelemetry::Value::Array(opentelemetry::Array::String(
                    v.iter().map(|s| (*s).to_owned().into()).collect(),
                )),
            };
            Ok(KeyValue::new(attr.key, value))
        })
        .collect()
}

fn validate_instrument(def: &InstrumentDef, expected: InstrumentKind) -> Result<(), Rejection> {
    if def.kind != expected || !schema::metrics::ALL.contains(&def.name) {
        health::reject(Rejection::UnknownName);
        return Err(Rejection::UnknownName);
    }
    limits::validate_name(def.name)
}

fn accept_series(name: &'static str, attrs: &[KeyValue]) -> bool {
    let fingerprint = format!("{attrs:?}");
    let mut all = SERIES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let stream = all.entry(name).or_default();
    if stream.contains(&fingerprint) {
        return true;
    }
    if stream.len() >= limits::MAX_CARDINALITY {
        health::reject(Rejection::Cardinality);
        return false;
    }
    stream.insert(fingerprint);
    true
}

#[derive(Clone, Copy, Debug)]
pub struct Counter(&'static InstrumentDef);
#[derive(Clone, Copy, Debug)]
pub struct Histogram(&'static InstrumentDef);
#[derive(Clone, Copy, Debug)]
pub struct UpDownCounter(&'static InstrumentDef);

#[must_use]
pub const fn counter(def: &'static InstrumentDef) -> Counter {
    Counter(def)
}
#[must_use]
pub const fn histogram(def: &'static InstrumentDef) -> Histogram {
    Histogram(def)
}
#[must_use]
pub const fn up_down_counter(def: &'static InstrumentDef) -> UpDownCounter {
    UpDownCounter(def)
}

impl Counter {
    pub fn add(self, value: u64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        let Some(meter) = METER.get() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::Counter)?;
        let kv = key_values(attrs).inspect_err(|reason| health::reject(*reason))?;
        if !accept_series(self.0.name, &kv) {
            return Err(Rejection::Cardinality);
        }
        meter
            .u64_counter(self.0.name)
            .with_unit(self.0.unit)
            .build()
            .add(value, &kv);
        Ok(())
    }
}
impl Histogram {
    pub fn record(self, value: f64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        let Some(meter) = METER.get() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::Histogram)?;
        let kv = key_values(attrs).inspect_err(|reason| health::reject(*reason))?;
        if !accept_series(self.0.name, &kv) {
            return Err(Rejection::Cardinality);
        }
        meter
            .f64_histogram(self.0.name)
            .with_unit(self.0.unit)
            .build()
            .record(value, &kv);
        Ok(())
    }
}

impl UpDownCounter {
    pub fn add(self, value: i64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        let Some(meter) = METER.get() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::UpDownCounter)?;
        let kv = key_values(attrs).inspect_err(|reason| health::reject(*reason))?;
        if !accept_series(self.0.name, &kv) {
            return Err(Rejection::Cardinality);
        }
        meter
            .i64_up_down_counter(self.0.name)
            .with_unit(self.0.unit)
            .build()
            .add(value, &kv);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::Value, schema::attrs};

    #[test]
    fn cardinality_rejects_the_257th_set_without_eviction() {
        install(&opentelemetry::global::meter("cardinality-test"));
        let before = crate::facade_health().cardinality;
        for index in 0..limits::MAX_CARDINALITY {
            let value = index.to_string();
            counter(&TERMINAL_BYTES)
                .add(
                    1,
                    &[Attr {
                        key: attrs::JOB_ID,
                        value: Value::Str(&value),
                    }],
                )
                .unwrap();
        }
        let overflow = "overflow";
        assert_eq!(
            counter(&TERMINAL_BYTES).add(
                1,
                &[Attr {
                    key: attrs::JOB_ID,
                    value: Value::Str(overflow)
                }]
            ),
            Err(Rejection::Cardinality)
        );
        assert_eq!(crate::facade_health().cardinality, before + 1);
    }
}
