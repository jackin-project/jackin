//! Config load/parse hot path (every `jackin load`).
//!
//! ```sh
//! cargo bench -p jackin-config --bench config_resolve -- --test
//! ```

use std::hint::black_box;

use criterion::{Criterion, Throughput};
use jackin_config::AppConfig;
use jackin_core::paths::JackinPaths;
use tempfile::TempDir;

const SAMPLE: &str = r#"
version = "v1alpha9"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

[claude]
auth_forward = "sync"

[env]
OPERATOR_ORG = "acme"
"#;

fn bench_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_resolve");
    group.sample_size(30);
    group.throughput(Throughput::Bytes(SAMPLE.len() as u64));

    group.bench_function("toml_parse_app_config", |b| {
        b.iter(|| {
            let cfg: AppConfig = toml::from_str(black_box(SAMPLE)).expect("parse");
            black_box(cfg);
        });
    });

    group.bench_function("load_split_config_from_disk", |b| {
        b.iter_batched(
            || {
                let tmp = TempDir::new().expect("tempdir");
                let paths = JackinPaths::for_tests(tmp.path());
                std::fs::create_dir_all(&paths.config_dir).expect("config dir");
                std::fs::write(paths.config_dir.join("config.toml"), SAMPLE).expect("write");
                (tmp, paths)
            },
            |(_tmp, paths)| {
                let cfg = jackin_config::load_split_config(&paths, None).expect("load");
                black_box(cfg);
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion::criterion_group!(benches, bench_config);
criterion::criterion_main!(benches);
