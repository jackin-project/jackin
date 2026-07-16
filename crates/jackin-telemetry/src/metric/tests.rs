use super::*;
use crate::{event::Value, schema::attrs};

#[test]
fn cardinality_rejects_the_257th_set_without_eviction() {
    install(&opentelemetry::global::meter("cardinality-test")).expect("test meter installation");
    let before = crate::facade_health().cardinality;
    for index in 0..limits::MAX_CARDINALITY {
        let value = index.to_string();
        counter(&TERMINAL_BYTES)
            .add(
                1,
                &[Attr {
                    key: attrs::STREAM_DIRECTION,
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
                key: attrs::STREAM_DIRECTION,
                value: Value::Str(overflow)
            }]
        ),
        Err(Rejection::Cardinality)
    );
    assert_eq!(crate::facade_health().cardinality, before + 1);
}
