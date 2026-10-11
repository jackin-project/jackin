// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn status_parses_staged_unstaged_untracked_and_renames() {
    let output = b"M  crates/a.rs\0 M crates/b.rs\0?? crates/c.rs\0R  new.rs\0old.rs\0";
    let paths = parse_status_porcelain(output).expect("parse");
    assert_eq!(
        paths,
        vec![
            PathBuf::from("crates/a.rs"),
            PathBuf::from("crates/b.rs"),
            PathBuf::from("crates/c.rs"),
            PathBuf::from("new.rs"),
            PathBuf::from("old.rs"),
        ]
    );
}

#[test]
fn status_empty_and_non_utf8() {
    assert!(parse_status_porcelain(b"").expect("parse").is_empty());
    parse_status_porcelain(b"\xff\xfe\0").unwrap_err();
}

#[test]
fn renames_without_second_field_error() {
    parse_status_porcelain(b"R  only.rs\0").unwrap_err();
}

#[test]
fn classify_routes_members_nested_config_and_drops_noise() {
    let graph = graph();
    let nested = nested_fixtures();
    let member = |path: &str| classify(&graph, &nested, Path::new(path));
    assert!(matches!(
        member("crates/adapters/jackin-image/src/lib.rs"),
        Some(Owner::Member(_))
    ));
    assert!(matches!(
        member("crates/services/jackin-agent-status/fuzz/src/main.rs"),
        Some(Owner::Nested(0))
    ));
    assert!(matches!(
        member("vendor/arrayref/src/lib.rs"),
        Some(Owner::Nested(1))
    ));
    assert!(member("crates/tools/jackin-lints/src/lib.rs").is_none());
    assert!(matches!(member("Cargo.lock"), Some(Owner::Member(_))));
    assert!(matches!(member("clippy.toml"), Some(Owner::Member(_))));
    assert!(matches!(
        member(".cargo/config.toml"),
        Some(Owner::Member(_))
    ));
    assert!(member("native/Sources/App.swift").is_none());
    assert!(member("mise.toml").is_none());
    assert!(member("docker/runtime/entrypoint.sh").is_none());
    assert!(member("hk.pkl").is_none());
}

#[test]
fn select_expands_dependents_and_nested_path_dependents() {
    let graph = graph();
    let nested = nested_fixtures();
    let selection = select(
        &graph,
        &nested,
        &inputs_fixture(),
        &[PathBuf::from(
            "crates/services/jackin-agent-status/src/lib.rs",
        )],
    )
    .expect("select");
    assert_eq!(selection.members, vec!["jackin-agent-status".to_owned()]);
    assert_eq!(selection.nested, BTreeSet::from([0]));
}

#[test]
fn select_pulls_transitive_member_dependents() {
    let graph = graph();
    let nested = nested_fixtures();
    let selection = select(
        &graph,
        &nested,
        &inputs_fixture(),
        &[PathBuf::from("crates/adapters/jackin-image/src/lib.rs")],
    )
    .expect("select");
    assert_eq!(
        selection.members,
        vec!["jackin".to_owned(), "jackin-image".to_owned()]
    );
    assert!(selection.nested.is_empty());
}

#[test]
fn select_maps_cross_boundary_inputs_to_owners() {
    let graph = graph();
    let nested = nested_fixtures();
    let selection = select(
        &graph,
        &nested,
        &inputs_fixture(),
        &[PathBuf::from("docker/runtime/entrypoint.sh")],
    )
    .expect("select");
    assert_eq!(
        selection.members,
        vec!["jackin".to_owned(), "jackin-image".to_owned()]
    );
}

#[test]
fn select_nested_direct_change_pulls_resolve_dependents() {
    let graph = graph();
    let nested = nested_fixtures();
    let selection = select(
        &graph,
        &nested,
        &inputs_fixture(),
        &[PathBuf::from("vendor/arrayref/src/lib.rs")],
    )
    .expect("select");
    // arrayref feeds jackin-image (fixture resolve graph), which feeds jackin.
    assert_eq!(
        selection.members,
        vec!["jackin".to_owned(), "jackin-image".to_owned()]
    );
    assert_eq!(selection.nested, BTreeSet::from([1]));
}

#[test]
fn select_irrelevant_paths_select_nothing() {
    let graph = graph();
    let nested = nested_fixtures();
    let selection = select(
        &graph,
        &nested,
        &inputs_fixture(),
        &[
            PathBuf::from("native/Sources/App.swift"),
            PathBuf::from("crates/apps/jackin/README.md"),
            PathBuf::from("crates/tools/jackin-lints/src/lib.rs"),
            PathBuf::from(".github/workflows/ci.yml"),
        ],
    )
    .expect("select");
    assert!(selection.members.is_empty());
    assert!(selection.nested.is_empty());
}

#[test]
fn unknown_inputs_widen_to_everything_stable() {
    let graph = graph();
    let nested = nested_fixtures();
    let inputs = InputOwners {
        unknown: true,
        ..inputs_fixture()
    };
    let selection = select(
        &graph,
        &nested,
        &inputs,
        &[PathBuf::from("crates/apps/jackin/src/main.rs")],
    )
    .expect("select");
    assert_eq!(
        selection.members,
        ["jackin", "jackin-agent-status", "jackin-image"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
    assert_eq!(selection.nested, BTreeSet::from([0, 1]));
}

#[test]
fn member_clippy_args_match_ci_flags() {
    let args = member_clippy_args(&["beta".into(), "alpha".into()]);
    let rendered: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        rendered,
        vec![
            "clippy",
            "--locked",
            "--profile",
            "test",
            "--all-targets",
            "--all-features",
            "-p",
            "beta",
            "-p",
            "alpha",
            "--",
            "-D",
            "warnings",
        ]
    );
}

#[test]
fn nested_clippy_args_use_manifest_path_with_ci_flags() {
    let args = nested_clippy_args(Path::new("vendor/arrayref"));
    let rendered: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        rendered,
        vec![
            "clippy",
            "--manifest-path",
            "vendor/arrayref/Cargo.toml",
            "--locked",
            "--profile",
            "test",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ]
    );
}

#[test]
fn scanner_records_cross_boundary_literals() {
    let mut inputs = InputOwners::default();
    let file = Path::new("crates/adapters/jackin-image/src/derived_image.rs");
    let scope = Path::new("crates/adapters/jackin-image");
    scan_source_inputs(
        concat!(
            "const A: &str = include_str!",
            "(\"../../../docker/x.sh\");\n",
            "const B: &str = include_str!",
            "(\"local.toml\");\n",
            "#[path",
            " = \"tui/mod.rs\"]\n",
            "mod tui;",
        ),
        file,
        scope,
        Some("jackin-image"),
        None,
        &mut inputs,
    );
    assert!(!inputs.unknown);
    assert_eq!(
        inputs.members,
        BTreeMap::from([(
            PathBuf::from("docker/x.sh"),
            BTreeSet::from(["jackin-image".into()]),
        )])
    );
}

#[test]
fn scanner_handles_multiline_and_bytes_and_bare_forms() {
    let mut inputs = InputOwners::default();
    let file = Path::new("crates/p/src/a/b.rs");
    let scope = Path::new("crates/p");
    scan_source_inputs(
        concat!(
            "let a = include_bytes!",
            "(\n\"../../../../asset.bin\"\n);\n",
            "include!",
            "(\"../../../../gen.rs\");",
        ),
        file,
        scope,
        Some("p"),
        None,
        &mut inputs,
    );
    assert!(!inputs.unknown);
    assert!(inputs.members.contains_key(Path::new("asset.bin")));
    assert!(inputs.members.contains_key(Path::new("gen.rs")));
}

#[test]
fn scanner_widens_only_on_compilable_non_literals() {
    let mut inputs = InputOwners::default();
    let file = Path::new("crates/p/src/lib.rs");
    let scope = Path::new("crates/p");
    scan_source_inputs(
        concat!("include_str!", "(concat!(env!(\"X\"), \"/y\"));"),
        file,
        scope,
        Some("p"),
        None,
        &mut inputs,
    );
    assert!(inputs.unknown);

    // Out-of-repo literals drop: no commit can change their target.
    let mut inputs = InputOwners::default();
    scan_source_inputs(
        concat!("include_str!", "(\"../../../../../../etc/hostname\");"),
        file,
        scope,
        Some("p"),
        None,
        &mut inputs,
    );
    assert!(!inputs.unknown);
    assert!(inputs.members.is_empty());
}

#[test]
fn scanner_records_raw_string_path_and_include_literals() {
    let mut inputs = InputOwners::default();
    let file = Path::new("crates/p/src/lib.rs");
    let scope = Path::new("crates/p");
    scan_source_inputs(
        concat!(
            "#[path",
            " = r\"../../../shared.rs\"]\n",
            "mod shared;\n",
            "const A: &str = include_str!",
            "(r##\"../../../quoted\"#.txt\"##);\n",
        ),
        file,
        scope,
        Some("p"),
        None,
        &mut inputs,
    );
    assert!(!inputs.unknown);
    assert!(inputs.members.contains_key(Path::new("shared.rs")));
    assert!(inputs.members.contains_key(Path::new("quoted\"#.txt")));
}

#[test]
fn scanner_parses_all_compilable_cooked_escapes() {
    // Every escape below compiles in a `str` literal, so the `#[path]`
    // consumer must record (not silently skip) each of them.
    assert_eq!(
        parse_string_literal(r#""a\nb""#),
        Some(("a\nb".to_owned(), 6))
    );
    assert_eq!(
        parse_string_literal(r#""a\tb""#),
        Some(("a\tb".to_owned(), 6))
    );
    assert_eq!(
        parse_string_literal(r#""a\rb""#),
        Some(("a\rb".to_owned(), 6))
    );
    assert_eq!(
        parse_string_literal(r#""a\0b""#),
        Some(("a\0b".to_owned(), 6))
    );
    assert_eq!(
        parse_string_literal(r#""a\'b""#),
        Some(("a'b".to_owned(), 6))
    );
    assert_eq!(
        parse_string_literal(r#""\x2fb""#),
        Some(("/b".to_owned(), 7))
    );
    assert_eq!(
        parse_string_literal(r#""\u{2f}b""#),
        Some(("/b".to_owned(), 9))
    );
    assert_eq!(
        parse_string_literal("\"a\\\n   b\""),
        Some(("ab".to_owned(), 9))
    );
    // Non-`str` literals never compile in path/include position.
    assert_eq!(parse_string_literal("rb\"a\""), None);
    assert_eq!(parse_string_literal("br\"a\""), None);
    assert_eq!(parse_string_literal("\"\\x80\""), None);
    assert_eq!(parse_string_literal("concat!(\"a\")"), None);
}

#[test]
fn scanner_skips_path_like_attributes() {
    let mut inputs = InputOwners::default();
    let file = Path::new("crates/p/src/lib.rs");
    let scope = Path::new("crates/p");
    scan_source_inputs(
        "#[pathname = \"x\"]",
        file,
        scope,
        Some("p"),
        None,
        &mut inputs,
    );
    assert!(!inputs.unknown);
    assert!(inputs.members.is_empty());
}
