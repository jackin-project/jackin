use super::*;
use crate::{event::Value, schema::attrs};

#[test]
fn cardinality_rejects_the_257th_set_without_eviction() {
    install(&opentelemetry::global::meter("cardinality-test")).expect("test meter installation");
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
}

#[test]
fn fingerprint_is_order_independent_and_duplicates_reject() {
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
    assert_eq!(fingerprint(&first), fingerprint(&reversed));
    assert_eq!(
        validate_attributes(&LAUNCH_STAGE_DURATION, &[first[0], first[0]]),
        Err(Rejection::InvalidValue)
    );
}
