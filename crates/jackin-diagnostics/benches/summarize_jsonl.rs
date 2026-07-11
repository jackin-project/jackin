//! Diagnostics JSONL summary benchmark.
//!
//! `summarize_reader` walks every event line of a run log, parsing each as
//! owned `serde_json::Value` (and re-parsing nested `detail` for timing/
//! stage events). This is the Phase 4 hot path behind PERF-diag-double-parse.
//! The fixture is a ~20 MB synthetic JSONL buffer of realistic event shapes
//! so before/after of a streaming/borrowed parse can be compared.
//!
//! ```sh
//! cargo bench --bench summarize_jsonl -p jackin-diagnostics -- --quick
//! ```

use std::hint::black_box;
use std::io::Cursor;

use criterion::{BatchSize, Criterion, Throughput};
use jackin_diagnostics::summarize_reader;

/// Target fixture size (bytes) — large enough to dominate setup noise.
const TARGET_BYTES: usize = 20 * 1024 * 1024;

/// One repeating realistic event cycle (mirrors shapes in `summary/tests.rs`).
fn event_cycle(base_ts: u64) -> String {
    format!(
        r#"{{"run_id":"run-bench","ts_ms":{base_ts},"kind":"run_started","message":"start"}}
{{"run_id":"run-bench","ts_ms":{t1},"kind":"stage_started","stage":"hardline","message":"hardline start"}}
{{"run_id":"run-bench","ts_ms":{t2},"kind":"stage_done","stage":"hardline","message":"hardline done","detail":"{{\"duration_ms\":50}}"}}
{{"run_id":"run-bench","ts_ms":{t3},"kind":"timing_done","stage":"launch","message":"socket ready","detail":"{{\"name\":\"socket_probe\",\"duration_ms\":17}}"}}
{{"run_id":"run-bench","ts_ms":{t4},"kind":"warning","stage":"op","message":"secret missing"}}
{{"run_id":"run-bench","ts_ms":{t5},"kind":"selected_image_refresh_cache_miss","stage":"image","message":"cache miss","detail":"pull_base_image=true"}}
"#,
        t1 = base_ts + 10,
        t2 = base_ts + 60,
        t3 = base_ts + 120,
        t4 = base_ts + 130,
        t5 = base_ts + 140,
    )
}

fn build_fixture() -> Vec<u8> {
    let mut out = Vec::with_capacity(TARGET_BYTES + 4096);
    let mut ts = 1_000u64;
    while out.len() < TARGET_BYTES {
        out.extend_from_slice(event_cycle(ts).as_bytes());
        ts = ts.saturating_add(200);
    }
    out
}

fn bench_summarize(c: &mut Criterion) {
    let fixture = build_fixture();
    let bytes = fixture.len() as u64;

    let mut group = c.benchmark_group("summarize_jsonl");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(bytes));

    group.bench_function("summarize_reader_20mb", |b| {
        b.iter_batched(
            || fixture.clone(),
            |buf| {
                let summary = summarize_reader(Cursor::new(buf)).unwrap_or_else(|err| {
                    panic!("summarize fixture should parse: {err}");
                });
                black_box(summary.event_count);
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    bench_summarize(&mut criterion);
    criterion.final_summary();
}
