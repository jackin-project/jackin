use criterion::{Criterion, criterion_group, criterion_main};

fn bench_disabled(c: &mut Criterion) {
    c.bench_function("disabled counter", |b| {
        b.iter(|| {
            let _counter_result =
                jackin_telemetry::counter(&jackin_telemetry::metric::CLI_INVOCATIONS).add(1, &[]);
        });
    });
}

criterion_group!(benches, bench_disabled);
criterion_main!(benches);
