use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/contracts");

#[derive(Debug, Deserialize)]
struct SurfaceMatrix {
    schema_version: u64,
    cases: Vec<SurfaceCase>,
}

#[derive(Debug, Deserialize)]
struct SurfaceCase {
    id: String,
    surfaces: Vec<String>,
    state: String,
    dimensions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BypassAllowlist {
    schema_version: u64,
    calls: Vec<AllowedCall>,
}

#[derive(Debug, Deserialize)]
struct AllowedCall {
    path: String,
    symbol: String,
    classification: String,
}

#[test]
fn contract_baseline_projection_fixture_is_well_formed() {
    let fixture = read_json("usage-projection-v1-current.json");
    validate_projection_v1(&fixture).expect("canonical V1 fixture must satisfy contract");
}

#[test]
fn contract_baseline_accepts_local_source_identity_kind() {
    let mut fixture = read_json("usage-projection-v1-current.json");
    fixture["providers"][0]["accounts"][0]["identity_kind"] =
        Value::String("local_source_handle".to_owned());
    validate_projection_v1(&fixture)
        .expect("local source identity must satisfy the published projection contract");
}

#[test]
fn contract_baseline_accepts_unverified_identity_kind() {
    let mut fixture = read_json("usage-projection-v1-current.json");
    fixture["providers"][0]["accounts"][0]["identity_kind"] =
        Value::String("unverified_handle".to_owned());
    validate_projection_v1(&fixture)
        .expect("unverified identity must satisfy the published projection contract");
}

#[test]
fn contract_baseline_projection_rejects_invalid_fixtures() {
    let fixture = read_json("usage-projection-v1-invalid.json");
    let cases = fixture
        .as_array()
        .expect("invalid fixture must be a JSON array");
    assert!(
        !cases.is_empty(),
        "invalid fixture matrix must not be empty"
    );
    for case in cases {
        let id = required_string(case, "id").expect("invalid case needs an id");
        let projection = case
            .get("projection")
            .expect("invalid case needs a projection");
        assert!(
            validate_projection_v1(projection).is_err(),
            "invalid case {id} unexpectedly passed"
        );
    }
}

#[test]
fn contract_baseline_surface_matrix_names_every_state_family() {
    let matrix: SurfaceMatrix = serde_json::from_value(read_json("surface-matrix.json"))
        .expect("surface matrix must parse");
    assert_eq!(matrix.schema_version, 1);
    let actual = matrix
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected = [
        "cli-human-json",
        "console-major-states",
        "capsule-lifecycle",
        "desktop-runtime-accessibility",
        "cross-surface-partial-stale",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    for case in &matrix.cases {
        assert!(!case.surfaces.is_empty(), "{} has no surface", case.id);
        assert!(!case.state.is_empty(), "{} has no state", case.id);
        assert!(!case.dimensions.is_empty(), "{} has no dimensions", case.id);
    }
}

#[test]
fn contract_baseline_provider_calls_have_no_unclassified_route() {
    let allowlist: BypassAllowlist =
        serde_json::from_value(read_json("provider-call-allowlist.json"))
            .expect("provider call allowlist must parse");
    assert_eq!(allowlist.schema_version, 1);
    for call in &allowlist.calls {
        assert!(
            matches!(
                call.classification.as_str(),
                "broker_executor" | "adapter_internal" | "legacy_bypass"
            ),
            "{}:{} has unknown classification {}",
            call.path,
            call.symbol,
            call.classification
        );
    }
    let expected = allowlist
        .calls
        .iter()
        .map(|call| format!("{}|{}", call.path, call.symbol))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        expected.len(),
        allowlist.calls.len(),
        "provider call allowlist contains duplicates"
    );

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate must live below workspace root");
    let symbols = allowlist
        .calls
        .iter()
        .map(|call| call.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let actual = scan_production_calls(root, &symbols);
    assert_eq!(actual, expected, "provider-call inventory drifted");
}

#[test]
fn contract_baseline_provider_calls_detect_unlisted_claude_fetch_routes() {
    let workspace = tempfile::tempdir().expect("temporary workspace must exist");
    let source_dir = workspace.path().join("crates/consumer/src");
    fs::create_dir_all(&source_dir).expect("fixture source directory must exist");
    fs::write(
        source_dir.join("lib.rs"),
        "fn fetch_claude_usage_once() {}\n\
         fn fetch_claude_usage_with_retry() {}\n\
         fn fetch_claude_unused() {}\n\
         fn bypass() {\n\
             fetch_claude_usage_once::<()>();\n\
             let callback = fetch_claude_usage_with_retry;\n\
             callback(fetch_claude_usage_once);\n\
         }\n",
    )
    .expect("fixture source must be writable");
    let symbols = BTreeSet::<&str>::new();
    let calls = scan_production_calls(workspace.path(), &symbols);
    assert_eq!(
        calls,
        [
            "crates/consumer/src/lib.rs|fetch_claude_usage_once".to_owned(),
            "crates/consumer/src/lib.rs|fetch_claude_usage_with_retry".to_owned(),
        ]
        .into_iter()
        .collect()
    );
}

#[test]
fn contract_baseline_provider_calls_detect_injected_codex_route() {
    let workspace = tempfile::tempdir().expect("temporary workspace must exist");
    let source_dir = workspace.path().join("crates/consumer/src");
    fs::create_dir_all(&source_dir).expect("fixture source directory must exist");
    fs::write(
        source_dir.join("lib.rs"),
        "fn bypass() {\n    fetch_codex_rpc_usage();\n}\n",
    )
    .expect("fixture source must be writable");
    let symbols = ["fetch_codex_rpc_usage"].into_iter().collect();
    let calls = scan_production_calls(workspace.path(), &symbols);
    assert_eq!(
        calls,
        ["crates/consumer/src/lib.rs|fetch_codex_rpc_usage".to_owned()]
            .into_iter()
            .collect()
    );
}

#[test]
fn contract_baseline_has_no_removed_claude_cli_routes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate must live below workspace root");
    let forbidden = scan_forbidden_claude_routes(root);
    assert!(
        forbidden.is_empty(),
        "removed Claude CLI routes were reintroduced: {forbidden:?}"
    );
}

#[test]
fn contract_baseline_detects_injected_claude_cli_routes_without_comment_matches() {
    let workspace = tempfile::tempdir().expect("temporary workspace must exist");
    let source_dir = workspace.path().join("crates/consumer/src");
    fs::create_dir_all(&source_dir).expect("fixture source directory must exist");
    fs::write(
        source_dir.join("helper.rs"),
        "pub(crate) fn run_claude_usage_diagnostic() {}\n\
         fn diagnostic_route() { run_claude_usage_diagnostic(); }\n",
    )
    .expect("helper fixture must be writable");
    fs::write(
        source_dir.join("command.rs"),
        "fn cli_route() {\n\
             let _ = std::process::Command::new(\"claude\")\n\
                 .args([\"-p\", \"/usage\"]);\n\
         }\n",
    )
    .expect("command fixture must be writable");
    fs::write(
        source_dir.join("mutable_command.rs"),
        "fn cli_route() {\n\
             let mut command = std::process::Command::new(\"claude\");\n\
             command.arg(\"-p\");\n\
             command.arg(\"/usage\");\n\
         }\n",
    )
    .expect("mutable command fixture must be writable");
    fs::write(
        source_dir.join("aliased_command.rs"),
        "use std::process::Command as Process;\n\
         fn cli_route() {\n\
             let mut command = Process::new(\"claude\");\n\
             command.arg(\"-p\").arg(\"/usage\");\n\
         }\n",
    )
    .expect("aliased command fixture must be writable");
    fs::write(
        source_dir.join("comments.rs"),
        "// run_claude_usage_diagnostic(); Command::new(\"claude\").args([\"-p\", \"/usage\"]);\n\
         /* run_claude_usage_diagnostic(); Command::new(\"claude\").args([\"-p\", \"/usage\"]); */\n\
         fn documentation_only() {\n\
             let _ = \"run_claude_usage_diagnostic(); claude -p /usage\";\n\
         }\n",
    )
    .expect("comment fixture must be writable");

    assert_eq!(
        scan_forbidden_claude_routes(workspace.path()),
        [
            "crates/consumer/src/aliased_command.rs|claude -p /usage".to_owned(),
            "crates/consumer/src/command.rs|claude -p /usage".to_owned(),
            "crates/consumer/src/helper.rs|run_claude_usage_diagnostic".to_owned(),
            "crates/consumer/src/mutable_command.rs|claude -p /usage".to_owned(),
        ]
        .into_iter()
        .collect()
    );
}

fn validate_projection_v1(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "projection must be an object".to_owned())?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("schema_version must be 1".to_owned());
    }
    for key in ["projection_id", "discovery_revision", "broker_instance_id"] {
        required_string(value, key)?;
    }
    required_i64(value, "generated_at_epoch")?;
    required_u64(value, "broker_generation")?;
    required_enum(value, "refresh_state", &["idle", "refreshing"])?;
    for key in ["providers", "unresolved", "issues"] {
        if !object.get(key).is_some_and(Value::is_array) {
            return Err(format!("{key} must be an array"));
        }
    }
    for provider in object["providers"]
        .as_array()
        .expect("providers checked above")
    {
        validate_provider(provider)?;
    }
    Ok(())
}

fn validate_provider(provider: &Value) -> Result<(), String> {
    required_string(provider, "provider_id")?;
    required_string(provider, "display_name")?;
    required_u64(provider, "rank")?;
    required_enum(provider, "membership_state", &["current"])?;
    validate_freshness(provider.get("freshness"))?;
    let accounts = provider
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or_else(|| "provider accounts must be an array".to_owned())?;
    for account in accounts {
        validate_account(account)?;
    }
    Ok(())
}

fn validate_account(account: &Value) -> Result<(), String> {
    required_string(account, "canonical_account_id")?;
    required_u64(account, "rank")?;
    required_string(account, "display_label")?;
    required_enum(
        account,
        "identity_kind",
        &[
            "provider_account_id",
            "provider_stable_handle",
            "local_source_handle",
            "unverified_handle",
        ],
    )?;
    required_enum(
        account,
        "lifecycle",
        &[
            "available",
            "agent_uninitialized",
            "needs_login",
            "needs_secret",
            "unsupported",
            "unavailable",
            "error",
        ],
    )?;
    validate_freshness(account.get("freshness"))?;
    let windows = account
        .get("windows")
        .and_then(Value::as_array)
        .ok_or_else(|| "account windows must be an array".to_owned())?;
    for window in windows {
        validate_window(window)?;
    }
    Ok(())
}

fn validate_window(window: &Value) -> Result<(), String> {
    required_string(window, "window_id")?;
    required_u64(window, "rank")?;
    required_string(window, "label")?;
    required_string(window, "value_label")?;
    required_string(window, "reset_label")?;
    required_enum(
        window,
        "quota_state",
        &[
            "available",
            "not_started",
            "warning",
            "exhausted",
            "unsupported",
            "unavailable",
            "error",
        ],
    )?;
    let remaining = optional_percent(window, "remaining_percent")?;
    let used = optional_percent(window, "used_percent")?;
    if remaining.is_some() == used.is_some() {
        return Err("window needs exactly one percent representation".to_owned());
    }
    Ok(())
}

fn validate_freshness(value: Option<&Value>) -> Result<(), String> {
    let value = value.ok_or_else(|| "freshness is required".to_owned())?;
    required_u64(value, "generation")?;
    required_enum(
        value,
        "phase",
        &["current", "stale", "refreshing", "failed"],
    )?;
    if !value.get("is_stale").is_some_and(Value::is_boolean) {
        return Err("freshness is_stale must be boolean".to_owned());
    }
    Ok(())
}

fn optional_percent(value: &Value, key: &str) -> Result<Option<u64>, String> {
    match value.get(key) {
        None => Ok(None),
        Some(Value::Null) => Err(format!("{key} must be omitted, not null")),
        Some(value) => {
            let percent = value
                .as_u64()
                .ok_or_else(|| format!("{key} must be an unsigned integer"))?;
            if percent > 100 {
                return Err(format!("{key} exceeds 100"));
            }
            Ok(Some(percent))
        }
    }
}

fn required_enum(value: &Value, key: &str, allowed: &[&str]) -> Result<(), String> {
    let found = required_string(value, key)?;
    if allowed.contains(&found) {
        Ok(())
    } else {
        Err(format!("invalid {key}: {found}"))
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} must be a non-empty string"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} must be an unsigned integer"))
}

fn required_i64(value: &Value, key: &str) -> Result<i64, String> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{key} must be an integer"))
}

fn read_json(name: &str) -> Value {
    let path = Path::new(FIXTURES).join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("contract fixture must exist"))
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn scan_production_calls(root: &Path, symbols: &BTreeSet<&str>) -> BTreeSet<String> {
    let mut files = Vec::new();
    collect_rust_files(&root.join("crates"), &mut files);
    let mut calls = BTreeSet::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("scanned path must be below workspace root");
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        if relative_text.contains("/tests/") || relative_text.ends_with("/tests.rs") {
            continue;
        }
        if relative_text == "crates/jackin-usage/src/contract_baseline.rs" {
            continue;
        }
        let source = fs::read_to_string(&path).expect("Rust source must be readable");
        let mut inside_use_statement = false;
        for line in source.lines() {
            let trimmed = line.trim_start();
            if inside_use_statement {
                inside_use_statement = !trimmed.contains(';');
                continue;
            }
            if is_function_definition(trimmed)
                || trimmed.starts_with("//")
                || trimmed.starts_with('*')
            {
                continue;
            }
            if is_use_statement(trimmed) {
                inside_use_statement = !trimmed.contains(';');
                continue;
            }
            // Keep this scanner independent from the fixture for Claude's
            // provider-fetch path. New one-shot/retry collectors can be
            // passed as function values or called with turbofish syntax.
            let code = line.split_once("//").map_or(line, |(code, _)| code);
            let identifiers = code
                .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .filter(|identifier| !identifier.is_empty())
                .collect::<BTreeSet<_>>();
            for symbol in symbols.iter().copied().chain(
                identifiers
                    .iter()
                    .copied()
                    .filter(|identifier| identifier.starts_with("fetch_claude_")),
            ) {
                if identifiers.contains(symbol) {
                    calls.insert(format!("{relative_text}|{symbol}"));
                }
            }
        }
    }
    calls
}

fn is_function_definition(line: &str) -> bool {
    line.starts_with("fn ")
        || line.starts_with("async fn ")
        || (line.starts_with("pub") && line.contains(" fn "))
}

fn is_use_statement(line: &str) -> bool {
    line.starts_with("use ") || (line.starts_with("pub") && line.contains(" use "))
}

fn scan_forbidden_claude_routes(root: &Path) -> BTreeSet<String> {
    let mut files = Vec::new();
    collect_rust_files(&root.join("crates"), &mut files);
    let mut forbidden = BTreeSet::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("scanned path must be below workspace root");
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        if relative_text.contains("/tests/") || relative_text.ends_with("/tests.rs") {
            continue;
        }
        if relative_text == "crates/jackin-usage/src/contract_baseline.rs" {
            continue;
        }
        let source = fs::read_to_string(&path).expect("Rust source must be readable");
        let tokens = tokenize_rust_source(&source);
        if tokens.iter().any(|token| {
            matches!(
                token,
                RustSourceToken::Identifier(identifier)
                    if identifier == "run_claude_usage_diagnostic"
            )
        }) {
            forbidden.insert(format!("{relative_text}|run_claude_usage_diagnostic"));
        }
        if has_claude_usage_command(&tokens) {
            forbidden.insert(format!("{relative_text}|claude -p /usage"));
        }
    }
    forbidden
}

#[derive(Debug, PartialEq, Eq)]
enum RustSourceToken {
    Identifier(String),
    StringLiteral(String),
    Punctuation(char),
}

fn tokenize_rust_source(source: &str) -> Vec<RustSourceToken> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes[index..].starts_with(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"/*") {
            index += 2;
            let mut depth = 1usize;
            while index < bytes.len() && depth > 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else if let Some((start, end, after)) = raw_string_bounds(bytes, index) {
            tokens.push(RustSourceToken::StringLiteral(
                String::from_utf8_lossy(&bytes[start..end]).into_owned(),
            ));
            index = after;
        } else if bytes[index] == b'"' {
            let start = index + 1;
            index += 1;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index = (index + 2).min(bytes.len()),
                    b'"' => break,
                    _ => index += 1,
                }
            }
            let literal = String::from_utf8_lossy(&bytes[start..index]).into_owned();
            tokens.push(RustSourceToken::StringLiteral(literal));
            if index < bytes.len() {
                index += 1;
            }
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(RustSourceToken::Identifier(
                String::from_utf8_lossy(&bytes[start..index]).into_owned(),
            ));
        } else {
            tokens.push(RustSourceToken::Punctuation(char::from(bytes[index])));
            index += 1;
        }
    }
    tokens
}

fn raw_string_bounds(bytes: &[u8], start: usize) -> Option<(usize, usize, usize)> {
    let mut cursor = if bytes.get(start..start + 2) == Some(b"br") {
        start + 2
    } else if bytes.get(start) == Some(&b'r') {
        start + 1
    } else {
        return None;
    };
    let mut hashes = 0;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    let content_start = cursor + 1;
    cursor = content_start;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && cursor + 1 + hashes <= bytes.len()
            && bytes[cursor + 1..cursor + 1 + hashes]
                .iter()
                .all(|byte| *byte == b'#')
        {
            return Some((content_start, cursor, cursor + 1 + hashes));
        }
        cursor += 1;
    }
    None
}

// Cover literal `::new("claude")` constructors and literal arguments passed to
// their builder binding in the same lexical block. This intentionally does
// not resolve helper wrappers or dynamically assembled arguments.
fn has_claude_usage_command(tokens: &[RustSourceToken]) -> bool {
    for index in 0..tokens.len() {
        if index < 2
            || !is_identifier(tokens.get(index), "new")
            || !is_punctuation(tokens.get(index - 1), ':')
            || !is_punctuation(tokens.get(index - 2), ':')
            || !is_punctuation(tokens.get(index + 1), '(')
        {
            continue;
        }
        let Some((program, after_constructor)) = command_program(tokens, index + 2) else {
            continue;
        };
        let statement_end = tokens[after_constructor..]
            .iter()
            .position(|token| matches!(token, RustSourceToken::Punctuation(';' | '}')))
            .map_or(tokens.len(), |offset| after_constructor + offset);
        let arguments = &tokens[after_constructor..statement_end];
        let executable = program.rsplit(['/', '\\']).next().unwrap_or(program);
        if executable == "claude"
            && has_argument_method(arguments)
            && has_string_argument(arguments, "-p")
            && has_string_argument(arguments, "/usage")
        {
            return true;
        }
        let shell_command = arguments.iter().any(|token| {
            matches!(token, RustSourceToken::StringLiteral(value) if contains_claude_usage_command(value))
        });
        if matches!(executable, "sh" | "bash" | "zsh" | "dash")
            && has_string_argument(arguments, "-c")
            && shell_command
        {
            return true;
        }
        if contains_claude_usage_command(program) {
            return true;
        }
        let executable_is_claude =
            program.rsplit(['/', '\\']).next().unwrap_or(program) == "claude";
        if executable_is_claude
            && variable_assigned_to_constructor(tokens, index)
                .zip(enclosing_block_end(tokens, index))
                .is_some_and(|(variable, scope_end)| {
                    variable_runs_claude_usage_command(tokens, &variable, index, scope_end)
                })
        {
            return true;
        }
    }
    false
}

fn variable_assigned_to_constructor(
    tokens: &[RustSourceToken],
    constructor: usize,
) -> Option<String> {
    let start = (0..constructor)
        .rev()
        .find(|index| {
            is_punctuation(tokens.get(*index), ';')
                || is_punctuation(tokens.get(*index), '{')
                || is_punctuation(tokens.get(*index), '}')
        })
        .map_or(0, |index| index + 1);
    let assignment = (start..constructor)
        .rev()
        .find(|index| is_punctuation(tokens.get(*index), '='))?;
    let binding_start = (start..assignment).find(|index| is_identifier(tokens.get(*index), "let"));
    if let Some(binding_start) = binding_start {
        return tokens[binding_start + 1..assignment]
            .iter()
            .find_map(|token| match token {
                RustSourceToken::Identifier(identifier)
                    if identifier != "mut" && identifier != "ref" =>
                {
                    Some(identifier.clone())
                }
                _ => None,
            });
    }
    tokens[start..assignment]
        .iter()
        .rev()
        .find_map(|token| match token {
            RustSourceToken::Identifier(identifier) => Some(identifier.clone()),
            _ => None,
        })
}

fn enclosing_block_end(tokens: &[RustSourceToken], position: usize) -> Option<usize> {
    let mut open_blocks = Vec::new();
    for (index, token) in tokens.iter().enumerate().take(position + 1) {
        match token {
            RustSourceToken::Punctuation('{') => open_blocks.push(index),
            RustSourceToken::Punctuation('}') => {
                open_blocks.pop();
            }
            _ => {}
        }
    }
    let open = *open_blocks.last()?;
    let mut depth = 1usize;
    for (index, token) in tokens.iter().enumerate().skip(open + 1) {
        match token {
            RustSourceToken::Punctuation('{') => depth += 1,
            RustSourceToken::Punctuation('}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn variable_runs_claude_usage_command(
    tokens: &[RustSourceToken],
    variable: &str,
    constructor: usize,
    scope_end: usize,
) -> bool {
    let mut saw_argument_method = false;
    let mut saw_print_flag = false;
    let mut saw_usage_argument = false;
    let mut index = constructor + 1;
    while index + 3 < scope_end {
        if is_identifier(tokens.get(index), variable) && is_punctuation(tokens.get(index + 1), '.')
        {
            let mut cursor = index + 1;
            while cursor + 2 < scope_end
                && is_punctuation(tokens.get(cursor), '.')
                && (is_identifier(tokens.get(cursor + 1), "arg")
                    || is_identifier(tokens.get(cursor + 1), "args"))
                && is_punctuation(tokens.get(cursor + 2), '(')
            {
                saw_argument_method = true;
                let (after_call, found_print_flag, found_usage_argument) =
                    scan_builder_arguments(tokens, cursor + 2, scope_end);
                saw_print_flag |= found_print_flag;
                saw_usage_argument |= found_usage_argument;
                cursor = after_call;
            }
            index = cursor.max(index + 1);
        } else {
            index += 1;
        }
    }
    saw_argument_method && saw_print_flag && saw_usage_argument
}

fn scan_builder_arguments(
    tokens: &[RustSourceToken],
    opening_paren: usize,
    scope_end: usize,
) -> (usize, bool, bool) {
    let mut cursor = opening_paren + 1;
    let mut depth = 1usize;
    let mut saw_print_flag = false;
    let mut saw_usage_argument = false;
    while cursor < scope_end && depth > 0 {
        match tokens.get(cursor) {
            Some(RustSourceToken::Punctuation('(')) => depth += 1,
            Some(RustSourceToken::Punctuation(')')) => depth -= 1,
            Some(RustSourceToken::StringLiteral(value)) if depth > 0 => {
                saw_print_flag |= value == "-p";
                saw_usage_argument |= value == "/usage";
            }
            _ => {}
        }
        cursor += 1;
    }
    (cursor, saw_print_flag, saw_usage_argument)
}

fn command_program(tokens: &[RustSourceToken], start: usize) -> Option<(&str, usize)> {
    let mut program = None;
    let mut depth = 1usize;
    for (offset, token) in tokens.iter().enumerate().skip(start) {
        match token {
            RustSourceToken::StringLiteral(value) if depth == 1 && program.is_none() => {
                program = Some(value.as_str());
            }
            RustSourceToken::Punctuation('(') => depth += 1,
            RustSourceToken::Punctuation(')') => {
                depth -= 1;
                if depth == 0 {
                    return program.map(|program| (program, offset + 1));
                }
            }
            RustSourceToken::Punctuation(';') => return None,
            _ => {}
        }
    }
    None
}

fn has_argument_method(tokens: &[RustSourceToken]) -> bool {
    tokens.windows(2).any(|window| {
        is_punctuation(window.first(), '.')
            && (is_identifier(window.get(1), "arg") || is_identifier(window.get(1), "args"))
    })
}

fn has_string_argument(tokens: &[RustSourceToken], expected: &str) -> bool {
    tokens
        .iter()
        .any(|token| matches!(token, RustSourceToken::StringLiteral(value) if value == expected))
}

fn contains_claude_usage_command(command: &str) -> bool {
    command
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(3)
        .any(|words| words[0] == "claude" && words[1] == "-p" && words[2] == "/usage")
}

fn is_identifier(token: Option<&RustSourceToken>, expected: &str) -> bool {
    matches!(token, Some(RustSourceToken::Identifier(identifier)) if identifier == expected)
}

fn is_punctuation(token: Option<&RustSourceToken>, expected: char) -> bool {
    matches!(token, Some(RustSourceToken::Punctuation(punctuation)) if *punctuation == expected)
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
