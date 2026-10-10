// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn source_policy_is_syntax_aware_and_blocks_raw_meters() {
    let path = "crates/group/example/src/lib.rs";
    assert!(source_policy_violations(path, "// tokio::spawn(async {});").is_empty());
    assert!(
        source_policy_violations(path, "const TEXT: &str = \"provider.meter(\\\"x\\\")\";")
            .is_empty()
    );
    assert_eq!(
        source_policy_violations(
            path,
            "fn raw(provider: Provider) { let _ = provider.meter(\"x\"); }"
        ),
        ["raw OpenTelemetry meter construction"]
    );
    assert_eq!(
        source_policy_violations(path, "fn raw() { tokio::spawn(async {}); }"),
        ["unmanaged async/thread spawn"]
    );
    assert_eq!(
        source_policy_violations(
            "crates/testing/jackin-otlp-testbed/src/lib.rs",
            "fn raw() { tokio::spawn(async {}); }"
        ),
        ["unmanaged async/thread spawn"]
    );
    assert_eq!(
        source_policy_violations(path, "fn raw() { tracing::info!(\"raw\"); }"),
        ["raw tracing call outside governed facade"]
    );
}

#[test]
fn source_policy_resolves_raw_tracing_import_aliases() {
    let path = "crates/group/example/src/lib.rs";
    for source in [
        "use tracing as t; fn raw() { t::info!(\"event\"); }",
        "use tracing::info as emit; fn raw() { emit!(\"event\"); }",
        "use tracing::{info as emit}; fn raw() { emit!(\"event\"); }",
        "use tracing::*; fn raw() { info!(\"event\"); }",
        "use tracing as t; use t::info as emit; fn raw() { emit!(\"event\"); }",
        "use tracing::instrument as observe; #[observe] fn raw() {}",
        "use tracing as t; #[t::instrument] fn raw() {}",
        "use tracing::trace_span as scoped; fn raw() { let _span = scoped!(\"event\"); }",
    ] {
        assert_eq!(
            source_policy_violations(path, source),
            ["raw tracing call outside governed facade"],
            "{source}"
        );
    }
}

#[test]
fn source_policy_resolves_raw_meter_import_and_binding_aliases() {
    let path = "crates/group/example/src/lib.rs";
    for source in [
        "use opentelemetry::global as otel; fn raw() { let _ = otel::meter(\"raw\"); }",
        "use opentelemetry::global::meter as new_meter; fn raw() { let _ = new_meter(\"raw\"); }",
        "use opentelemetry::global::*; fn raw() { let _ = meter(\"raw\"); }",
        "use opentelemetry::*; fn raw() { let _ = global::meter; }",
        "use opentelemetry::global as otel; fn raw() { let new_meter = otel::meter; let _ = new_meter(\"raw\"); }",
        "use opentelemetry::global::meter as new_meter; const METER: fn(&str) = new_meter; fn raw() { let _ = METER(\"raw\"); }",
    ] {
        assert_eq!(
            source_policy_violations(path, source),
            ["raw OpenTelemetry meter construction"],
            "{source}"
        );
    }
    assert!(
        source_policy_violations(
            path,
            "mod global { pub fn meter() {} } fn safe() { let _ = global::meter; }"
        )
        .is_empty()
    );
}

#[test]
fn source_policy_inspects_opaque_macro_tokens_and_keeps_governed_paths() {
    let path = "crates/group/example/src/lib.rs";
    for source in [
        "macro_rules! hidden { () => { tracing::info!(\"raw\"); } }",
        "use tracing::info as emit; macro_rules! hidden { () => { emit!(\"raw\"); } }",
        "unknown!({ tracing::debug_span!(\"raw\"); });",
        "macro_rules! hidden { () => { ::tracing::info!(\"raw\"); } }",
        "macro_rules! hidden { () => { use tracing::info as emit; emit!(\"raw\"); } }",
        "macro_rules! hidden { () => { #[tracing::instrument] fn raw() {} } }",
        "macro_rules! hidden { () => { #[cfg_attr(feature = \"raw\", tracing::instrument)] fn raw() {} } }",
        "use tracing::instrument as observe; macro_rules! hidden { () => { #[cfg_attr(feature = \"raw\", observe)] fn raw() {} } }",
        "macro_rules! hidden { () => { ::diagnostics::telemetry_info!(\"raw\"); } }",
        "macro_rules! hidden { () => { let _ = ::opentelemetry::global::meter(\"raw\"); } }",
        "macro_rules! hidden { () => { diagnostics::telemetry_info!(\"raw\"); } }",
    ] {
        assert!(
            !source_policy_violations(path, source).is_empty(),
            "{source}"
        );
    }

    for source in [
        "unknown!(\"tracing::info!(not code)\");",
        "const TEXT: &str = \"tracing::info!(not code)\";",
        "use tracing::instrument as observe; unknown!([observe]);",
    ] {
        assert!(
            source_policy_violations(path, source).is_empty(),
            "{source}"
        );
    }

    assert!(
        source_policy_violations(
            "crates/services/jackin-telemetry/src/example.rs",
            "use tracing as t; macro_rules! facade { () => { t::info!(\"governed\"); } }"
        )
        .is_empty()
    );
    assert!(
        source_policy_violations(
            "crates/services/jackin-diagnostics/src/example.rs",
            "use opentelemetry::global as otel; fn facade() { let _ = otel::meter(\"governed\"); }"
        )
        .is_empty()
    );
}

#[test]
fn source_policy_blocks_legacy_and_generic_telemetry_macros_syntax_aware() {
    let path = "crates/group/example/src/lib.rs";
    for name in [
        "debug_log",
        "clog",
        "cdebug",
        "ctrace_payload",
        "cwarn",
        "cerror",
        "telemetry_info",
        "telemetry_debug",
        "telemetry_warn",
        "telemetry_error",
    ] {
        let source = format!("fn rejected() {{ diagnostics::{name}!(\"body\"); }}");
        assert_eq!(
            source_policy_violations(path, &source),
            ["prohibited legacy/generic telemetry macro"],
            "{name}"
        );

        let inert = format!(
            "// diagnostics::{name}!(\"comment\");\nconst TEXT: &str = \"{name}! in a string\";"
        );
        assert!(source_policy_violations(path, &inert).is_empty(), "{name}");
    }
}

#[test]
fn spawn_policy_resolves_cross_module_executor_aliases() {
    let files = [
        (
            "crates/group/example/src/executor.rs",
            "pub type Base = tokio::runtime::Handle;
             pub type Executor = Base;
             pub type PendingBase = tokio::task::JoinSet<()>;
             pub type Pending = PendingBase;",
        ),
        (
            "crates/group/example/src/worker.rs",
            "use crate::executor::{Executor as Runtime, Pending};
             fn raw(handle: Runtime, tasks: &mut Pending, qualified: crate::executor::Executor) {
                 handle.spawn(async {});
                 tasks.spawn(async {});
                 qualified.spawn(async {});
             }",
        ),
    ];
    assert_eq!(
        source_policy_violations_for_files(&files),
        [
            "unmanaged async/thread spawn",
            "unmanaged async/thread spawn",
            "unmanaged async/thread spawn"
        ]
    );
}

#[test]
fn spawn_policy_covers_executor_forms_without_matching_processes() {
    let path = "crates/group/example/src/lib.rs";
    for source in [
        "fn raw() { tokio :: task :: spawn (async {}); }",
        "fn raw() { tokio::task::spawn_local(async {}); }",
        "fn raw(handle: Handle) { handle.spawn(async {}); }",
        "fn raw(handle: Handle) { handle.spawn_blocking(|| {}); }",
        "fn raw() { let mut arbitrary = JoinSet::new(); arbitrary.spawn(async {}); }",
        "fn raw(local: LocalSet) { local.spawn_local(async {}); }",
        "fn raw() { std::thread::Builder::new().name(\"worker\".into()).spawn(|| {}); }",
        "fn raw() { std::thread::scope(|scope| { scope.spawn(|| {}); }); }",
        "use tokio::spawn as launch; fn raw() { launch(async {}); }",
        "fn raw() { let launch = tokio::spawn; launch(async {}); }",
        "use tokio::task as runner; fn raw() { runner::spawn(async {}); }",
        "use tokio::task as runner; fn raw() { let launch = runner::spawn; launch(async {}); }",
        "use std::thread as worker; fn raw() { worker::spawn(|| {}); }",
        "use tokio as executor; fn raw() { executor::spawn(async {}); }",
        "fn raw(arbitrary: tokio::runtime::Handle) { arbitrary.spawn(async {}); }",
        "fn raw(arbitrary: &mut tokio::task::JoinSet<()>) { arbitrary.spawn(async {}); }",
        "fn raw() { let arbitrary: tokio::runtime::Handle = make_handle(); arbitrary.spawn(async {}); }",
        "struct Pool { executor: tokio::runtime::Handle } fn raw(pool: &Pool) { pool.executor.spawn(async {}); }",
        "struct Pool { pending: tokio::task::JoinSet<()> } fn raw(pool: &mut Pool) { pool.pending.spawn(async {}); }",
        "fn make_executor() -> tokio::runtime::Handle { todo!() } fn raw() { make_executor().spawn(async {}); }",
        "struct Pool; impl Pool { fn pending(&self) -> tokio::task::JoinSet<()> { todo!() } fn raw(&self) { self.pending().spawn(async {}); } }",
        "use tokio::runtime::Handle as Executor; fn raw(arbitrary: Executor) { arbitrary.spawn(async {}); }",
        "use tokio::task::JoinSet as Tasks; struct Pool { pending: Tasks<()> } fn raw(pool: &mut Pool) { pool.pending.spawn(async {}); }",
        "type Executor = tokio::runtime::Handle; type Nested = Executor; fn raw() { let arbitrary: Nested = make_handle(); arbitrary.spawn(async {}); }",
        "type Tasks = tokio::task::JoinSet<()>; type Pending = Tasks; fn make_pending() -> Pending { todo!() } fn raw() { make_pending().spawn(async {}); }",
    ] {
        assert_eq!(
            source_policy_violations(path, source),
            ["unmanaged async/thread spawn"],
            "{source}"
        );
    }
    assert!(
        source_policy_violations(path, "fn child(mut command: Command) { command.spawn(); }")
            .is_empty()
    );
}

#[test]
fn async_scope_policy_rejects_guards_and_allows_sync_scopes() {
    let path = "crates/group/example/src/lib.rs";
    for source in [
        "async fn bad(span: Span) { let _guard = span.enter(); work().await; }",
        "fn bad(span: Span) { async move { let _guard = span.entered(); work().await; }; }",
        "async fn bad(context: Context) { let _guard = context.attach(); work().await; }",
        "async fn bad(context: Context) { let _guard: ContextGuard = context.attach(); work().await; }",
        "async fn bad(runtime_span: Span) { let _guard = runtime_span.enter(); work().await; }",
        "async fn bad(span: Span) { let _guard: tracing::span::Entered<'_> = helper(span); work().await; }",
        "async fn bad(span: Span) { let _guard: tracing::span::EnteredSpan = helper(span); work().await; }",
    ] {
        assert!(
            !source_policy_violations(path, source).is_empty(),
            "{source}"
        );
    }
    assert!(
        source_policy_violations(
            path,
            "async fn safe(runtime: tokio::runtime::Runtime, span: Span) { let _runtime = runtime.enter(); span.in_scope(|| sync_work()); work().await; }"
        )
        .is_empty()
    );
    assert!(
        source_policy_violations(
            path,
            "async fn safe(span: Span) { let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap(); let _runtime = runtime.enter(); span.in_scope(|| sync_work()); work().await; }"
        )
        .is_empty()
    );
}

#[test]
fn observable_callbacks_are_snapshot_only() {
    let allowed = r"fn install(builder: Builder, value: AtomicU64) {
        builder.with_callback(move |observer| observer.observe(value.load(Ordering::Relaxed), &[]));
    }";
    assert!(
        source_policy_violations("crates/services/jackin-diagnostics/src/example.rs", allowed)
            .is_empty()
    );

    for prohibited in [
        "std::fs::read_to_string(\"state\")",
        "state.lock()",
        "cache.refresh()",
        "state.snapshot()",
        "handle.block_on(work())",
        "handle.enter()",
        "socket.read(&mut bytes)",
        "std::println!(\"callback\")",
        "(hooks.sample)()",
    ] {
        let source = format!(
            "fn install(builder: Builder) {{ builder.with_callback(move |_observer| {{ let _ = {prohibited}; }}); }}"
        );
        assert_eq!(
            source_policy_violations("crates/services/jackin-diagnostics/src/example.rs", &source),
            ["observable callback performs blocking/runtime work"],
            "{prohibited}"
        );
    }
    let indirect = r#"
        fn sample_filesystem() { let _ = std::fs::read_to_string("state"); }
        fn install(builder: Builder) {
            builder.with_callback(move |_observer| sample_filesystem());
        }
    "#;
    assert_eq!(
        source_policy_violations(
            "crates/services/jackin-diagnostics/src/example.rs",
            indirect
        ),
        ["observable callback performs blocking/runtime work"]
    );
    for indirect_callback in [
        "fn install(builder: Builder) { let callback = move |_observer| std::fs::read_to_string(\"state\"); builder.with_callback(callback); }",
        "fn callback() -> impl Fn(&Observer) { move |_observer| std::fs::read_to_string(\"state\") } fn install(builder: Builder) { builder.with_callback(callback()); }",
    ] {
        assert_eq!(
            source_policy_violations(
                "crates/services/jackin-diagnostics/src/example.rs",
                indirect_callback
            ),
            ["observable callback must be an inline snapshot-only closure"]
        );
    }

    let runtime_metrics = r"fn install(builder: Builder, handle: Handle) {
        builder.with_callback(move |observer| {
            observer.observe(handle.metrics().num_workers() as u64, &[]);
            observer.observe(handle.metrics().num_alive_tasks() as u64, &[]);
            observer.observe(handle.metrics().global_queue_depth() as u64, &[]);
        });
    }";
    assert!(
        source_policy_violations(
            "crates/services/jackin-diagnostics/src/example.rs",
            runtime_metrics
        )
        .is_empty()
    );
}

#[test]
fn snapshot_callback_completes_without_blocking() {
    let value = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(42));
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let callback = || value.load(std::sync::atomic::Ordering::Relaxed);
        sent.send(callback()).expect("callback result receiver");
    });
    assert_eq!(
        received.recv_timeout(std::time::Duration::from_secs(1)),
        Ok(42)
    );
}

#[test]
fn registry_generation_is_deterministic_and_covers_dotted_commands() {
    let root = repo_root().expect("repository root must resolve");
    let Ok(first) = generate_rust_sources(&root) else {
        return;
    };
    let Ok(second) = generate_rust_sources(&root) else {
        return;
    };
    assert_eq!(first, second);
    validate_registry_matches_rust(&root, &first)
        .expect("checked-in telemetry schema must match generated output");
    let enums = first
        .iter()
        .find(|(path, _)| path.ends_with("schema/enums.rs"))
        .map(|(_, contents)| contents)
        .expect("enum output must exist");
    assert!(enums.contains("RoleValidate => \"role.validate\""));
    assert!(enums.contains("ConfigMountAdd => \"config.mount.add\""));
    let attrs = first
        .iter()
        .find(|(path, _)| path.ends_with("schema/attrs.rs"))
        .map(|(_, contents)| contents)
        .expect("attribute output must exist");
    assert!(attrs.contains("pub use opentelemetry_semantic_conventions::attribute::APP_CRASH_ID;"));
    assert!(attrs.contains("(APP_CRASH_ID, \"app.crash.id\"),"));
    assert!(
        attrs.contains(
            "pub use opentelemetry_semantic_conventions::attribute::APP_JANK_FRAME_COUNT;"
        )
    );
    assert!(attrs.contains("(APP_JANK_FRAME_COUNT, \"app.jank.frame_count\"),"));
    assert!(attrs.contains(
        "pub use std_attrs::{APP_JANK_FRAME_COUNT, APP_JANK_PERIOD, APP_JANK_THRESHOLD};"
    ));
    let events = first
        .iter()
        .find(|(path, _)| path.ends_with("schema/events.rs"))
        .map(|(_, contents)| contents)
        .expect("event output must exist");
    assert!(events.contains("app.crash.id:recommended"));
    assert!(events.contains("exception.stacktrace:recommended"));
    assert!(events.contains("app.jank.frame_count:recommended"));
    assert!(events.contains("severity: super::EventSeverity::Warn"));
    for path in ["event_defs.rs", "operation_defs.rs", "metric_defs.rs"] {
        let facade = first
            .iter()
            .find(|(candidate, _)| candidate.ends_with(path))
            .map_or_else(
                || panic!("{path} output must exist"),
                |(_, contents)| contents,
            );
        assert!(facade.contains("pub const ALL:"));
        assert!(facade.contains("::generated(&schema::"));
    }
    assert!(enums.contains("GlobalConfigSchemaVersion"));
    assert!(enums.contains("WorkspaceConfigSchemaVersion"));
    assert!(!enums.contains("bounded_values!(ConfigSchemaVersion"));
}

#[test]
fn checked_in_generation_rejects_single_byte_drift() {
    let root = repo_root().expect("repository root must resolve");
    let Ok(mut generated) = generate_rust_sources(&root) else {
        return;
    };
    generated[0].1.push(' ');
    assert!(validate_registry_matches_rust(&root, &generated).is_err());
}

#[test]
fn event_severity_registry_rejects_missing_and_unknown_values() {
    let valid: serde_yaml_ng::Value =
        serde_yaml_ng::from_str("name: example.event\nnote: runtime_severity=error\n")
            .expect("fixture parses");
    assert_eq!(event_runtime_severity(&valid).unwrap(), "error");
    for source in [
        "name: example.event\n",
        "name: example.event\nnote: runtime_severity=verbose\n",
    ] {
        let invalid = serde_yaml_ng::from_str(source).expect("fixture parses");
        event_runtime_severity(&invalid).unwrap_err();
    }
}

#[test]
fn rust_names_preserve_version_boundaries() {
    assert_eq!(
        rust_pascal("config.schema.v1alpha9"),
        "ConfigSchemaV1Alpha9"
    );
}

#[test]
fn ownership_census_extracts_static_and_dynamic_types() {
    assert_eq!(
        ownership_census::ownership_type(&[
            ".record_telemetry_error(",
            "schema::enums::ErrorType::DependencyCancelled,",
        ]),
        "DependencyCancelled"
    );
    assert_eq!(
        ownership_census::ownership_type(&["record_error(error_type)"]),
        "dynamic"
    );
}
