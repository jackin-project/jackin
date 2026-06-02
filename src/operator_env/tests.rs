//! Tests for `operator_env`: env resolution, `op://` parsing, layer merge,
//! launch diagnostics, and picker metadata deserialization.
use super::*;

static LAUNCH_DIAGNOSTIC_OUTPUT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Trust-model guard: if `OpField` ever grows a value-bearing field,
/// this exhaustive destructure breaks and forces a cache review.
#[test]
fn op_cache_does_not_store_field_values() {
    let mut cache = OpCache::default();
    cache.put_fields(
        None,
        "v1",
        "i1",
        vec![OpField {
            id: "f1".into(),
            label: "password".into(),
            field_type: "STRING".into(),
            concealed: true,
            reference: "op://v/i/f".into(),
        }],
    );
    for field in cache.get_fields(None, "v1", "i1").unwrap() {
        let OpField {
            id: _,
            label: _,
            field_type: _,
            concealed: _,
            reference: _,
        } = field;
    }
}

#[test]
fn op_section_id_slugifies_labels() {
    assert_eq!(op_section_id("My Creds!"), "my_creds");
    assert_eq!(op_section_id("Auth"), "auth");
    assert_eq!(op_section_id("a---b__c"), "a_b_c");
    assert_eq!(op_section_id("  leading"), "leading");
    assert_eq!(op_section_id("trailing  "), "trailing");
    assert_eq!(op_section_id("  "), "section");
    assert_eq!(op_section_id("!!"), "section");
    assert_eq!(op_section_id(""), "section");
}

#[test]
fn apply_field_edit_overwrite_by_id_preserves_section() {
    // Two fields share the label "token": one in a GUI-created section
    // with an opaque id, one in root. Editing by the sectioned field's
    // real id must overwrite THAT field and leave its section intact.
    let mut item = serde_json::json!({
        "fields": [
            { "id": "f_sectioned", "label": "token", "type": "CONCEALED",
              "value": "old", "section": { "id": "Section_opaque99" } },
            { "id": "f_root", "label": "token", "type": "CONCEALED", "value": "root" },
        ],
        "sections": [ { "id": "Section_opaque99", "label": "Auth" } ],
    });

    apply_field_edit(
        &mut item,
        &FieldTarget::Existing {
            id: "f_sectioned".into(),
            label: "token".into(),
        },
        "new",
        Some("Auth"),
    )
    .unwrap();

    let fields = item["fields"].as_array().unwrap();
    // Sectioned field overwritten, section id untouched (not re-slugged).
    assert_eq!(fields[0]["value"], "new");
    assert_eq!(fields[0]["section"]["id"], "Section_opaque99");
    // The same-labeled root field is left alone.
    assert_eq!(fields[1]["value"], "root");
    // No duplicate section created.
    assert_eq!(item["sections"].as_array().unwrap().len(), 1);
}

#[test]
fn apply_field_edit_stale_field_id_bails_without_mutating() {
    // The picker resolved a field id that no longer exists (renamed or
    // deleted out-of-band). Overwriting by id must error rather than
    // append a stray label-named field that the read-back would then miss.
    let mut item = serde_json::json!({
        "id": "i1",
        "fields": [
            { "id": "f_live", "label": "token", "type": "CONCEALED", "value": "old" },
        ],
    });
    let err = apply_field_edit(
        &mut item,
        &FieldTarget::Existing {
            id: "f_gone".into(),
            label: "token".into(),
        },
        "new",
        None,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("not found"),
        "stale field id must bail: {err}"
    );
    // No stray field appended; the live field is untouched.
    let fields = item["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 1, "no stray field appended");
    assert_eq!(fields[0]["value"], "old", "live field left unmodified");
}

#[test]
fn resolve_edited_field_ref_overwrite_matches_exact_id_not_label() {
    // Two fields share label "token"; the overwrite resolves the one
    // with the requested id, building the ref from its UUID.
    let updated = serde_json::json!({
        "id": "i1",
        "title": "Claude",
        "vault": { "id": "v1", "name": "Personal" },
        "fields": [
            { "id": "f_other", "label": "token" },
            { "id": "f_target", "label": "token" },
        ],
    });
    let r = resolve_edited_field_ref(
        &updated,
        &FieldTarget::Existing {
            id: "f_target".into(),
            label: "token".into(),
        },
        "v1",
        "i1",
        None,
    )
    .unwrap();
    assert_eq!(r.op, "op://v1/i1/f_target");
    assert_eq!(r.path, "Personal/Claude/token");
}

#[test]
fn resolve_edited_field_ref_append_falls_back_to_label() {
    let updated = serde_json::json!({
        "id": "i1",
        "title": "Claude",
        "vault": { "id": "v1", "name": "Personal" },
        "fields": [ { "id": "op_assigned_id", "label": "OAuth-Token" } ],
    });
    // Append path (field_id None) matches label case-insensitively.
    let r = resolve_edited_field_ref(
        &updated,
        &FieldTarget::New {
            label: "oauth-token".into(),
        },
        "v1",
        "i1",
        None,
    )
    .unwrap();
    assert_eq!(r.op, "op://v1/i1/op_assigned_id");
}

#[test]
fn resolve_edited_field_ref_missing_id_errors() {
    let updated = serde_json::json!({
        "id": "i1",
        "fields": [ { "id": "f_live", "label": "token" } ],
    });
    let err = resolve_edited_field_ref(
        &updated,
        &FieldTarget::Existing {
            id: "f_gone".into(),
            label: "token".into(),
        },
        "v1",
        "i1",
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("no field matching"), "{err}");
}

#[test]
fn apply_field_edit_append_places_new_field_in_section() {
    let mut item = serde_json::json!({ "fields": [] });
    apply_field_edit(
        &mut item,
        &FieldTarget::New {
            label: "oauth-token".into(),
        },
        "secret",
        Some("Creds"),
    )
    .unwrap();

    let field = &item["fields"].as_array().unwrap()[0];
    assert_eq!(field["label"], "oauth-token");
    assert_eq!(field["value"], "secret");
    assert_eq!(field["type"], "CONCEALED");
    assert_eq!(field["section"]["id"], "creds");
    // The section is registered on the item.
    let sections = item["sections"].as_array().unwrap();
    assert_eq!(sections[0]["id"], "creds");
    assert_eq!(sections[0]["label"], "Creds");
}

#[test]
fn apply_field_edit_overwrite_by_label_does_not_re_section() {
    // Append-by-label that collides with an existing sectioned field
    // overwrites it without re-parenting (section param is ignored on
    // overwrite).
    let mut item = serde_json::json!({
        "fields": [
            { "id": "f1", "label": "token", "type": "CONCEALED",
              "value": "old", "section": { "id": "Section_real" } },
        ],
    });
    apply_field_edit(
        &mut item,
        &FieldTarget::New {
            label: "token".into(),
        },
        "new",
        Some("Different"),
    )
    .unwrap();
    let field = &item["fields"].as_array().unwrap()[0];
    assert_eq!(field["value"], "new");
    assert_eq!(field["section"]["id"], "Section_real");
    // No "different" section was registered (overwrite path).
    assert!(item["sections"].as_array().is_none_or(Vec::is_empty));
}

#[test]
fn parse_op_reference_three_segments() {
    let parts = parse_op_reference("op://Vault/Item/field").unwrap();
    assert_eq!(parts.vault, "Vault");
    assert_eq!(parts.item, "Item");
    assert_eq!(parts.section, None);
    assert_eq!(parts.field, "field");
}

#[test]
fn parse_op_reference_handles_section_in_4_segment() {
    let parts = parse_op_reference("op://Personal/Item/Auth/password").unwrap();
    assert_eq!(parts.vault, "Personal");
    assert_eq!(parts.item, "Item");
    assert_eq!(parts.section, Some("Auth".to_string()));
    assert_eq!(parts.field, "password");
}

#[test]
fn parse_op_reference_strips_query_suffix() {
    // `resolve_op_uri_to_ref` can append a `?attribute=…` / `?ssh-format=…`
    // suffix to the final segment; it must not leak into the parsed field.
    let parts = parse_op_reference("op://Vault/Item/token?attribute=otp").unwrap();
    assert_eq!(parts.field, "token");
    assert_eq!(parts.section, None);

    let parts = parse_op_reference("op://Vault/Item/Auth/key?ssh-format=openssh").unwrap();
    assert_eq!(parts.section, Some("Auth".to_string()));
    assert_eq!(parts.field, "key");
}

#[test]
fn parse_op_reference_invalid_segment_count() {
    assert!(parse_op_reference("plain").is_none());
    assert!(parse_op_reference("op://only/two").is_none());
    assert!(parse_op_reference("op://a/b/c/d/e").is_none());
    assert!(parse_op_reference("op://").is_none());
    // Empty segments are malformed, not blank-named references.
    assert!(parse_op_reference("op:////").is_none());
    assert!(parse_op_reference("op://vault//field").is_none());
}

/// `OpReferenceParts::manual_delete_hint` is the canonical CLI
/// recovery shape surfaced in two error-message paths
/// (rotate-failure + orphan-cleanup-failure). Pinning the exact
/// rendered string here means a typo in the format string fails
/// at PR time.
#[test]
fn op_reference_parts_manual_delete_hint_renders_canonical_cli() {
    let parts = parse_op_reference("op://VAULT_UUID/ITEM_UUID/FIELD").unwrap();
    assert_eq!(
        parts.manual_delete_hint().to_string(),
        "op item delete ITEM_UUID --vault VAULT_UUID",
    );
}

struct TestOpRunner {
    response: std::cell::RefCell<Option<anyhow::Result<String>>>,
    last_ref: std::cell::RefCell<Option<String>>,
}

impl TestOpRunner {
    fn new(response: anyhow::Result<String>) -> Self {
        Self {
            response: std::cell::RefCell::new(Some(response)),
            last_ref: std::cell::RefCell::new(None),
        }
    }

    fn forbidden() -> Self {
        Self {
            response: std::cell::RefCell::new(None),
            last_ref: std::cell::RefCell::new(None),
        }
    }

    fn last_ref(&self) -> std::cell::Ref<'_, Option<String>> {
        self.last_ref.borrow()
    }
}

impl OpRunner for TestOpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        *self.last_ref.borrow_mut() = Some(reference.to_string());
        let Some(r) = self.response.borrow_mut().take() else {
            panic!("op CLI should not have been invoked");
        };
        r
    }
}

// ---- as_display_str tests --------------------------------------------

#[test]
fn env_value_as_display_str_returns_path_for_op_ref() {
    let v = EnvValue::OpRef(OpRef {
        op: "op://abc/def/fld".into(),
        path: "Private/Claude/auth".into(),
        account: None,
    });
    assert_eq!(v.as_display_str(), "Private/Claude/auth");
}

#[test]
fn env_value_as_display_str_returns_literal_for_plain() {
    let v = EnvValue::Plain("postgres://localhost".into());
    assert_eq!(v.as_display_str(), "postgres://localhost");
}

// ---- resolve_env_value dispatch tests --------------------------------

#[test]
fn dispatch_plain_returns_literal_unchanged() {
    let runner = TestOpRunner::forbidden();
    let v = EnvValue::Plain("hello".into());
    let r = resolve_env_value("test", "X", &v, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(r, "hello");
    assert!(
        runner.last_ref.borrow().is_none(),
        "no op call expected for Plain"
    );
}

#[test]
fn dispatch_plain_with_bare_op_uri_passes_through_literally() {
    let runner = TestOpRunner::forbidden();
    let v = EnvValue::Plain("op://Vault/Item/Field".into());
    let r = resolve_env_value("test", "X", &v, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(
        r, "op://Vault/Item/Field",
        "bare op:// in Plain must NOT be resolved; passes through to container"
    );
    assert!(
        runner.last_ref.borrow().is_none(),
        "no op call expected for Plain(op://...)"
    );
}

/// Regression pin: workspaces written before this branch have
/// `MY_VAR = "op://Vault/Item/Field"` as a scalar string — those
/// load as `EnvValue::Plain`. At runtime the resolver must pass
/// them through to the container as a literal string: no `op read`
/// call, no error.
#[test]
fn legacy_bare_op_uri_at_runtime_passes_through_literally() {
    let runner = TestOpRunner::forbidden(); // panics if read() is ever called
    let v = EnvValue::Plain("op://Vault/Item/Field".into());
    let r = resolve_env_value("test", "OLD", &v, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .expect("must succeed — Plain values never fail unless $VAR is unset");
    assert_eq!(
        r, "op://Vault/Item/Field",
        "Plain bare op:// must pass through literally, not be resolved",
    );
    assert!(
        runner.last_ref.borrow().is_none(),
        "no op read call must be made for Plain values, even op://-shaped ones",
    );
}

#[test]
fn dispatch_op_ref_calls_op_read_with_canonical_uri() {
    let runner = TestOpRunner::new(Ok("secret-value".to_string()));
    let v = EnvValue::OpRef(OpRef {
        op: "op://abc/def/fld".into(),
        path: "Vault/Item/Field".into(),
        account: None,
    });
    let r = resolve_env_value("test", "X", &v, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(r, "secret-value");
    assert_eq!(
        runner.last_ref().as_deref(),
        Some("op://abc/def/fld"),
        "must call op read with the canonical UUID URI"
    );
}

#[test]
fn dispatch_op_ref_failure_wraps_error_with_path() {
    let runner = TestOpRunner::new(Err(anyhow::anyhow!("not signed in")));
    let v = EnvValue::OpRef(OpRef {
        op: "op://abc/def/fld".into(),
        path: "Private/Claude/security/auth token".into(),
        account: None,
    });
    let err = resolve_env_value("workspace foo", "TOKEN", &v, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("workspace foo"), "msg: {msg}");
    assert!(msg.contains("TOKEN"), "msg: {msg}");
    assert!(
        msg.contains("Private/Claude/security/auth token"),
        "msg should reference path for the operator, not raw UUID URI: {msg}"
    );
    assert!(msg.contains("not signed in"), "msg: {msg}");
}

#[test]
fn op_cli_invokes_binary_and_returns_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\nif [ \"$1\" = \"read\" ] && [ \"$2\" = \"op://Personal/api/token\" ]; then printf '%s' 'tok-123'; exit 0; fi\nexit 99\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let out = runner.read("op://Personal/api/token").unwrap();
    assert_eq!(out, "tok-123");
}

#[test]
fn op_cli_strips_trailing_newline_from_op_read_output() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-newline");
    write_fake_op(&bin_path, "#!/bin/sh\nprintf 'tok-123\\n'\nexit 0\n");

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let out = runner.read("op://Personal/api/token").unwrap();
    assert_eq!(
        out, "tok-123",
        "trailing \\n from op read must be stripped; got {out:?}"
    );
}

/// A secret legitimately ending in `\n` (e.g. a PEM block) is sent
/// by `op read` as value+\n; strip exactly one so inner newlines
/// survive.
#[test]
fn op_cli_strips_only_one_trailing_newline_preserves_value_newline() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-double-newline");
    write_fake_op(&bin_path, "#!/bin/sh\nprintf 'line1\\nline2\\n'\nexit 0\n");

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let out = runner.read("op://Personal/api/multi").unwrap();
    assert_eq!(
        out, "line1\nline2",
        "internal newline must survive while final trailing \\n is stripped"
    );
}

#[test]
fn op_cli_missing_binary_returns_clear_error() {
    let runner = OpCli::with_binary("/nonexistent/op/binary/path".to_string());
    let err = runner.read("op://Personal/api/token").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("op"), "expected binary name in error: {msg}");
    assert!(
        msg.contains("not found")
            || msg.contains("No such file")
            || msg.contains("failed to spawn"),
        "expected a missing-binary hint in error: {msg}"
    );
}

#[test]
fn op_cli_nonzero_exit_propagates_stderr_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-fail");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\n>&2 echo 'item not found: op://Foo/bar'\nexit 1\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let err = runner.read("op://Foo/bar").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exit"), "expected exit code in error: {msg}");
    assert!(
        msg.contains("item not found"),
        "expected bounded stderr in error: {msg}"
    );
}

#[test]
fn op_cli_large_stderr_is_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-big-stderr");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\npython3 -c \"import sys; sys.stderr.write('X' * 16384)\" 2>&1 1>&2\nexit 1\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let err = runner.read("op://Foo/bar").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.len() < 6 * 1024,
        "expected bounded stderr in error; got {} bytes",
        msg.len()
    );
}

#[test]
fn op_cli_hanging_binary_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-hang");
    write_fake_op(&bin_path, "#!/bin/sh\nsleep 60\n");

    let runner = OpCli::with_binary_and_timeout(
        bin_path.to_string_lossy().to_string(),
        std::time::Duration::from_millis(250),
    );
    let start = std::time::Instant::now();
    let err = runner.read("op://Foo/bar").unwrap_err();
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "runner must abort before 5s; actual={elapsed:?}"
    );
    assert!(
        err.to_string().contains("timeout") || err.to_string().contains("timed out"),
        "expected timeout in error: {err}"
    );
}

#[test]
fn op_cli_probe_succeeds_when_binary_exists_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-version");
    write_fake_op(&bin_path, "#!/bin/sh\necho '2.30.0'\nexit 0\n");

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    runner.probe().unwrap();
}

#[cfg(unix)]
#[test]
fn op_cli_probe_times_out_when_binary_hangs() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-version-hang");
    write_fake_op(&bin_path, "#!/bin/sh\nsleep 60\n");

    let runner = OpCli::with_binary_and_timeout(
        bin_path.to_string_lossy().to_string(),
        std::time::Duration::from_millis(250),
    );
    let start = std::time::Instant::now();
    let err = runner.probe().unwrap_err();
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "probe must abort before 5s; actual={elapsed:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("timeout") || msg.contains("timed out"),
        "expected timeout in error: {msg}"
    );
}

#[test]
fn op_cli_probe_fails_with_install_link_when_binary_missing() {
    let runner = OpCli::with_binary("/nonexistent/op/binary/path".to_string());
    let err = runner.probe().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("1Password") || msg.contains("op"),
        "expected reference to op in error: {msg}"
    );
    assert!(
        msg.contains("developer.1password.com"),
        "expected install link in error: {msg}"
    );
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) {}

fn write_fake_op(path: &std::path::Path, script: &str) {
    use std::io::Write;
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(script.as_bytes()).unwrap();
    file.sync_all().unwrap();
    drop(file);
    make_executable(path);
}

#[test]
fn truncate_stderr_returns_input_for_short_string() {
    let s = "short error message";
    assert_eq!(truncate_stderr(s), s);
}

#[test]
fn truncate_stderr_truncates_long_ascii_at_boundary() {
    let s: String = "x".repeat(OP_STDERR_MAX + 100);
    let out = truncate_stderr(&s);
    assert!(
        out.starts_with(&s[..OP_STDERR_MAX]),
        "ASCII truncation must keep exactly OP_STDERR_MAX bytes"
    );
    assert!(out.ends_with("[truncated]"));
}

/// Multi-byte UTF-8 char straddling `OP_STDERR_MAX` — naive byte
/// slicing would panic; the boundary walk-back must round down.
#[test]
fn truncate_stderr_does_not_panic_on_utf8_boundary() {
    // ASCII padding + 4-byte emoji (`U+1F4A9`) straddling the cap.
    let pad_len = OP_STDERR_MAX - 2;
    let mut s = String::with_capacity(pad_len + 16);
    s.push_str(&"a".repeat(pad_len));
    for _ in 0..10 {
        s.push('\u{1F4A9}');
    }
    assert!(
        !s.is_char_boundary(OP_STDERR_MAX),
        "test fixture must place a non-boundary byte at OP_STDERR_MAX; \
             got is_char_boundary == true"
    );
    let out = truncate_stderr(&s);
    assert!(out.ends_with("[truncated]"));
    let head = out
        .strip_suffix("… [truncated]")
        .expect("truncate marker present");
    assert!(
        head.is_char_boundary(head.len()),
        "truncated head must end on a UTF-8 char boundary"
    );
    assert!(head.len() <= OP_STDERR_MAX);
}

use std::collections::BTreeMap;

fn m(pairs: &[(&str, &str)]) -> BTreeMap<String, EnvValue> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), EnvValue::Plain((*v).to_string())))
        .collect()
}

#[test]
fn merge_empty_layers_returns_empty() {
    let merged = merge_layers(&m(&[]), &m(&[]), &m(&[]), &m(&[]));
    assert!(merged.is_empty());
}

#[test]
fn merge_global_only() {
    let merged = merge_layers(&m(&[("A", "1"), ("B", "2")]), &m(&[]), &m(&[]), &m(&[]));
    assert_eq!(
        merged.get("A").map(super::EnvValue::as_persisted_str),
        Some("1")
    );
    assert_eq!(
        merged.get("B").map(super::EnvValue::as_persisted_str),
        Some("2")
    );
}

#[test]
fn merge_agent_overrides_global() {
    let merged = merge_layers(
        &m(&[("A", "global"), ("B", "global")]),
        &m(&[("B", "role")]),
        &m(&[]),
        &m(&[]),
    );
    assert_eq!(
        merged.get("A").map(super::EnvValue::as_persisted_str),
        Some("global")
    );
    assert_eq!(
        merged.get("B").map(super::EnvValue::as_persisted_str),
        Some("role")
    );
}

#[test]
fn merge_workspace_overrides_agent() {
    let merged = merge_layers(
        &m(&[("A", "global")]),
        &m(&[("A", "role")]),
        &m(&[("A", "workspace")]),
        &m(&[]),
    );
    assert_eq!(
        merged.get("A").map(super::EnvValue::as_persisted_str),
        Some("workspace")
    );
}

#[test]
fn merge_workspace_agent_overrides_workspace() {
    let merged = merge_layers(
        &m(&[("A", "global")]),
        &m(&[("A", "role")]),
        &m(&[("A", "workspace")]),
        &m(&[("A", "ws-role")]),
    );
    assert_eq!(
        merged.get("A").map(super::EnvValue::as_persisted_str),
        Some("ws-role")
    );
}

#[test]
fn merge_preserves_non_overlapping_keys_across_layers() {
    let merged = merge_layers(
        &m(&[("G", "g")]),
        &m(&[("A", "a")]),
        &m(&[("W", "w")]),
        &m(&[("X", "x")]),
    );
    assert_eq!(
        merged.get("G").map(super::EnvValue::as_persisted_str),
        Some("g")
    );
    assert_eq!(
        merged.get("A").map(super::EnvValue::as_persisted_str),
        Some("a")
    );
    assert_eq!(
        merged.get("W").map(super::EnvValue::as_persisted_str),
        Some("w")
    );
    assert_eq!(
        merged.get("X").map(super::EnvValue::as_persisted_str),
        Some("x")
    );
}

#[test]
fn validate_reserved_names_rejects_global_reserved() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env
        .insert("DOCKER_HOST".to_string(), "whatever".to_string().into());

    let err = validate_reserved_names(&cfg).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("DOCKER_HOST"), "{msg}");
    assert!(msg.contains("global [env]"), "{msg}");
    assert!(msg.contains("reserved"), "{msg}");
}

#[test]
fn validate_reserved_names_rejects_per_agent_reserved() {
    let mut cfg = crate::config::AppConfig::default();
    let mut role = crate::config::RoleSource {
        git: "https://example.com/x.git".to_string(),
        trusted: true,
        env: std::collections::BTreeMap::new(),
    };
    role.env
        .insert("JACKIN".to_string(), "whatever".to_string().into());
    cfg.roles.insert("agent-smith".to_string(), role);

    let err = validate_reserved_names(&cfg).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("JACKIN"), "{msg}");
    assert!(msg.contains("role \"agent-smith\""), "{msg}");
}

#[test]
fn validate_reserved_names_rejects_per_workspace_reserved() {
    let mut cfg = crate::config::AppConfig::default();
    let mut ws = crate::workspace::WorkspaceConfig {
        workdir: "/x".to_string(),
        mounts: vec![crate::workspace::MountConfig {
            src: "/x".to_string(),
            dst: "/x".to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    ws.env
        .insert("DOCKER_TLS_VERIFY".to_string(), "0".to_string().into());
    cfg.workspaces.insert("big-monorepo".to_string(), ws);

    let err = validate_reserved_names(&cfg).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("DOCKER_TLS_VERIFY"), "{msg}");
    assert!(msg.contains("workspace \"big-monorepo\""), "{msg}");
}

#[test]
fn validate_reserved_names_rejects_workspace_agent_override_reserved() {
    let mut cfg = crate::config::AppConfig::default();
    let mut override_ = crate::workspace::WorkspaceRoleOverride::default();
    override_
        .env
        .insert("DOCKER_CERT_PATH".to_string(), "/tmp".to_string().into());
    let mut ws = crate::workspace::WorkspaceConfig {
        workdir: "/x".to_string(),
        mounts: vec![crate::workspace::MountConfig {
            src: "/x".to_string(),
            dst: "/x".to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    ws.roles.insert("agent-smith".to_string(), override_);
    cfg.workspaces.insert("big-monorepo".to_string(), ws);

    let err = validate_reserved_names(&cfg).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("DOCKER_CERT_PATH"), "{msg}");
    assert!(
        msg.contains("workspace \"big-monorepo\"") && msg.contains("role \"agent-smith\""),
        "{msg}"
    );
}

#[test]
fn validate_reserved_names_reports_all_conflicts_in_one_error() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env
        .insert("DOCKER_HOST".to_string(), "x".to_string().into());
    cfg.env
        .insert("DOCKER_TLS_VERIFY".to_string(), "y".to_string().into());

    let err = validate_reserved_names(&cfg).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("DOCKER_HOST"), "{msg}");
    assert!(msg.contains("DOCKER_TLS_VERIFY"), "{msg}");
}

#[test]
fn validate_reserved_names_accepts_non_reserved() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env
        .insert("MY_VAR".to_string(), "value".to_string().into());
    cfg.env
        .insert("OPERATOR_TOKEN".to_string(), "op://...".to_string().into());

    validate_reserved_names(&cfg).unwrap();
}

#[test]
fn resolve_empty_config_returns_empty_map() {
    let cfg = crate::config::AppConfig::default();
    let resolved = resolve_operator_env_with(&cfg, None, None, &TestOpRunner::forbidden(), |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert!(resolved.is_empty());
}

#[test]
fn resolve_global_literal_value() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env.insert("FOO".to_string(), "bar".to_string().into());
    let resolved = resolve_operator_env_with(&cfg, None, None, &TestOpRunner::forbidden(), |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(
        resolved.get("FOO").map(std::string::String::as_str),
        Some("bar")
    );
}

#[test]
fn resolve_layers_apply_in_order_with_workspace_agent_winning() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env.insert("X".to_string(), "global".to_string().into());

    let mut role_source = crate::config::RoleSource {
        git: "https://example.com/x.git".to_string(),
        trusted: true,
        env: std::collections::BTreeMap::new(),
    };
    role_source
        .env
        .insert("X".to_string(), "role".to_string().into());
    cfg.roles.insert("agent-smith".to_string(), role_source);

    let mut ws = crate::workspace::WorkspaceConfig {
        workdir: "/x".to_string(),
        mounts: vec![crate::workspace::MountConfig {
            src: "/x".to_string(),
            dst: "/x".to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    ws.env
        .insert("X".to_string(), "workspace".to_string().into());
    let mut wsa = crate::workspace::WorkspaceRoleOverride::default();
    wsa.env
        .insert("X".to_string(), "ws-role".to_string().into());
    ws.roles.insert("agent-smith".to_string(), wsa);
    cfg.workspaces.insert("big-monorepo".to_string(), ws);

    let resolved = resolve_operator_env_with(
        &cfg,
        Some("agent-smith"),
        Some("big-monorepo"),
        &TestOpRunner::forbidden(),
        |_| Err(std::env::VarError::NotPresent),
    )
    .unwrap();

    assert_eq!(
        resolved.get("X").map(std::string::String::as_str),
        Some("ws-role")
    );
}

#[test]
fn resolve_reports_all_failures_in_one_error() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env
        .insert("A".to_string(), "$MISSING_A".to_string().into());
    cfg.env
        .insert("B".to_string(), "$MISSING_B".to_string().into());

    let err = resolve_operator_env_with(&cfg, None, None, &TestOpRunner::forbidden(), |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("\"A\""), "{msg}");
    assert!(msg.contains("\"B\""), "{msg}");
    assert!(msg.contains("MISSING_A"), "{msg}");
    assert!(msg.contains("MISSING_B"), "{msg}");
}

#[test]
fn resolve_probes_op_cli_once_when_any_op_ref_present() {
    struct ProbeCountingRunner {
        probe_calls: std::cell::Cell<u32>,
        read_calls: std::cell::Cell<u32>,
    }
    impl OpRunner for ProbeCountingRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            self.read_calls.set(self.read_calls.get() + 1);
            Ok("stub".into())
        }
        fn probe(&self) -> anyhow::Result<()> {
            self.probe_calls.set(self.probe_calls.get() + 1);
            Ok(())
        }
    }

    let mut cfg = crate::config::AppConfig::default();
    cfg.env.insert(
        "A".to_string(),
        EnvValue::OpRef(OpRef {
            op: "op://abc-vault/abc-item/field-a".to_string(),
            path: "Personal/ItemA/field-a".to_string(),
            account: None,
        }),
    );
    cfg.env.insert(
        "B".to_string(),
        EnvValue::OpRef(OpRef {
            op: "op://abc-vault/abc-item/field-b".to_string(),
            path: "Personal/ItemA/field-b".to_string(),
            account: None,
        }),
    );
    let runner = ProbeCountingRunner {
        probe_calls: std::cell::Cell::new(0),
        read_calls: std::cell::Cell::new(0),
    };
    resolve_operator_env_with(&cfg, None, None, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(runner.probe_calls.get(), 1, "probe must fire exactly once");
    assert_eq!(runner.read_calls.get(), 2, "each OpRef key is resolved");
}

#[test]
fn resolve_skips_probe_when_no_op_refs_present() {
    struct ProbeCountingRunner {
        probe_calls: std::cell::Cell<u32>,
    }
    impl OpRunner for ProbeCountingRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            panic!("read must not be called when no op:// refs exist")
        }
        fn probe(&self) -> anyhow::Result<()> {
            self.probe_calls.set(self.probe_calls.get() + 1);
            Ok(())
        }
    }

    let mut cfg = crate::config::AppConfig::default();
    cfg.env
        .insert("A".to_string(), "literal".to_string().into());
    let runner = ProbeCountingRunner {
        probe_calls: std::cell::Cell::new(0),
    };
    resolve_operator_env_with(&cfg, None, None, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(
        runner.probe_calls.get(),
        0,
        "probe must not fire when no op:// refs exist"
    );
}

#[test]
fn resolve_probe_failure_surfaces_install_link_once() {
    struct FailingProbeRunner;
    impl OpRunner for FailingProbeRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            panic!("read must not be called when probe fails")
        }
        fn probe(&self) -> anyhow::Result<()> {
            anyhow::bail!(
                "1Password CLI (\"op\") was not found on PATH — install from \
                     https://developer.1password.com/docs/cli/"
            )
        }
    }

    let mut cfg = crate::config::AppConfig::default();
    cfg.env.insert(
        "A".to_string(),
        EnvValue::OpRef(OpRef {
            op: "op://abc-vault/abc-item/field-a".to_string(),
            path: "Personal/ItemA/field-a".to_string(),
            account: None,
        }),
    );
    cfg.env.insert(
        "B".to_string(),
        EnvValue::OpRef(OpRef {
            op: "op://abc-vault/abc-item/field-b".to_string(),
            path: "Personal/ItemA/field-b".to_string(),
            account: None,
        }),
    );
    let err = resolve_operator_env_with(&cfg, None, None, &FailingProbeRunner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("developer.1password.com"),
        "expected install link once: {msg}"
    );
}

#[test]
fn resolve_op_failure_includes_layer_and_key() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env.insert(
        "TOKEN".to_string(),
        EnvValue::OpRef(OpRef {
            op: "op://abc-vault/abc-item/token".to_string(),
            path: "Personal/BrokenItem/token".to_string(),
            account: None,
        }),
    );

    let runner = TestOpRunner::new(Err(anyhow::anyhow!("item not found")));

    let err = resolve_operator_env_with(&cfg, None, None, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("TOKEN"), "{msg}");
    // Error references the human-readable path, not the raw UUID URI.
    assert!(msg.contains("Personal/BrokenItem/token"), "{msg}");
    assert!(msg.contains("global [env]"), "{msg}");
}

#[test]
fn resolve_host_ref_success_returns_value() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env.insert(
        "API_KEY".to_string(),
        "${MY_HOST_API_KEY}".to_string().into(),
    );

    let resolved =
        resolve_operator_env_with(&cfg, None, None, &TestOpRunner::forbidden(), |name| {
            if name == "MY_HOST_API_KEY" {
                Ok("host-secret".to_string())
            } else {
                Err(std::env::VarError::NotPresent)
            }
        })
        .unwrap();

    assert_eq!(
        resolved.get("API_KEY").map(std::string::String::as_str),
        Some("host-secret")
    );
}

#[test]
fn launch_diagnostic_normal_mode_prints_counts_only_no_values() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env
        .insert("LITERAL_KEY".to_string(), "super-secret".to_string().into());
    cfg.env
        .insert("HOST_KEY".to_string(), "$HOST_VAR".to_string().into());
    cfg.env.insert(
        "OP_KEY".to_string(),
        EnvValue::OpRef(OpRef {
            op: "op://abc-vault/abc-item/field".to_string(),
            path: "Personal/item/field".to_string(),
            account: None,
        }),
    );
    let resolved: std::collections::BTreeMap<String, String> = [
        ("LITERAL_KEY".to_string(), "super-secret".to_string()),
        ("HOST_KEY".to_string(), "host-value-secret".to_string()),
        ("OP_KEY".to_string(), "op-value-secret".to_string()),
    ]
    .into_iter()
    .collect();

    let rendered = format_launch_diagnostic_for_test(&cfg, None, None, &resolved, false);

    assert!(rendered.contains("3 resolved"), "{rendered}");
    assert!(rendered.contains("1 op://"), "{rendered}");
    assert!(rendered.contains("1 host ref"), "{rendered}");
    assert!(rendered.contains("1 literal"), "{rendered}");

    // Values must never appear under any mode.
    assert!(!rendered.contains("super-secret"), "{rendered}");
    assert!(!rendered.contains("host-value-secret"), "{rendered}");
    assert!(!rendered.contains("op-value-secret"), "{rendered}");

    // In normal mode, references are NOT emitted (only counts).
    assert!(!rendered.contains("$HOST_VAR"), "{rendered}");
    assert!(!rendered.contains("op://Personal/item/field"), "{rendered}");
}

#[test]
fn launch_diagnostic_debug_mode_prints_references_but_not_values() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env
        .insert("LITERAL_KEY".to_string(), "super-secret".to_string().into());
    cfg.env.insert(
        "OP_KEY".to_string(),
        EnvValue::OpRef(OpRef {
            op: "op://abc-vault/abc-item/field".to_string(),
            path: "Personal/item/field".to_string(),
            account: None,
        }),
    );
    let resolved: std::collections::BTreeMap<String, String> = [
        ("LITERAL_KEY".to_string(), "super-secret".to_string()),
        ("OP_KEY".to_string(), "op-value-secret".to_string()),
    ]
    .into_iter()
    .collect();

    let rendered = format_launch_diagnostic_for_test(&cfg, None, None, &resolved, true);

    // Debug mode emits the canonical op URI (config, not secret)
    // and the "literal" label — never the resolved value.
    assert!(
        rendered.contains("op://abc-vault/abc-item/field"),
        "{rendered}"
    );
    assert!(rendered.contains("literal"), "{rendered}");
    assert!(!rendered.contains("super-secret"), "{rendered}");
    assert!(!rendered.contains("op-value-secret"), "{rendered}");
}

#[test]
fn launch_diagnostic_routes_to_run_file_while_rich_surface_is_active() {
    let _lock = LAUNCH_DIAGNOSTIC_OUTPUT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    crate::tui::set_rich_surface_active(false);
    let tmp = tempfile::tempdir().unwrap();
    let paths = crate::paths::JackinPaths::for_tests(tmp.path());
    let run = crate::diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    crate::tui::set_rich_surface_active(true);
    let mut stderr = Vec::new();
    emit_launch_diagnostic(
        "[jackin] operator env: 1 resolved (1 op://, 0 host ref, 0 literal)\n",
        false,
        &mut stderr,
    );
    crate::tui::set_rich_surface_active(false);

    assert!(
        stderr.is_empty(),
        "rich launch surface must not receive stderr diagnostics"
    );
    let jsonl = std::fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("\"kind\":\"operator_env\""), "{jsonl}");
    assert!(jsonl.contains("1 resolved"), "{jsonl}");
}

#[test]
fn launch_diagnostic_debug_mode_routes_to_run_file_not_stderr() {
    let _lock = LAUNCH_DIAGNOSTIC_OUTPUT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    crate::tui::set_rich_surface_active(false);
    let tmp = tempfile::tempdir().unwrap();
    let paths = crate::paths::JackinPaths::for_tests(tmp.path());
    let run = crate::diagnostics::RunDiagnostics::start(&paths, true, "load").unwrap();
    let _active = run.activate();

    let mut stderr = Vec::new();
    emit_launch_diagnostic(
        "[jackin] operator env:\n  TOKEN  op://...  (global)\n",
        true,
        &mut stderr,
    );

    assert!(
        stderr.is_empty(),
        "debug launch diagnostics belong in the run file"
    );
    let jsonl = std::fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("\"kind\":\"operator_env\""), "{jsonl}");
    assert!(jsonl.contains("TOKEN"), "{jsonl}");
}

// ---- OpStructRunner tests --------------------------------------------

#[test]
fn op_field_concealed_derives_from_type_or_purpose() {
    // Type CONCEALED -> concealed=true.
    let raw_concealed = RawOpField {
        id: "f1".to_string(),
        label: "password".to_string(),
        field_type: "CONCEALED".to_string(),
        purpose: String::new(),
        reference: String::new(),
    };
    assert!(OpField::from(raw_concealed).concealed);

    // Purpose PASSWORD -> concealed=true, even when type is empty.
    let raw_purpose = RawOpField {
        id: "f2".to_string(),
        label: "pw".to_string(),
        field_type: String::new(),
        purpose: "PASSWORD".to_string(),
        reference: String::new(),
    };
    assert!(OpField::from(raw_purpose).concealed);

    // Plain text field -> concealed=false.
    let raw_text = RawOpField {
        id: "f3".to_string(),
        label: "username".to_string(),
        field_type: "STRING".to_string(),
        purpose: "USERNAME".to_string(),
        reference: String::new(),
    };
    assert!(!OpField::from(raw_text).concealed);
}

#[cfg(unix)]
#[test]
fn op_struct_runner_vault_list_parses_json() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-vault-list");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\nif [ \"$1\" = \"vault\" ] && [ \"$2\" = \"list\" ]; then \
             printf '%s' '[{\"id\":\"v1\",\"name\":\"Personal\"}]'; exit 0; fi\nexit 99\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let vaults = runner.vault_list(None).unwrap();
    assert_eq!(
        vaults,
        vec![OpVault {
            id: "v1".to_string(),
            name: "Personal".to_string(),
        }]
    );
}

#[cfg(unix)]
#[test]
fn op_struct_runner_item_list_parses_json() {
    // Two items are returned: the first carries an
    // `additional_information` subtitle (the username/email 1Password
    // surfaces in its UI), the second omits it. Both must round-trip
    // — the first into a populated `subtitle`, the second into an
    // empty string via `#[serde(default)]`.
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-item-list");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\nif [ \"$1\" = \"item\" ] && [ \"$2\" = \"list\" ]; then \
             printf '%s' '[{\"id\":\"i1\",\"title\":\"Google\",\"additional_information\":\"alexey@zhokhov.com\"},\
{\"id\":\"i2\",\"title\":\"API Keys\"}]'; exit 0; fi\nexit 99\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let items = runner.item_list("v1", None).unwrap();
    assert_eq!(
        items,
        vec![
            OpItem {
                id: "i1".to_string(),
                name: "Google".to_string(),
                subtitle: "alexey@zhokhov.com".to_string(),
            },
            OpItem {
                id: "i2".to_string(),
                name: "API Keys".to_string(),
                subtitle: String::new(),
            },
        ]
    );
}

#[cfg(unix)]
#[test]
fn op_struct_runner_item_list_handles_missing_additional_information() {
    // Items without `additional_information` (e.g., secure notes)
    // must deserialize cleanly with an empty `subtitle`. Regression
    // coverage for the `#[serde(default)]` on `RawOpItem`.
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-item-list-no-subtitle");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\nif [ \"$1\" = \"item\" ] && [ \"$2\" = \"list\" ]; then \
             printf '%s' '[{\"id\":\"i1\",\"title\":\"Recovery codes\"}]'; exit 0; fi\nexit 99\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let items = runner.item_list("v1", None).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "Recovery codes");
    assert_eq!(items[0].subtitle, "");
}

#[cfg(unix)]
#[test]
fn op_struct_runner_item_get_parses_fields_no_value() {
    // The crucial safety test: even when `op item get` JSON contains
    // a `value` key on each field, our `RawOpField` struct does not
    // declare it, so serde silently drops the value during
    // deserialization. The resulting `OpField` has no value field
    // at all (the type itself doesn't have one).
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-item-get");
    let json = r#"{"id":"i1","title":"API Keys","fields":[
            {"id":"username","label":"username","type":"STRING","purpose":"USERNAME","value":"alice","reference":"op://Personal/API Keys/username"},
            {"id":"password","label":"password","type":"CONCEALED","purpose":"PASSWORD","value":"super-secret","reference":"op://Personal/API Keys/password"}
        ]}"#;
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"item\" ] && [ \"$2\" = \"get\" ]; then \
             cat <<'JSON'\n{json}\nJSON\nexit 0; fi\nexit 99\n"
    );
    write_fake_op(&bin_path, &script);

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let fields = runner.item_get("i1", "v1", None).unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].label, "username");
    assert!(!fields[0].concealed);
    assert_eq!(fields[1].label, "password");
    assert!(fields[1].concealed);
    // Compile-time guarantee: OpField has no `value` field. If a
    // future refactor adds one, this struct-match will fail to
    // compile and force an explicit re-review of the trust model.
    // The destructure also names `reference` — drop it from
    // `OpField` and this fails to compile, forcing a re-review of
    // the picker's commit path (which depends on `reference`
    // being the authoritative `op://` string from the CLI rather
    // than a synthesized one).
    let OpField {
        id: _,
        label: _,
        field_type: _,
        concealed: _,
        reference: _,
    } = fields[1].clone();
}

/// `op item get --format json` emits a `reference` key on every
/// field carrying the authoritative `op://...` string. The picker
/// commits this verbatim instead of synthesizing a path from
/// display names, so verify it round-trips into `OpField`.
#[cfg(unix)]
#[test]
fn op_struct_runner_item_get_captures_reference_field() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-item-get-reference");
    // `auth/secret_key` is the sectioned shape: 4-segment reference
    // where the 3rd segment is a section name. The picker must be
    // able to commit this verbatim.
    let json = r#"{"id":"i1","title":"X","fields":[
            {"id":"f1","label":"top","type":"STRING","reference":"op://X/Y/Z"},
            {"id":"f2","label":"key","type":"CONCEALED","reference":"op://Personal/API Keys/auth/secret_key"}
        ]}"#;
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"item\" ] && [ \"$2\" = \"get\" ]; then \
             cat <<'JSON'\n{json}\nJSON\nexit 0; fi\nexit 99\n"
    );
    write_fake_op(&bin_path, &script);

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let fields = runner.item_get("i1", "v1", None).unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].reference, "op://X/Y/Z");
    assert_eq!(
        fields[1].reference,
        "op://Personal/API Keys/auth/secret_key"
    );
}

#[cfg(unix)]
#[test]
fn op_struct_runner_account_list_parses_real_op_output() {
    // Captured from `op account list --format json` against op CLI v2.x.
    // The actual key is `account_uuid`, not `id` — verify our serde
    // alias makes both shapes parse.
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-accounts");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\ncat <<'EOF'\n[\n  {\n    \"url\": \"example.1password.com\",\n    \"email\": \"someone@example.com\",\n    \"user_uuid\": \"USERUUIDXXXX\",\n    \"account_uuid\": \"ACCTUUIDYYYY\"\n  }\n]\nEOF\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let accounts = runner
        .account_list()
        .expect("real op account list output must parse");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "ACCTUUIDYYYY");
    // email + url round-trip from the realistic JSON fixture so the
    // picker's Account pane has the human-readable display string.
    assert_eq!(accounts[0].email, "someone@example.com");
    assert_eq!(accounts[0].url, "example.1password.com");
}

#[cfg(unix)]
#[test]
fn op_struct_runner_threads_account_flag_to_op_cli() {
    // The fake `op` shim echoes its argv to stdout when invoked. We
    // assert that passing `Some(account_uuid)` to vault_list produces
    // an `--account ACCT123` pair in the spawned argv. JSON output
    // is the empty array so deserialization succeeds.
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-account-flag");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\necho \"$@\" >&2\nprintf '%s' '[]'\nexit 0\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    // With Some(_) → must include `--account <id>` in argv.
    let _ = runner.vault_list(Some("ACCT123")).unwrap();
    // With None → must NOT include `--account` in argv.
    let _ = runner.vault_list(None).unwrap();
    // (Argv is echoed to stderr, which run_op_json drains but does
    // not return on success. Concrete argv-ordering coverage is in
    // the picker integration test that uses an inspectable stub.
    // This test verifies both code paths return Ok without panicking
    // — i.e., the args slice is well-formed in both branches.)
}

#[test]
fn op_struct_runner_signed_out_detection() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("fake-op-signed-out");
    write_fake_op(
        &bin_path,
        "#!/bin/sh\n>&2 echo 'You are not currently signed in. Run `op signin`.'\nexit 1\n",
    );

    let runner = OpCli::with_binary(bin_path.to_string_lossy().to_string());
    let err = runner.vault_list(None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("not signed in") || msg.contains("op signin"),
        "expected signed-out detection in error: {msg}"
    );
}

#[test]
fn env_value_round_trip_through_toml() {
    use std::collections::BTreeMap;

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Wrap {
        env: BTreeMap<String, EnvValue>,
    }

    let toml_in = r#"
[env]
PLAIN = "literal-value"
HOST_VAR = "${HOME}"
LEGACY = "op://Vault/Item/Field"
PINNED = { op = "op://abc/def/fld", path = "Vault/Item/Field" }
PINNED_AMBIG = { op = "op://abc/def/fld", path = "Vault/Item[sub]/Field" }
"#;
    let parsed: Wrap = toml::from_str(toml_in).unwrap();
    assert_eq!(
        parsed.env.get("PLAIN"),
        Some(&EnvValue::Plain("literal-value".into()))
    );
    assert_eq!(
        parsed.env.get("HOST_VAR"),
        Some(&EnvValue::Plain("${HOME}".into()))
    );
    assert_eq!(
        parsed.env.get("LEGACY"),
        Some(&EnvValue::Plain("op://Vault/Item/Field".into()))
    );
    assert_eq!(
        parsed.env.get("PINNED"),
        Some(&EnvValue::OpRef(OpRef {
            op: "op://abc/def/fld".into(),
            path: "Vault/Item/Field".into(),
            account: None,
        }))
    );

    // Round-trip back to TOML and re-parse must produce the same map.
    let serialized = toml::to_string(&parsed).unwrap();
    let reparsed: Wrap = toml::from_str(&serialized).unwrap();
    assert_eq!(parsed, reparsed);
}

#[test]
fn op_ref_rejects_unknown_fields_in_inline_table() {
    #[derive(serde::Deserialize)]
    struct Wrap {
        #[allow(dead_code)] // exercised via deserialization, never read in this negative test
        env: std::collections::BTreeMap<String, EnvValue>,
    }

    // Typo'd inline table: "paht" instead of "path". Should fail
    // with a clear "unknown field" error, not silently produce an
    // OpRef with empty path.
    let toml_in = r#"
[env]
TOKEN = { op = "op://abc/def/fld", path = "Vault/Item/Field", paht = "stray" }
"#;
    let result: Result<Wrap, _> = toml::from_str(toml_in);
    let err = result
        .err()
        .expect("deny_unknown_fields must reject `paht`");
    let err_msg = format!("{err}");
    // Either an "unknown field" error or a fall-through-to-Plain failure
    // (because OpRef rejected; Plain expects scalar string, not table).
    // The important thing is it doesn't silently accept the OpRef shape.
    assert!(
        err_msg.contains("unknown field")
            || err_msg.contains("paht")
            || err_msg.contains("invalid type"),
        "expected unknown-field or invalid-type error; got: {err_msg}"
    );
}

// ---- resolve_op_uri_to_ref tests ------------------------------------

/// Minimal stub for `OpStructRunner` used by `resolve_op_uri_to_ref` unit
/// tests. Supports builder methods (`with_vault`, `with_item`, `with_field`)
/// and covers only the synchronous path used by the CLI resolver.
struct StubOpStructRunner {
    vaults: Vec<OpVault>,
    /// (`vault_id` → items)
    items: std::collections::HashMap<String, Vec<OpItem>>,
    /// (`item_id` → fields)
    fields: std::collections::HashMap<String, Vec<OpField>>,
}

impl Default for StubOpStructRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl StubOpStructRunner {
    fn new() -> Self {
        Self {
            vaults: Vec::new(),
            items: std::collections::HashMap::new(),
            fields: std::collections::HashMap::new(),
        }
    }

    fn with_vault(mut self, name: &str, id: &str) -> Self {
        self.vaults.push(OpVault {
            id: id.to_string(),
            name: name.to_string(),
        });
        self
    }

    fn with_item(mut self, vault_id: &str, name: &str, id: &str, subtitle: &str) -> Self {
        self.items
            .entry(vault_id.to_string())
            .or_default()
            .push(OpItem {
                id: id.to_string(),
                name: name.to_string(),
                subtitle: subtitle.to_string(),
            });
        self
    }

    fn with_field(mut self, item_id: &str, label: &str, id: &str, concealed: bool) -> Self {
        self.fields
            .entry(item_id.to_string())
            .or_default()
            .push(OpField {
                id: id.to_string(),
                label: label.to_string(),
                field_type: if concealed {
                    "CONCEALED".into()
                } else {
                    "STRING".into()
                },
                concealed,
                reference: String::new(),
            });
        self
    }

    /// Like `with_field`, but allows specifying the `reference` string
    /// (1Password's canonical op:// reference) explicitly. Used by tests
    /// that exercise section-name canonicalization.
    fn with_field_with_reference(
        mut self,
        item_id: &str,
        label: &str,
        id: &str,
        concealed: bool,
        reference: &str,
    ) -> Self {
        self.fields
            .entry(item_id.to_string())
            .or_default()
            .push(OpField {
                id: id.to_string(),
                label: label.to_string(),
                field_type: if concealed {
                    "CONCEALED".into()
                } else {
                    "STRING".into()
                },
                concealed,
                reference: reference.to_string(),
            });
        self
    }
}

impl OpStructRunner for StubOpStructRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        Ok(vec![])
    }

    fn vault_list(&self, _account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        Ok(self.vaults.clone())
    }

    fn item_list(&self, vault_id: &str, _account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(self.items.get(vault_id).cloned().unwrap_or_default())
    }

    fn item_get(
        &self,
        item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<Vec<OpField>> {
        Ok(self.fields.get(item_id).cloned().unwrap_or_default())
    }
}

#[test]
fn resolve_op_uri_unique_resolves_to_op_ref() {
    let stub = StubOpStructRunner::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Stripe", "i_uuid", "")
        .with_field("i_uuid", "api key", "f_uuid", false);

    let result = resolve_op_uri_to_ref("op://Private/Stripe/api key", &stub, None).unwrap();
    assert_eq!(result.op, "op://v_uuid/i_uuid/f_uuid");
    assert_eq!(result.path, "Private/Stripe/api key");
}

#[test]
fn resolve_op_uri_ambiguous_errors_with_disambig_list() {
    let stub = StubOpStructRunner::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_a", "alexey@zhokhov.com")
        .with_item("v_uuid", "Claude", "i_b", "alexey@chainargos.com")
        .with_item("v_uuid", "Claude", "i_c", "team@example.com");

    let err = resolve_op_uri_to_ref("op://Private/Claude/auth", &stub, None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("3 items"), "msg: {msg}");
    assert!(msg.contains("Claude[alexey@zhokhov.com]"), "msg: {msg}");
    assert!(msg.contains("Claude[alexey@chainargos.com]"), "msg: {msg}");
    assert!(msg.contains("Claude[team@example.com]"), "msg: {msg}");
    // The full op:// line must be copy-pasteable.
    assert!(
        msg.contains("op://Private/Claude[alexey@zhokhov.com]/auth"),
        "full disambiguation line should be present, got:\n{msg}"
    );
}

#[test]
fn resolve_op_uri_with_subtitle_filter_resolves() {
    let stub = StubOpStructRunner::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_a", "alexey@zhokhov.com")
        .with_item("v_uuid", "Claude", "i_b", "alexey@chainargos.com")
        .with_field("i_a", "auth", "f_uuid_a", false);

    let result =
        resolve_op_uri_to_ref("op://Private/Claude[alexey@zhokhov.com]/auth", &stub, None).unwrap();
    assert_eq!(result.op, "op://v_uuid/i_a/f_uuid_a");
    // Path retains brackets because the item is ambiguous in the vault.
    assert_eq!(result.path, "Private/Claude[alexey@zhokhov.com]/auth");
}

#[test]
fn resolve_op_uri_plain_literal_not_affected() {
    // Non-op:// input must be rejected by resolve_op_uri_to_ref.
    let stub = StubOpStructRunner::new();
    let err = resolve_op_uri_to_ref("postgres://localhost", &stub, None).unwrap_err();
    assert!(err.to_string().contains("not an op://"), "{err}");
}

#[test]
fn resolve_op_uri_with_dollar_var_errors() {
    // `${VAR}` substitution inside op:// URIs is unsupported.
    let stub = StubOpStructRunner::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Stripe", "i_uuid", "");

    let err = resolve_op_uri_to_ref("op://${APP_ENV}/Stripe/api key", &stub, None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("substitution") || msg.contains("${"),
        "msg: {msg}"
    );
}

#[test]
fn resolve_op_uri_uuid_form_resolves() {
    // UUID-form input: the vault/item/field IDs are supplied directly.
    // vault_list returns a vault whose id matches the input segment.
    let stub = StubOpStructRunner::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Stripe", "i_uuid", "")
        .with_field("i_uuid", "api key", "f_uuid", false);

    let result = resolve_op_uri_to_ref("op://v_uuid/i_uuid/f_uuid", &stub, None).unwrap();
    assert_eq!(result.op, "op://v_uuid/i_uuid/f_uuid");
    assert_eq!(result.path, "Private/Stripe/api key");
}

#[test]
fn resolve_op_uri_with_attribute_query_preserves_query() {
    let stub = StubOpStructRunner::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "GitHub", "i_uuid", "")
        .with_field("i_uuid", "one-time password", "f_uuid", false);

    let result = resolve_op_uri_to_ref(
        "op://Private/GitHub/one-time password?attribute=otp",
        &stub,
        None,
    )
    .unwrap();
    assert!(result.op.contains("?attribute=otp"), "op: {}", result.op);
    assert!(
        result.path.contains("?attribute=otp"),
        "path: {}",
        result.path
    );
}

#[test]
fn resolve_op_uri_with_attr_short_alias_preserves_query() {
    // 1Password URI grammar accepts `?attr=` as a shorthand for `?attribute=`.
    let stub = StubOpStructRunner::default()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "GitHub", "i_uuid", "")
        .with_field("i_uuid", "one-time password", "f_uuid", false);
    let r = resolve_op_uri_to_ref(
        "op://Private/GitHub/one-time password?attr=type",
        &stub,
        None,
    )
    .unwrap();
    assert!(r.op.contains("?attr=type"), "op: {}", r.op);
    assert!(r.path.contains("?attr=type"), "path: {}", r.path);
}

#[test]
fn resolve_op_uri_with_ssh_format_query_preserves_query() {
    let stub = StubOpStructRunner::default()
        .with_vault("Personal", "v_uuid")
        .with_item("v_uuid", "MyKey", "i_uuid", "")
        .with_field("i_uuid", "private key", "f_uuid", false);
    let r = resolve_op_uri_to_ref(
        "op://Personal/MyKey/private key?ssh-format=openssh",
        &stub,
        None,
    )
    .unwrap();
    assert!(r.op.contains("?ssh-format=openssh"), "op: {}", r.op);
    assert!(r.path.contains("?ssh-format=openssh"), "path: {}", r.path);
}

#[test]
fn resolve_op_uri_4_segment_with_section_resolves() {
    let stub = StubOpStructRunner::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid", "")
        .with_field("i_uuid", "auth token", "f_uuid", false);

    let result =
        resolve_op_uri_to_ref("op://Private/Claude/security/auth token", &stub, None).unwrap();
    assert_eq!(result.path, "Private/Claude/security/auth token");
    assert!(result.op.contains("/security/"), "op: {}", result.op);
}

#[test]
fn resolve_op_uri_vault_not_found_errors() {
    let stub = StubOpStructRunner::new().with_vault("Personal", "v1");

    let err = resolve_op_uri_to_ref("op://NoSuchVault/Item/field", &stub, None).unwrap_err();
    assert!(err.to_string().contains("vault not found"), "{}", err);
}

#[test]
fn resolve_op_uri_item_not_found_errors() {
    let stub = StubOpStructRunner::new().with_vault("Private", "v_uuid");
    // No items in the vault.

    let err = resolve_op_uri_to_ref("op://Private/NoSuchItem/field", &stub, None).unwrap_err();
    assert!(err.to_string().contains("not found"), "{}", err);
}

#[test]
fn resolve_op_uri_field_not_found_errors() {
    let stub = StubOpStructRunner::new()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Stripe", "i_uuid", "");
    // No fields on the item.

    let err = resolve_op_uri_to_ref("op://Private/Stripe/api key", &stub, None).unwrap_err();
    assert!(err.to_string().contains("not found"), "{}", err);
}

#[test]
fn resolve_op_uri_normalizes_section_to_field_reference_form() {
    let stub = StubOpStructRunner::default()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid", "")
        .with_field_with_reference(
            "i_uuid",
            "auth token",
            "f_uuid",
            false,
            // canonical: "Security" capitalized
            "op://Private/Claude/Security/auth token",
        );
    let r = resolve_op_uri_to_ref(
        // User types lowercase "security"
        "op://Private/Claude/security/auth token",
        &stub,
        None,
    )
    .unwrap();
    // Both op and path normalize to the canonical "Security" capitalization.
    assert!(
        r.op.contains("/Security/"),
        "op should use canonical section form, got {}",
        r.op
    );
    assert!(
        r.path.contains("/Security/"),
        "path should use canonical section form, got {}",
        r.path
    );
}

#[test]
fn resolve_op_uri_disambiguation_uses_id_prefix_when_subtitle_empty() {
    let stub = StubOpStructRunner::default()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Notes", "abcdef1234567890", "")
        .with_item("v_uuid", "Notes", "fedcba0987654321", "");
    let err = resolve_op_uri_to_ref("op://Private/Notes/notesPlain", &stub, None).unwrap_err();
    let msg = format!("{err:#}");
    // Empty subtitles fall back to short id prefixes.
    assert!(
        msg.contains("Notes[#abcdef12]"),
        "expected #id-prefix form, got:\n{msg}"
    );
    assert!(
        msg.contains("Notes[#fedcba09]"),
        "expected #id-prefix form, got:\n{msg}"
    );
}

/// Fix 1B: `[#<id-prefix>]` suggestions from disambig error are parseable
/// and select the correct item by ID prefix.
#[test]
fn resolve_op_uri_with_id_prefix_filter_resolves() {
    let stub = StubOpStructRunner::default()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Notes", "abcdef1234567890", "")
        .with_item("v_uuid", "Notes", "fedcba0987654321", "")
        .with_field("abcdef1234567890", "notesPlain", "f_uuid", false);
    let r = resolve_op_uri_to_ref("op://Private/Notes[#abcdef12]/notesPlain", &stub, None).unwrap();
    assert_eq!(r.op, "op://v_uuid/abcdef1234567890/f_uuid");
}

/// Fix 1C: empty field label falls back to field.id in the display path.
#[test]
fn resolve_op_uri_empty_field_label_uses_field_id_in_path() {
    let stub = StubOpStructRunner::default()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Stripe", "i_uuid", "")
        .with_field("i_uuid", "", "f_uuid", false);
    let r = resolve_op_uri_to_ref("op://Private/Stripe/f_uuid", &stub, None).unwrap();
    // path must not end with a trailing slash (empty label)
    assert_eq!(r.path, "Private/Stripe/f_uuid");
}

/// `CLAUDE_CODE_OAUTH_TOKEN` in `[workspaces.X.env]` resolves normally.
#[test]
fn workspace_env_oauth_token_resolves() {
    let mut cfg = crate::config::AppConfig::default();
    let mut ws = crate::workspace::WorkspaceConfig {
        workdir: "/x".into(),
        ..Default::default()
    };
    ws.env.insert(
        CLAUDE_OAUTH_TOKEN_ENV.into(),
        EnvValue::Plain("sk-ant-oat01-from-env".into()),
    );
    cfg.workspaces.insert("proj".into(), ws);

    let resolved = resolve_operator_env_with(
        &cfg,
        Some("smith"),
        Some("proj"),
        &TestOpRunner::forbidden(),
        |_| Err(std::env::VarError::NotPresent),
    )
    .unwrap();
    assert_eq!(
        resolved.get(CLAUDE_OAUTH_TOKEN_ENV).map(String::as_str),
        Some("sk-ant-oat01-from-env")
    );
}

/// Workspace-role env overrides workspace env (standard last-wins merge).
#[test]
fn workspace_role_env_overrides_workspace_env_for_oauth_token() {
    let mut cfg = crate::config::AppConfig::default();
    let mut ws = crate::workspace::WorkspaceConfig {
        workdir: "/x".into(),
        ..Default::default()
    };
    ws.env.insert(
        CLAUDE_OAUTH_TOKEN_ENV.into(),
        EnvValue::Plain("workspace-tier".into()),
    );
    let mut ov = crate::workspace::WorkspaceRoleOverride::default();
    ov.env.insert(
        CLAUDE_OAUTH_TOKEN_ENV.into(),
        EnvValue::Plain("role-tier".into()),
    );
    ws.roles.insert("smith".into(), ov);
    cfg.workspaces.insert("proj".into(), ws);

    let resolved = resolve_operator_env_with(
        &cfg,
        Some("smith"),
        Some("proj"),
        &TestOpRunner::forbidden(),
        |_| Err(std::env::VarError::NotPresent),
    )
    .unwrap();
    assert_eq!(
        resolved.get(CLAUDE_OAUTH_TOKEN_ENV).map(String::as_str),
        Some("role-tier")
    );
}

/// Workspace env overrides global env for `CLAUDE_CODE_OAUTH_TOKEN`.
#[test]
fn workspace_env_overrides_global_env_for_oauth_token() {
    let mut cfg = crate::config::AppConfig::default();
    cfg.env.insert(
        CLAUDE_OAUTH_TOKEN_ENV.into(),
        EnvValue::Plain("global-token".into()),
    );
    let mut ws = crate::workspace::WorkspaceConfig {
        workdir: "/x".into(),
        ..Default::default()
    };
    ws.env.insert(
        CLAUDE_OAUTH_TOKEN_ENV.into(),
        EnvValue::Plain("workspace-token".into()),
    );
    cfg.workspaces.insert("proj".into(), ws);

    let resolved =
        resolve_operator_env_with(&cfg, None, Some("proj"), &TestOpRunner::forbidden(), |_| {
            Err(std::env::VarError::NotPresent)
        })
        .unwrap();
    assert_eq!(
        resolved.get(CLAUDE_OAUTH_TOKEN_ENV).map(String::as_str),
        Some("workspace-token")
    );
}

/// Fix 1A: 3-segment input where the field actually lives in a section
/// (per field.reference) must include that section in the result.
#[test]
fn resolve_op_uri_3seg_input_picks_up_section_from_field_reference() {
    let stub = StubOpStructRunner::default()
        .with_vault("Private", "v_uuid")
        .with_item("v_uuid", "Claude", "i_uuid", "")
        .with_field_with_reference(
            "i_uuid",
            "auth token",
            "f_uuid",
            false,
            "op://Private/Claude/Security/auth token",
        );
    // User supplies 3-segment URI (no section), but field lives in "Security"
    let r = resolve_op_uri_to_ref("op://Private/Claude/auth token", &stub, None).unwrap();
    assert!(
        r.op.contains("/Security/"),
        "op must include section from field.reference; got {}",
        r.op
    );
    assert!(
        r.path.contains("/Security/"),
        "path must include section from field.reference; got {}",
        r.path
    );
}

/// `account` arg threads through every underlying `op` call.
///
/// `parse_reuse` constructs an `OpCli` without `with_account` and
/// must instead pass the operator's `--op-account` straight to
/// `resolve_op_uri_to_ref`. Regression test for the bug where
/// the account was set on the runner but the resolver passed
/// hardcoded `None` to `vault_list` / `item_list` / `item_get`,
/// so multi-1P-account operators silently resolved against the
/// default account.
#[test]
fn resolve_op_uri_threads_account_into_struct_runner_calls() {
    use std::cell::RefCell;

    struct AccountRecordingStub {
        inner: StubOpStructRunner,
        calls: RefCell<Vec<(&'static str, Option<String>)>>,
    }

    impl OpStructRunner for AccountRecordingStub {
        fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
            Ok(vec![])
        }
        fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
            self.calls
                .borrow_mut()
                .push(("vault_list", account.map(str::to_string)));
            self.inner.vault_list(account)
        }
        fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
            self.calls
                .borrow_mut()
                .push(("item_list", account.map(str::to_string)));
            self.inner.item_list(vault_id, account)
        }
        fn item_get(
            &self,
            item_id: &str,
            vault_id: &str,
            account: Option<&str>,
        ) -> anyhow::Result<Vec<OpField>> {
            self.calls
                .borrow_mut()
                .push(("item_get", account.map(str::to_string)));
            self.inner.item_get(item_id, vault_id, account)
        }
    }

    let stub = AccountRecordingStub {
        inner: StubOpStructRunner::new()
            .with_vault("Private", "v_uuid")
            .with_item("v_uuid", "Stripe", "i_uuid", "")
            .with_field("i_uuid", "api key", "f_uuid", true),
        calls: RefCell::new(Vec::new()),
    };

    let _ = resolve_op_uri_to_ref(
        "op://Private/Stripe/api key",
        &stub,
        Some("work-account-uuid"),
    )
    .unwrap();

    let calls = stub.calls.borrow();
    assert_eq!(
        calls.len(),
        3,
        "resolver should call vault_list, item_list, item_get exactly once each; got {calls:?}"
    );
    for (name, account) in calls.iter() {
        assert_eq!(
            account.as_deref(),
            Some("work-account-uuid"),
            "{name} must receive the threaded account"
        );
    }
}
