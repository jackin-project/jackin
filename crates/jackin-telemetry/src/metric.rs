// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock, RwLock, atomic::AtomicU64},
    time::Instant,
};

use opentelemetry::{
    KeyValue,
    metrics::{
        Counter as OtelCounter, Gauge as OtelGauge, Histogram as OtelHistogram, Meter,
        ObservableCounter, UpDownCounter as OtelUpDownCounter,
    },
};

use crate::{
    event::{Attr, Rejection, Value},
    health, limits, schema, validation,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstrumentKind {
    Counter,
    Gauge,
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
            schema::MetricInstrument::Gauge => InstrumentKind::Gauge,
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

include!("metric_defs.rs");

#[derive(Debug)]
struct InstalledInstruments {
    counters: HashMap<&'static str, OtelCounter<u64>>,
    gauges: HashMap<&'static str, OtelGauge<f64>>,
    histograms: HashMap<&'static str, OtelHistogram<f64>>,
    up_down_counters: HashMap<&'static str, OtelUpDownCounter<i64>>,
    _health: ObservableCounter<u64>,
}

static INSTRUMENTS: RwLock<Option<InstalledInstruments>> = RwLock::new(None);
static METER_STATE: Mutex<MeterState> = Mutex::new(MeterState {
    reserved: false,
    active_generation: None,
});
static NEXT_METER_GENERATION: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DimensionValue {
    Str(String),
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(u64),
    StrArray(Vec<String>),
}

type SeriesIdentity = Vec<(&'static str, DimensionValue)>;
type SeriesByInstrument = HashMap<&'static str, Vec<SeriesIdentity>>;
static SERIES: OnceLock<Mutex<SeriesByInstrument>> = OnceLock::new();

#[derive(Debug)]
struct MeterState {
    reserved: bool,
    active_generation: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeterInstallError;

impl std::fmt::Display for MeterInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("telemetry facade meter is already installed or reserved")
    }
}

impl std::error::Error for MeterInstallError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterDetachError {
    DeadlineExceeded,
}

impl std::fmt::Display for MeterDetachError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("telemetry facade reader fence exceeded its shutdown deadline")
    }
}

impl std::error::Error for MeterDetachError {}

#[must_use = "the reservation must be committed after the subscriber is installed"]
#[derive(Debug)]
pub struct MeterReservation {
    instruments: Option<InstalledInstruments>,
}

impl MeterReservation {
    pub fn commit(mut self) -> Result<MeterInstallation, MeterInstallError> {
        let instruments = self.instruments.take().ok_or(MeterInstallError)?;
        let mut state = METER_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.reserved || state.active_generation.is_some() {
            state.reserved = false;
            return Err(MeterInstallError);
        }

        let generation = NEXT_METER_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *INSTRUMENTS
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(instruments);
        state.reserved = false;
        state.active_generation = Some(generation);
        Ok(MeterInstallation {
            generation,
            retired_instruments: None,
        })
    }
}

impl Drop for MeterReservation {
    fn drop(&mut self) {
        if self.instruments.is_some() {
            METER_STATE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .reserved = false;
        }
    }
}

/// Owns the installed facade instruments for one meter-provider lifecycle.
///
/// Detaching or dropping the installation removes the provider-bound facade and
/// clears its bounded-series registry. Dropping the installation also releases
/// the retained handles, allowing a later provider to be installed in the same
/// process without retaining state from the retired provider. If a direct drop
/// cannot acquire the reader fence immediately, it fails closed and preserves
/// the generation instead of waiting unboundedly or allowing overlap.
#[must_use = "the meter installation must live as long as its meter provider"]
#[derive(Debug)]
pub struct MeterInstallation {
    generation: u64,
    retired_instruments: Option<InstalledInstruments>,
}

impl MeterInstallation {
    /// Detach the facade from the active provider while retaining its handles
    /// until this installation is dropped.
    ///
    /// The write fence waits, up to `deadline`, for every in-flight metric
    /// operation that acquired the facade read lock. Calls that begin after
    /// the detach observe an empty facade and become no-ops instead of
    /// recording into a retiring provider.
    /// The generation remains active until the installation is dropped, so a
    /// replacement provider cannot be installed while the retired handles are
    /// still owned by this lease.
    pub fn detach_before(&mut self, deadline: Instant) -> Result<(), MeterDetachError> {
        self.detach_inner(deadline, || {})
    }

    fn detach_inner(
        &mut self,
        deadline: Instant,
        before_write: impl FnOnce(),
    ) -> Result<(), MeterDetachError> {
        if self.retired_instruments.is_some() {
            return Ok(());
        }

        let state = METER_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active_generation != Some(self.generation) {
            return Ok(());
        }

        if Instant::now() >= deadline {
            return Err(MeterDetachError::DeadlineExceeded);
        }
        before_write();
        let mut instruments = loop {
            match INSTRUMENTS.try_write() {
                Ok(instruments) => break instruments,
                Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(MeterDetachError::DeadlineExceeded);
                    }
                    std::thread::yield_now();
                }
            }
        };
        self.retired_instruments = instruments.take();
        if let Some(series) = SERIES.get() {
            series
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
        Ok(())
    }
}

impl Drop for MeterInstallation {
    fn drop(&mut self) {
        let mut state = METER_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active_generation != Some(self.generation) {
            return;
        }

        if let Some(retired) = self.retired_instruments.take() {
            state.active_generation = None;
            drop(state);
            drop(retired);
            return;
        }

        match INSTRUMENTS.try_write() {
            Ok(mut instruments) => {
                let retired = instruments.take();
                if let Some(series) = SERIES.get() {
                    series
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clear();
                }
                state.active_generation = None;
                drop(state);
                drop(retired);
            }
            Err(std::sync::TryLockError::Poisoned(error)) => {
                let mut instruments = error.into_inner();
                let retired = instruments.take();
                if let Some(series) = SERIES.get() {
                    series
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clear();
                }
                state.active_generation = None;
                drop(state);
                drop(retired);
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                // Drop cannot report a fence timeout. Leave the facade and
                // generation installed in this terminal state; a replacement
                // provider must be rejected rather than overlap a writer that
                // still owns the retiring facade.
            }
        }
    }
}

pub fn reserve_meter(meter: &Meter) -> Result<MeterReservation, MeterInstallError> {
    {
        let state = METER_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active_generation.is_some() || state.reserved {
            return Err(MeterInstallError);
        }
    }

    let instruments = build_instruments(meter);
    let mut state = METER_STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.active_generation.is_some() || state.reserved {
        return Err(MeterInstallError);
    }
    state.reserved = true;
    Ok(MeterReservation {
        instruments: Some(instruments),
    })
}

pub fn install(meter: &Meter) -> Result<MeterInstallation, MeterInstallError> {
    reserve_meter(meter)?.commit()
}

fn build_instruments(meter: &Meter) -> InstalledInstruments {
    let mut counters = HashMap::new();
    let mut gauges = HashMap::new();
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
            InstrumentKind::Gauge => {
                gauges.insert(
                    definition.name,
                    meter
                        .f64_gauge(definition.name)
                        .with_unit(definition.unit)
                        .with_description(definition.description)
                        .build(),
                );
            }
            InstrumentKind::UpDownCounter if definition.name != PROCESS_MEMORY_USAGE.name => {
                up_down_counters.insert(
                    definition.name,
                    meter
                        .i64_up_down_counter(definition.name)
                        .with_unit(definition.unit)
                        .with_description(definition.description)
                        .build(),
                );
            }
            InstrumentKind::Counter | InstrumentKind::UpDownCounter => {}
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
        gauges,
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
        return Err(Rejection::UnknownName);
    }
    limits::validate_name(def.name)
}

fn validate_attributes(def: &InstrumentDef, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
    if attrs.iter().any(|attr| {
        matches!(
            attr.key,
            schema::attrs::CLI_INVOCATION_ID
                | schema::attrs::std_attrs::SESSION_ID
                | schema::attrs::JOB_ID
                | schema::attrs::UI_SCREEN_VISIT_ID
                | schema::attrs::std_attrs::GEN_AI_CONVERSATION_ID
        )
    }) {
        health::reject(health::Signal::Metric, Rejection::Cardinality);
        return Err(Rejection::Cardinality);
    }
    validation::attributes(def.attributes, attrs, limits::MAX_METRIC_ATTRIBUTES)
        .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))
}

fn accept_series(name: &'static str, attrs: &[Attr<'_>]) -> bool {
    let order = series_order(attrs);
    let mut all = SERIES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if accept_series_in(&mut all, name, attrs, &order[..attrs.len()]) {
        return true;
    }
    health::reject(health::Signal::Metric, Rejection::Cardinality);
    false
}

fn accept_series_in(
    all: &mut SeriesByInstrument,
    name: &'static str,
    attrs: &[Attr<'_>],
    order: &[usize],
) -> bool {
    let stream = all.entry(name).or_default();
    if stream
        .iter()
        .any(|identity| series_matches(identity, attrs, order))
    {
        return true;
    }
    if stream.len() >= limits::MAX_CARDINALITY {
        return false;
    }
    stream.push(series_identity_with_order(attrs, order));
    true
}

fn series_order(attrs: &[Attr<'_>]) -> [usize; limits::MAX_METRIC_ATTRIBUTES] {
    let mut order = [0usize; limits::MAX_METRIC_ATTRIBUTES];
    for (index, slot) in order[..attrs.len()].iter_mut().enumerate() {
        *slot = index;
    }
    order[..attrs.len()].sort_unstable_by_key(|index| attrs[*index].key);
    order
}

fn series_matches(identity: &SeriesIdentity, attrs: &[Attr<'_>], order: &[usize]) -> bool {
    identity.len() == order.len()
        && identity.iter().zip(order).all(|((key, stored), index)| {
            let attr = attrs[*index];
            *key == attr.key && dimension_matches(stored, attr.value)
        })
}

fn dimension_matches(stored: &DimensionValue, value: Value<'_>) -> bool {
    match (stored, value) {
        (DimensionValue::Str(stored), Value::Str(value)) => stored == value,
        (DimensionValue::Bool(stored), Value::Bool(value)) => *stored == value,
        (DimensionValue::I64(stored), Value::I64(value)) => *stored == value,
        (DimensionValue::U64(stored), Value::U64(value)) => *stored == value,
        (DimensionValue::F64(stored), Value::F64(value)) => *stored == value.to_bits(),
        (DimensionValue::StrArray(stored), Value::StrArray(values)) => {
            stored.len() == values.len()
                && stored.iter().zip(values).all(|(left, right)| left == right)
        }
        _ => false,
    }
}

#[cfg(test)]
fn series_identity(attrs: &[Attr<'_>]) -> SeriesIdentity {
    let order = series_order(attrs);
    series_identity_with_order(attrs, &order[..attrs.len()])
}

fn series_identity_with_order(attrs: &[Attr<'_>], order: &[usize]) -> SeriesIdentity {
    order
        .iter()
        .map(|index| {
            let attr = attrs[*index];
            let value = match attr.value {
                Value::Str(value) => DimensionValue::Str(value.to_owned()),
                Value::Bool(value) => DimensionValue::Bool(value),
                Value::I64(value) => DimensionValue::I64(value),
                Value::U64(value) => DimensionValue::U64(value),
                Value::F64(value) => DimensionValue::F64(value.to_bits()),
                Value::StrArray(values) => DimensionValue::StrArray(
                    values.iter().map(|value| (*value).to_owned()).collect(),
                ),
            };
            (attr.key, value)
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Counter(&'static InstrumentDef);
#[derive(Clone, Copy, Debug)]
pub struct Gauge(&'static InstrumentDef);
#[derive(Clone, Copy, Debug)]
pub struct Histogram(&'static InstrumentDef);
#[derive(Clone, Copy, Debug)]
pub struct UpDownCounter(&'static InstrumentDef);

#[must_use]
pub const fn counter(def: &'static InstrumentDef) -> Counter {
    Counter(def)
}
#[must_use]
pub const fn gauge(def: &'static InstrumentDef) -> Gauge {
    Gauge(def)
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
        reject_identity_dimensions(attrs)?;
        let instruments = INSTRUMENTS
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(instruments) = instruments.as_ref() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::Counter)
            .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))?;
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
        reject_identity_dimensions(attrs)?;
        let instruments = INSTRUMENTS
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(instruments) = instruments.as_ref() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::Histogram)
            .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))?;
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

impl Gauge {
    pub fn record(self, value: f64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        reject_identity_dimensions(attrs)?;
        let instruments = INSTRUMENTS
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(instruments) = instruments.as_ref() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::Gauge)
            .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))?;
        validate_attributes(self.0, attrs)?;
        if !accept_series(self.0.name, attrs) {
            return Err(Rejection::Cardinality);
        }
        let kv = key_values(attrs)
            .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))?;
        instruments.gauges[&self.0.name].record(value, &kv);
        Ok(())
    }
}

impl UpDownCounter {
    pub fn add(self, value: i64, attrs: &[Attr<'_>]) -> Result<(), Rejection> {
        reject_identity_dimensions(attrs)?;
        let instruments = INSTRUMENTS
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(instruments) = instruments.as_ref() else {
            return Ok(());
        };
        validate_instrument(self.0, InstrumentKind::UpDownCounter)
            .inspect_err(|reason| health::reject(health::Signal::Metric, *reason))?;
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

fn reject_identity_dimensions(attrs: &[Attr<'_>]) -> Result<(), Rejection> {
    if attrs.iter().any(|attr| {
        matches!(
            attr.key,
            schema::attrs::CLI_INVOCATION_ID
                | schema::attrs::std_attrs::SESSION_ID
                | schema::attrs::JOB_ID
                | schema::attrs::UI_SCREEN_VISIT_ID
                | schema::attrs::std_attrs::GEN_AI_CONVERSATION_ID
        )
    }) {
        health::reject(health::Signal::Metric, Rejection::Cardinality);
        Err(Rejection::Cardinality)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
