//! Role-manifest parse + validate (every role load).
//!
//! ```sh
//! cargo bench -p jackin-manifest --bench manifest_validate -- --test
//! ```

use std::hint::black_box;

use criterion::{Criterion, Throughput};
use jackin_manifest::{RoleManifest, validate_agent_consistency, validate_role_manifest};

const SAMPLE: &str = r#"
version = "v1alpha6"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#;

fn bench_manifest(c: &mut Criterion) {
    let mut group = c.benchmark_group("manifest_validate");
    group.sample_size(40);
    group.throughput(Throughput::Bytes(SAMPLE.len() as u64));

    group.bench_function("parse_and_validate", |b| {
        b.iter(|| {
            let m: RoleManifest = toml::from_str(black_box(SAMPLE)).expect("parse");
            drop(validate_role_manifest(&m).expect("validate"));
            drop(validate_agent_consistency(&m).expect("consistency"));
            black_box(m);
        });
    });

    group.finish();
}

criterion::criterion_group!(benches, bench_manifest);
criterion::criterion_main!(benches);
