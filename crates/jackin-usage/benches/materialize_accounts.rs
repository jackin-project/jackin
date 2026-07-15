//! Materialize-accounts hot path (plan 014 residual / R-014-materialize-bench).
//!
//! `UsageCache::materialize_accounts` encodes every cached focused usage view
//! and atomically writes `accounts.json`. The production path is container-
//! fixed (`/jackin/run/usage/accounts.json`); this bench injects a temp path
//! via the `#[doc(hidden)]` seam so host CI is hermetic.
//!
//! ```sh
//! cargo bench --bench materialize_accounts -p jackin-usage -- --quick
//! ```

#![expect(clippy::panic, reason = "criterion bench harness: fail-fast setup")]

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, Throughput};
use jackin_protocol::control::FocusedUsageView;
use jackin_usage::usage::UsageCache;
use tempfile::TempDir;

fn seed_cache(n: usize, path: PathBuf) -> UsageCache {
    let mut cache = UsageCache::default();
    cache.set_accounts_materialize_path(path);
    for i in 0..n {
        let agent = format!("agent-{i}");
        let view = FocusedUsageView::unavailable(format!("bench-{i}"), 1_700_000_000 + i as i64);
        cache.insert_snapshot_for_test(&agent, Some("provider"), view);
    }
    cache
}

fn bench_materialize(c: &mut Criterion) {
    let tmp = match TempDir::new() {
        Ok(t) => t,
        Err(e) => panic!("tempdir: {e}"),
    };
    let out = tmp.path().join("accounts.json");
    let n = 64usize;
    let cache = seed_cache(n, out);

    let mut group = c.benchmark_group("materialize_accounts");
    group.sample_size(20);
    group.throughput(Throughput::Elements(n as u64));
    group.bench_function("materialize_64_accounts", |b| {
        b.iter(|| {
            if let Err(e) = cache.materialize_accounts_for_bench(black_box(1_700_000_100)) {
                panic!("materialize: {e}");
            }
        });
    });
    group.finish();
}

criterion::criterion_group!(benches, bench_materialize);
criterion::criterion_main!(benches);
