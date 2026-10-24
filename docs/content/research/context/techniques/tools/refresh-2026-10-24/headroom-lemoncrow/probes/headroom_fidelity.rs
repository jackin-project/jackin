use headroom_core::transforms::{JsonMinifier, ReformatTransform, LogCompressor, LogCompressorConfig};
use headroom_core::ccr::{CcrStore, backends::SqliteCcrStore, DEFAULT_TTL, DEFAULT_MAX_LIFETIME_MULTIPLIER};
use std::time::Instant;

fn main() {
    for (label, input) in [
        ("ordinary", "{\n  \"name\": \"alpha\", \"count\": 2\n}"),
        ("duplicate_keys", "{ \"key\": 1, \"key\": 2 }"),
        ("large_number", "{ \"n\": 123456789012345678901234567890, \"decimal\": 1.0 }"),
        ("escaped_unicode", "{ \"letter\": \"\\u0041\", \"astral\": \"\\ud83d\\ude00\" }"),
        ("malformed", "{ \"n\": [1, }"),
    ] {
        let result=JsonMinifier.apply(input);
        println!("json_case={label} input={input:?} result={result:?}");
    }
    let shell="rg\n/opt/homebrew/bin/ast-grep\n";
    let logs=LogCompressor::new(LogCompressorConfig::default());
    println!("log_shell={:?}", logs.compress(shell, 0.0));
    let db=std::env::args().nth(1).expect("pass scratch database path");
    let store=SqliteCcrStore::open(&db,1800).expect("open scratch db");
    store.put("valid", "raw original");
    println!("recovery_initial={:?}",store.get("valid"));
    let conn=rusqlite::Connection::open(&db).unwrap();
    conn.execute("DROP TABLE ccr_entries", []).unwrap();
    store.put("after_corruption", "this observation must not get a recovery marker");
    println!("recovery_after_schema_loss={:?} put_return_type=unit",store.get("after_corruption"));
    println!("default_idle_ttl_seconds={} absolute_lifetime_multiplier={}",DEFAULT_TTL.as_secs(),DEFAULT_MAX_LIFETIME_MULTIPLIER);
    let data="{\n  \"records\": [1,2,3], \"message\": \"hello\"\n}";
    let start=Instant::now();
    for _ in 0..10_000 { std::hint::black_box(JsonMinifier.apply(data).unwrap()); }
    println!("warm_10000_json_calls_us={} profile=debug fixture=47_bytes_not_task_latency",start.elapsed().as_micros());
}
