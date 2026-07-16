// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn disabled_alloc_facade_fast_paths_allocate_nothing() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    runtime.block_on(async {
        tokio::spawn(async {}).await.expect("warm direct spawn");
        jackin_telemetry::spawn::spawn_joined(async {})
            .await
            .expect("warm governed spawn");
    });
    let _profiler = dhat::Profiler::builder().testing().build();

    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .expect("registered event");
    let operation =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .expect("registered operation");
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .expect("registered counter");

    let stats = dhat::HeapStats::get();
    dhat::assert_eq!(stats.total_blocks, 0);
    dhat::assert_eq!(stats.total_bytes, 0);

    runtime.block_on(async {
        let before = dhat::HeapStats::get();
        tokio::spawn(async {}).await.expect("direct spawn");
        let direct = dhat::HeapStats::get().total_blocks - before.total_blocks;
        let before = dhat::HeapStats::get();
        jackin_telemetry::spawn::spawn_joined(async {})
            .await
            .expect("governed spawn");
        let governed = dhat::HeapStats::get().total_blocks - before.total_blocks;
        dhat::assert_eq!(governed, direct);
    });
}
