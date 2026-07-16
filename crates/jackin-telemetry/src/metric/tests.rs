use super::*;
use crate::{event::Value, schema::attrs};

#[test]
fn cardinality_rejects_the_257th_set_without_eviction() {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    install(&provider.meter("cardinality-test")).expect("test meter installation");
    let before = crate::facade_health().cardinality;
    for index in 0..limits::MAX_CARDINALITY {
        let value = index.to_string();
        histogram(&DB_CLIENT_OPERATION_DURATION)
            .record(
                1.0,
                &[Attr {
                    key: attrs::std_attrs::DB_OPERATION_NAME,
                    value: Value::Str(&value),
                }],
            )
            .unwrap();
    }
    histogram(&DB_CLIENT_OPERATION_DURATION)
        .record(
            2.0,
            &[Attr {
                key: attrs::std_attrs::DB_OPERATION_NAME,
                value: Value::Str("0"),
            }],
        )
        .expect("an existing exact series remains accepted at the cap");
    let overflow = "overflow";
    assert_eq!(
        histogram(&DB_CLIENT_OPERATION_DURATION).record(
            1.0,
            &[Attr {
                key: attrs::std_attrs::DB_OPERATION_NAME,
                value: Value::Str(overflow)
            }]
        ),
        Err(Rejection::Cardinality)
    );
    assert_eq!(crate::facade_health().cardinality, before + 1);
    provider.force_flush().expect("metric flush");
    let point_count = exporter
        .get_finished_metrics()
        .expect("metric export")
        .iter()
        .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .find(|metric| metric.name() == DB_CLIENT_OPERATION_DURATION.name())
        .and_then(|metric| match metric.data() {
            opentelemetry_sdk::metrics::data::AggregatedMetrics::F64(
                opentelemetry_sdk::metrics::data::MetricData::Histogram(histogram),
            ) => Some(histogram.data_points().count()),
            _ => None,
        })
        .expect("exported governed histogram");
    assert_eq!(point_count, limits::MAX_CARDINALITY);
}

#[test]
fn series_identity_is_order_independent_and_duplicates_reject() {
    let first = [
        Attr {
            key: attrs::LAUNCH_STAGE_NAME,
            value: Value::Str("network"),
        },
        Attr {
            key: attrs::OUTCOME,
            value: Value::Str("success"),
        },
    ];
    let reversed = [first[1], first[0]];
    assert_eq!(series_identity(&first), series_identity(&reversed));
    assert_eq!(
        validate_attributes(&LAUNCH_STAGE_DURATION, &[first[0], first[0]]),
        Err(Rejection::InvalidValue)
    );
}
