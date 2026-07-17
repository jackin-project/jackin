// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    contains_legacy_telemetry_name, event_runtime_severity, generate_rust_sources, repo_root,
    rust_pascal, source_policy_violations, source_policy_violations_for_files,
    validate_registry_matches_rust,
};

#[test]
fn source_policy_is_syntax_aware_and_blocks_raw_meters() {
    let path = "crates/example/src/lib.rs";
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
            "crates/jackin-otlp-testbed/src/lib.rs",
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
fn spawn_policy_resolves_cross_module_executor_aliases() {
    let files = [
        (
            "crates/example/src/executor.rs",
            "pub type Base = tokio::runtime::Handle;
             pub type Executor = Base;
             pub type PendingBase = tokio::task::JoinSet<()>;
             pub type Pending = PendingBase;",
        ),
        (
            "crates/example/src/worker.rs",
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
    let path = "crates/example/src/lib.rs";
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
    let path = "crates/example/src/lib.rs";
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
        source_policy_violations("crates/jackin-diagnostics/src/example.rs", allowed).is_empty()
    );

    for prohibited in [
        "std::fs::read_to_string(\"state\")",
        "state.lock()",
        "handle.block_on(work())",
        "handle.enter()",
        "socket.read(&mut bytes)",
    ] {
        let source = format!(
            "fn install(builder: Builder) {{ builder.with_callback(move |_observer| {{ let _ = {prohibited}; }}); }}"
        );
        assert_eq!(
            source_policy_violations("crates/jackin-diagnostics/src/example.rs", &source),
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
        source_policy_violations("crates/jackin-diagnostics/src/example.rs", indirect),
        ["observable callback performs blocking/runtime work"]
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
    let first = generate_rust_sources(&root).expect("registry must generate");
    let second = generate_rust_sources(&root).expect("registry must generate twice");
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
    let mut generated = generate_rust_sources(&root).expect("registry must generate");
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
fn namespace_scan_detects_telemetry_literals_without_flagging_identifiers() {
    let path = "fixture.rs";
    assert!(contains_legacy_telemetry_name(
        path,
        "const FIELD: &str = \"jackin.unregistered.field\";"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "record.insert(\"parallax.unregistered\", value);"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "pub const LABEL_ROLE_KEY: &str = \"jackin.role\";"
    ));
    assert!(!contains_legacy_telemetry_name(
        "crates/jackin-runtime/src/runtime/naming.rs",
        "pub const LABEL_ROLE_KEY: &str = \"jackin.role\";"
    ));
    assert!(contains_legacy_telemetry_name(
        "crates/jackin-runtime/src/runtime/naming.rs",
        "pub const DIFFERENT_SYMBOL: &str = \"jackin.role\";"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "attrs.get(\"jackin.unregistered\")"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "map.get(\"jackin.role\"); emit(\"jackin.unregistered\")"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "const LABEL_ROLE: &str = \"jackin.role\"; const FIELD: &str = \"parallax.bad\";"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "path.join(\"jackin.state\"); record(\"jackin.bad\")"
    ));
    let negative_fixture = "crates/jackin-otlp-testbed/src/tests.rs";
    assert!(!contains_legacy_telemetry_name(
        negative_fixture,
        "fn namespace_detector_rejects_synthetic_legacy_attribute() { let _ = \"jackin.synthetic\"; }"
    ));
    assert!(contains_legacy_telemetry_name(
        negative_fixture,
        "fn different_test() { let _ = \"jackin.synthetic\"; }"
    ));
}

#[test]
fn namespace_scan_handles_rust_literal_forms_and_macro_construction() {
    let path = "fixture.rs";
    assert!(contains_legacy_telemetry_name(
        path,
        r##"let _ = r#"jackin.raw.field"#;"##
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        r#"let _ = "jackin.\u{72}aw.field";"#
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        r#"let _ = b"parallax.byte.field";"#
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        r#"let _ = concat!("jackin.", "concat.field");"#
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "let _ = stringify!(parallax.stringify.field);"
    ));
    assert!(contains_legacy_telemetry_name(
        path,
        "let _ = \"jackin.multiline\\\n.field\";"
    ));
}
