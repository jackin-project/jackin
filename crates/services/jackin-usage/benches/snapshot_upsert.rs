//! Per-row account snapshot upserts into a temp Turso DB (plan 026).
//!
//! ```sh
//! cargo bench -p jackin-usage --bench snapshot_upsert -- --test
//! ```

#![expect(clippy::panic, reason = "criterion bench harness: fail-fast setup")]

use std::hint::black_box;

use criterion::{Criterion, Throughput};
use jackin_protocol::control::FocusedUsageView;
use jackin_usage::usage_snapshot_store::store_usage_snapshots;
use tempfile::TempDir;

fn views(n: usize) -> Vec<FocusedUsageView> {
    (0..n)
        .map(|i| FocusedUsageView::unavailable(format!("bench-{i}"), 1_700_000_000 + i as i64))
        .collect()
}

fn bench_upsert(c: &mut Criterion) {
    let tmp = TempDir::new().unwrap_or_else(|e| panic!("{e}"));
    let db = tmp.path().join("snapshots.db");
    let n = 32usize;
    let batch = views(n);

    let mut group = c.benchmark_group("snapshot_upsert");
    group.sample_size(15);
    group.throughput(Throughput::Elements(n as u64));
    group.bench_function("store_usage_snapshots_32", |b| {
        b.iter(|| {
            // Fresh path each iter would dominate; reuse DB and overwrite.
            store_usage_snapshots(black_box(&db), black_box(&batch))
                .unwrap_or_else(|e| panic!("{e}"));
        });
    });
    group.finish();
}

criterion::criterion_group!(benches, bench_upsert);
criterion::criterion_main!(benches);
