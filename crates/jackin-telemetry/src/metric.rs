// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

use opentelemetry::{
    KeyValue,
    metrics::{
        Counter as OtelCounter, Histogram as OtelHistogram, Meter, ObservableCounter,
        UpDownCounter as OtelUpDownCounter,
    },
};

use crate::{
    event::{Attr, Rejection, Value},
    health, limits, schema, validation,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstrumentKind {
    Counter,
    UpDownCounter,
    Histogram,
}

#[derive(Clone, Copy, Debug)]
pub struct InstrumentDef {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) unit: &'static str,
    pub(crate) kind: InstrumentKind,
    pub(crate) boundaries: &'static [f64],
    pub(crate) attributes: &'static [schema::AttributeRequirement],
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

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn description(&self) -> &'static str {
        self.description
    }

    #[must_use]
    pub const fn unit(&self) -> &'static str {
        self.unit
    }

    #[must_use]
    pub const fn boundaries(&self) -> &'static [f64] {
        self.boundaries
    }

    #[must_use]
    pub const fn dimensions(&self) -> &'static [schema::AttributeRequirement] {
        self.attributes
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

#[derive(Debug)]
struct InstalledInstruments {
    counters: HashMap<&'static str, OtelCounter<u64>>,
    histograms: HashMap<&'static str, OtelHistogram<f64>>,
    up_down_counters: HashMap<&'static str, OtelUpDownCounter<i64>>,
    _health: ObservableCounter<u64>,
}

static INSTRUMENTS: OnceLock<InstalledInstruments> = OnceLock::new();
static METER_RESERVED: Mutex<bool> = Mutex::new(false);
type SeriesFingerprint = (u64, u64);
type SeriesByInstrument = HashMap<&'static str, HashSet<SeriesFingerprint>>;
static SERIES: OnceLock<Mutex<SeriesByInstrument>> = OnceLock::new();

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
    instruments: Option<InstalledInstruments>,
}

impl MeterReservation {
    pub fn commit(mut self) -> Result<(), MeterInstallError> {
        let instruments = self.instruments.take().ok_or(MeterInstallError)?;
        let result = INSTRUMENTS.set(instruments).map_err(|_| MeterInstallError);
        *METER_RESERVED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        result
    }
}

impl Drop for MeterReservation {
    fn drop(&mut self) {
        if self.instruments.is_some() {
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
    if INSTRUMENTS.get().is_some() || *reserved {
        return Err(MeterInstallError);
    }
    *reserved = true;
    Ok(MeterReservation {
        instruments: Some(build_instruments(meter)),
    })
}

pub fn install(meter: &Meter) -> Result<(), MeterInstallError> {
    reserve_meter(meter)?.commit()
}

fn build_instruments(meter: &Meter) -> InstalledInstruments {
    let mut counters = HashMap::new();
    let mut histograms = HashMap::new();
    let mut up_down_counters = HashMap::new();
    for definition in ALL {
        match definition.kind {
            InstrumentKind::Counter if definition.name != TELEMETRY_REJECTIONS.name => {
                counters.insert(
                    definition.name,
                    meter
                        .u64_counter(definition.name)
                        .with_unit(definition.unit)
                        .with_description(definition.description)
                        .build(),
                );
            }
            InstrumentKind::Histogram => {
                histograms.insert(
                    definition.name,
                    meter
                        .f64_histogram(definition.name)
                        .with_unit(definition.unit)
                        .with_description(definition.description)
                        .build(),
                );
            }
            InstrumentKind::UpDownCounter => {
                up_down_counters.insert(
                    definition.name,
                    meter
                        .i64_up_down_counter(definition.name)
                        .with_unit(definition.unit)
                        .with_description(definition.description)
                        .build(),
                );
            }
            InstrumentKind::Counter => {}
        }
    }
    let dimensions = health_dimensions();
    let health = meter
        .u64_observable_counter(TELEMETRY_REJECTIONS.name)
        .with_unit(TELEMETRY_REJECTIONS.unit)
        .with_description(TELEMETRY_REJECTIONS.description)
        .with_callback(move |observer| {
            for (signal, reason, attrs) in &dimensions {
                observer.observe(health::count(*signal, *reason), attrs);
            }
        })
        .build();
    InstalledInstruments {
        counters,
        histograms,
        up_down_counters,
        _health: health,
    }
}

fn health_dimensions() -> Vec<(health::Signal, Rejection, [KeyValue; 2])> {
    let reasons = [
        Rejection::UnknownName,
        Rejection::UnknownAttribute,
        Rejection::InvalidValue,
        Rejection::Privacy,
        Rejection::Cardinality,
        Rejection::SizeLimit,
    ];
    health::Signal::ALL
        .into_iter()
        .flat_map(|signal| {
            reasons.into_iter().map(move |reason| {
                (
                    signal,
                    reason,
                    [
                        KeyValue::new(schema::attrs::TELEMETRY_SIGNAL, signal.as_str()),
                        KeyValue::new(
                            schema::attrs::TELEMETRY_REJECTION_REASON,
                            rejection_name(reason),
                        ),
                    ],
                )
            })
        })
        .collect()
}

const fn rejection_name(reason: Rejection) -> &'static str {
    match reason {
        Rejection::UnknownName => "unknown_name",
        Rejection::UnknownAttribute => "unknown_attribute",
        Rejection::InvalidValue => "invalid_value",
        Rejection::Privacy => "privacy",
        Rejection::Cardinality => "cardinality",
        Rejection::SizeLimit => "size_limit",
    }
}

fn key_values(attrs: &[Attr<'_>]) -> Result<Vec<KeyValue>, Rejection> {
    if attrs.len() > limits::MAX_METRIC_ATTRIBUTES {
        return Err(Rejection::SizeLimit);
    }
    attrs
        .iter()
        .map(|attr| {
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

fn validate_instrument(
    def: &'static InstrumentDef,
    expected: InstrumentKind,
) -> Result<(), Rejection> {
    let canonical = schema::metrics::definition(def.name);
    if def.kind != expected
        || !canonical.is_some_and(|metadata| {
            metadata.name == def.name
                && metadata.description == def.description
                && metadata.unit == def.unit
                && metadata.attributes == def.attributes
        })
    {
        health::reject(health::Signal::Metric, Rejection::UnknownName);
        return Err(Rejection::UnknownName);
    }
    limits::validate_name(def.name)
}

fn validate_attributes(def: &InstrumentDef, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
    validation::attributes(def.attributes, attrs, limits::MAX_METRIC_ATTRIBUTES)
}

fn accept_series(name: &'static str, attrs: &[Attr<'_>]) -> bool {
    let fingerprint = fingerprint(attrs);
    let mut all = SERIES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let stream = all.entry(name).or_default();
    if stream.contains(&fingerprint) {
        return true;
    }
    if stream.len() >= limits::MAX_CARDINALITY {
        health::reject(health::Signal::Metric, Rejection::Cardinality);
        return false;
    }
    stream.insert(fingerprint);
    true
}

fn fingerprint(attrs: &[Attr<'_>]) -> (u64, u64) {
    let mut order = [0usize; limits::MAX_METRIC_ATTRIBUTES];
    for (index, slot) in order[..attrs.len()].iter_mut().enumerate() {
        *slot = index;
    }
    order[..attrs.len()].sort_unstable_by_key(|index| attrs[*index].key);
    let mut first = 0xcbf2_9ce4_8422_2325_u64;
    let mut second = 0x8422_2325_cbf2_9ce4_u64;
    for index in &order[..attrs.len()] {
        hash_attr(&mut first, &attrs[*index]);
        hash_attr(&mut second, &attrs[*index]);
    }
    (first, second)
}

fn hash_attr(state: &mut u64, attr: &Attr<'_>) {
    let mut hash = |bytes: &[u8]| {
        for byte in bytes {
            *state = (*state ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
        }
    };
    hash(attr.key.as_bytes());
    match attr.value {
        Value::Str(value) => hash(value.as_bytes()),
        Value::Bool(value) => hash(&[u8::from(value)]),
        Value::I64(value) => hash(&value.to_le_bytes()),
        Value::U64(value) => hash(&value.to_le_bytes()),
        Value::F64(value) => hash(&value.to_bits().to_le_bytes()),
        Value::StrArray(values) => {
            for value in values {
                hash(value.as_bytes());
                hash(&[0]);
            }
        }
    }
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
        let Some(instruments) = INSTRUMENTS.get() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::Counter)?;
        validate_attributes(self.0, attrs)?;
        if !accept_series(self.0.name, attrs) {
            return Err(Rejection::Cardinality);
        }
        let kv = key_values(attrs)
            .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))?;
        instruments.counters[&self.0.name].add(value, &kv);
        Ok(())
    }
}
impl Histogram {
    pub fn record(self, value: f64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        let Some(instruments) = INSTRUMENTS.get() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::Histogram)?;
        validate_attributes(self.0, attrs)?;
        if !accept_series(self.0.name, attrs) {
            return Err(Rejection::Cardinality);
        }
        let kv = key_values(attrs)
            .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))?;
        instruments.histograms[&self.0.name].record(value, &kv);
        Ok(())
    }
}

impl UpDownCounter {
    pub fn add(self, value: i64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        let Some(instruments) = INSTRUMENTS.get() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::UpDownCounter)?;
        validate_attributes(self.0, attrs)?;
        if !accept_series(self.0.name, attrs) {
            return Err(Rejection::Cardinality);
        }
        let kv = key_values(attrs)
            .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))?;
        instruments.up_down_counters[&self.0.name].add(value, &kv);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
