// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

use opentelemetry::{KeyValue, metrics::Meter};

use crate::{
    event::{Attr, Rejection, Value},
    health, limits, privacy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstrumentKind {
    Counter,
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
pub const TERMINAL_BYTES: InstrumentDef = InstrumentDef {
    name: "terminal.io.bytes",
    unit: "By",
    kind: InstrumentKind::Counter,
};
pub const TELEMETRY_REJECTIONS: InstrumentDef = InstrumentDef {
    name: "telemetry.rejections",
    unit: "{rejection}",
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

#[must_use]
pub const fn counter(def: &'static InstrumentDef) -> Counter {
    Counter(def)
}
#[must_use]
pub const fn histogram(def: &'static InstrumentDef) -> Histogram {
    Histogram(def)
}

impl Counter {
    pub fn add(self, value: u64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        let kv = key_values(attrs).inspect_err(|reason| health::reject(*reason))?;
        if !accept_series(self.0.name, &kv) {
            return Err(Rejection::Cardinality);
        }
        if let Some(meter) = METER.get() {
            meter
                .u64_counter(self.0.name)
                .with_unit(self.0.unit)
                .build()
                .add(value, &kv);
        }
        Ok(())
    }
}
impl Histogram {
    pub fn record(self, value: f64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        let kv = key_values(attrs).inspect_err(|reason| health::reject(*reason))?;
        if !accept_series(self.0.name, &kv) {
            return Err(Rejection::Cardinality);
        }
        if let Some(meter) = METER.get() {
            meter
                .f64_histogram(self.0.name)
                .with_unit(self.0.unit)
                .build()
                .record(value, &kv);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::Value, schema::attrs};

    #[test]
    fn cardinality_rejects_the_257th_set_without_eviction() {
        static DEF: InstrumentDef = InstrumentDef {
            name: "test.cardinality",
            unit: "{item}",
            kind: InstrumentKind::Counter,
        };
        let before = crate::facade_health().cardinality;
        for index in 0..limits::MAX_CARDINALITY {
            let value = index.to_string();
            counter(&DEF)
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
            counter(&DEF).add(
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
