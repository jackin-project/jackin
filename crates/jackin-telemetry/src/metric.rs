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
    pub description: &'static str,
    pub unit: &'static str,
    pub kind: InstrumentKind,
    pub boundaries: &'static [f64],
    pub attributes: &'static [schema::AttributeRequirement],
}

impl InstrumentDef {
    const fn generated(metadata: &'static schema::MetricMetadata) -> Self {
        let kind = match metadata.instrument {
            schema::MetricInstrument::Counter => InstrumentKind::Counter,
            schema::MetricInstrument::UpDownCounter => InstrumentKind::UpDownCounter,
            schema::MetricInstrument::Histogram => InstrumentKind::Histogram,
        };
        Self {
            name: metadata.name,
            description: metadata.description,
            unit: metadata.unit,
            kind,
            boundaries: metadata.boundaries,
            attributes: metadata.attributes,
        }
    }
}

macro_rules! generated_instruments {
    ($($name:ident => $definition:ident),+ $(,)?) => {
        $(pub const $name: InstrumentDef =
            InstrumentDef::generated(&schema::metrics::$definition);)+
        pub const ALL: &[InstrumentDef] = &[$($name),+];
    };
}

generated_instruments! {
    CLI_INVOCATIONS => CLI_INVOCATIONS_DEF,
    CLI_DURATION => CLI_DURATION_DEF,
    CLI_FAILURES => CLI_FAILURES_DEF,
    UI_TRANSITIONS => UI_TRANSITIONS_DEF,
    UI_ACTIONS => UI_ACTIONS_DEF,
    UI_DWELL => UI_SCREEN_DWELL_DEF,
    UI_FOCUS_DURATION => UI_FOCUS_DURATION_DEF,
    UI_RENDER_DURATION => UI_RENDER_DURATION_DEF,
    UI_JANK => UI_JANK_DEF,
    LAUNCH_STAGE_DURATION => LAUNCH_STAGE_DURATION_DEF,
    LAUNCH_CACHE_REUSE => LAUNCH_CACHE_REUSE_DEF,
    PREWARM_JOBS => PREWARM_JOBS_DEF,
    PREWARM_ACTIVE => PREWARM_ACTIVE_DEF,
    PREWARM_DURATION => PREWARM_DURATION_DEF,
    BACKGROUND_CYCLES => BACKGROUND_CYCLES_DEF,
    BACKGROUND_CYCLE_DURATION => BACKGROUND_CYCLE_DURATION_DEF,
    CONNECTION_ATTEMPTS => CONNECTION_ATTEMPTS_DEF,
    CONNECTION_ACTIVE => CONNECTION_ACTIVE_DEF,
    CONNECTION_DURATION => CONNECTION_DURATION_DEF,
    DB_CLIENT_OPERATION_DURATION => DB_CLIENT_OPERATION_DURATION_DEF,
    RPC_REQUESTS => RPC_REQUESTS_DEF,
    RPC_ACTIVE => RPC_ACTIVE_DEF,
    RPC_DURATION => RPC_DURATION_DEF,
    AGENT_STATE_TRANSITIONS => AGENT_STATE_TRANSITIONS_DEF,
    AGENT_STATE_STUCK => AGENT_STATE_STUCK_DEF,
    AGENT_STATE_FLAPS => AGENT_STATE_FLAPS_DEF,
    TERMINAL_BYTES => TERMINAL_IO_BYTES_DEF,
    TERMINAL_CURSOR_MOVES => TERMINAL_CURSOR_MOVES_DEF,
    TERMINAL_RENDER_CELLS => TERMINAL_RENDER_CELLS_DEF,
    TERMINAL_RENDER_DURATION => TERMINAL_RENDER_DURATION_DEF,
    TERMINAL_RENDER_FRAMES => TERMINAL_RENDER_FRAMES_DEF,
    TERMINAL_INPUT_MOUSE => TERMINAL_INPUT_MOUSE_DEF,
    TELEMETRY_REJECTIONS => TELEMETRY_REJECTIONS_DEF,
    TELEMETRY_VALIDATE => TELEMETRY_VALIDATE_DEF,
}

static METER: OnceLock<Meter> = OnceLock::new();
static METER_RESERVED: Mutex<bool> = Mutex::new(false);
static SERIES: OnceLock<Mutex<HashMap<&'static str, HashSet<String>>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeterInstallError;

impl std::fmt::Display for MeterInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("telemetry facade meter is already installed or reserved")
    }
}

impl std::error::Error for MeterInstallError {}

#[must_use = "the reservation must be committed after the subscriber is installed"]
#[derive(Debug)]
pub struct MeterReservation {
    meter: Option<Meter>,
}

impl MeterReservation {
    pub fn commit(mut self) -> Result<(), MeterInstallError> {
        let meter = self.meter.take().ok_or(MeterInstallError)?;
        let result = METER.set(meter).map_err(|_| MeterInstallError);
        *METER_RESERVED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        result
    }
}

impl Drop for MeterReservation {
    fn drop(&mut self) {
        if self.meter.is_some() {
            *METER_RESERVED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        }
    }
}

pub fn reserve_meter(meter: &Meter) -> Result<MeterReservation, MeterInstallError> {
    let mut reserved = METER_RESERVED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if METER.get().is_some() || *reserved {
        return Err(MeterInstallError);
    }
    *reserved = true;
    Ok(MeterReservation {
        meter: Some(meter.clone()),
    })
}

pub fn install(meter: &Meter) -> Result<(), MeterInstallError> {
    reserve_meter(meter)?.commit()
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

fn validate_attributes(def: &InstrumentDef, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
    if def.attributes.is_empty() {
        return Ok(());
    }
    for attr in attrs {
        let Some(requirement) = def
            .attributes
            .iter()
            .find(|requirement| requirement.name == attr.key)
        else {
            return Err(Rejection::UnknownAttribute);
        };
        let valid = matches!(
            (attr.value, requirement.value_type),
            (Value::Str(_), schema::ValueType::String)
                | (Value::Bool(_), schema::ValueType::Boolean)
                | (Value::I64(_) | Value::U64(_), schema::ValueType::Integer)
                | (Value::F64(_), schema::ValueType::Double)
                | (Value::StrArray(_), schema::ValueType::StringArray)
        );
        if !valid {
            return Err(Rejection::InvalidValue);
        }
    }
    for requirement in def
        .attributes
        .iter()
        .filter(|requirement| requirement.requirement == schema::RequirementLevel::Required)
    {
        if !attrs.iter().any(|attr| attr.key == requirement.name) {
            return Err(Rejection::InvalidValue);
        }
    }
    Ok(())
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
        validate_attributes(self.0, attrs)?;
        let kv = key_values(attrs).inspect_err(|reason| health::reject(*reason))?;
        if !accept_series(self.0.name, &kv) {
            return Err(Rejection::Cardinality);
        }
        meter
            .u64_counter(self.0.name)
            .with_unit(self.0.unit)
            .with_description(self.0.description)
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
        validate_attributes(self.0, attrs)?;
        let kv = key_values(attrs).inspect_err(|reason| health::reject(*reason))?;
        if !accept_series(self.0.name, &kv) {
            return Err(Rejection::Cardinality);
        }
        meter
            .f64_histogram(self.0.name)
            .with_unit(self.0.unit)
            .with_description(self.0.description)
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
        validate_attributes(self.0, attrs)?;
        let kv = key_values(attrs).inspect_err(|reason| health::reject(*reason))?;
        if !accept_series(self.0.name, &kv) {
            return Err(Rejection::Cardinality);
        }
        meter
            .i64_up_down_counter(self.0.name)
            .with_unit(self.0.unit)
            .with_description(self.0.description)
            .build()
            .add(value, &kv);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
