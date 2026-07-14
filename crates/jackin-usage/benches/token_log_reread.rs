//! Token-log whole-file re-read recompute cost (plan 026).
//!
//! ```sh
//! cargo bench -p jackin-usage --bench token_log_reread -- --test
//! ```

#![expect(
    clippy::expect_used,
    reason = "criterion bench harness: fail-fast fixture setup"
)]

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, Throughput};
use jackin_usage::token_monitor::{SpendAcc, recompute_spend};
use tempfile::TempDir;

fn write_logs(dir: &std::path::Path, files: usize, lines: usize) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(files);
    for i in 0..files {
        let p = dir.join(format!("log-{i}.jsonl"));
        let mut body = String::new();
        for j in 0..lines {
            body.push_str(&format!(
                "{{\"type\":\"usage\",\"input_tokens\":{},\"output_tokens\":{}}}\n",
                100 + j,
                50 + j
            ));
        }
        std::fs::write(&p, body).expect("write log");
        paths.push(p);
    }
    paths
}

fn bench_reread(c: &mut Criterion) {
    let tmp = TempDir::new().expect("tempdir");
    let sizes = [(4usize, 200usize), (16, 500), (32, 1000)];
    let mut group = c.benchmark_group("token_log_reread");
    group.sample_size(20);

    for (files, lines) in sizes {
        let paths = write_logs(tmp.path(), files, lines);
        let elems = (files * lines) as u64;
        group.throughput(Throughput::Elements(elems));
        group.bench_function(format!("recompute_{files}x{lines}"), |b| {
            b.iter(|| {
                let acc =
                    recompute_spend(black_box(&paths), "bench", |text, acc: &mut SpendAcc| {
                        // Cheap fold: count non-empty lines as activity units.
                        acc.input += text.lines().filter(|l| !l.is_empty()).count() as u64;
                        acc.seen = true;
                    });
                black_box(acc);
            });
        });
    }
    group.finish();
}

criterion::criterion_group!(benches, bench_reread);
criterion::criterion_main!(benches);
