use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_calibration(c: &mut Criterion) {
    c.bench_function("telemetry_calibration", |b| {
        b.iter(|| {
            let mut value = black_box(0x9e37_79b9_7f4a_7c15_u64);
            for index in 0..1_024_u64 {
                value = value.rotate_left(7) ^ black_box(index.wrapping_mul(0x100_0000_01b3));
            }
            black_box(value)
        });
    });
}

fn bench_disabled(c: &mut Criterion) {
    c.bench_function("disabled event", |b| {
        b.iter(|| {
            let _event_result = jackin_telemetry::emit_event(
                &jackin_telemetry::event::TELEMETRY_VALIDATE,
                jackin_telemetry::FieldSet::default(),
            );
        });
    });
    c.bench_function("disabled operation guard", |b| {
        b.iter(|| {
            let operation = jackin_telemetry::operation_or_disabled(
                &jackin_telemetry::operation::TELEMETRY_VALIDATE,
                &[],
            );
            operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        });
    });
    c.bench_function("disabled counter", |b| {
        b.iter(|| {
            let _counter_result =
                jackin_telemetry::counter(&jackin_telemetry::metric::CLI_INVOCATIONS).add(1, &[]);
        });
    });
}

criterion_group!(benches, bench_calibration, bench_disabled);
criterion_main!(benches);
