use super::*;

fn graph() -> WorkspaceGraph {
    WorkspaceGraph {
        names: BTreeMap::from([
            ("image-id".into(), "jackin-image".into()),
            ("status-id".into(), "jackin-agent-status".into()),
            ("app-id".into(), "jackin".into()),
        ]),
        package_names: BTreeMap::from([
            ("image-id".into(), "jackin-image".into()),
            ("status-id".into(), "jackin-agent-status".into()),
            ("app-id".into(), "jackin".into()),
            ("arrayref 0.3.9".into(), "arrayref".into()),
        ]),
        roots: BTreeMap::from([
            ("image-id".into(), PathBuf::from("crates/jackin-image")),
            (
                "status-id".into(),
                PathBuf::from("crates/jackin-agent-status"),
            ),
            ("app-id".into(), PathBuf::from("crates/jackin")),
        ]),
        dependents: BTreeMap::from([("image-id".into(), BTreeSet::from(["app-id".into()]))]),
        resolved_dependencies: BTreeMap::from([
            ("image-id".into(), BTreeSet::from(["arrayref 0.3.9".into()])),
            ("status-id".into(), BTreeSet::new()),
            ("app-id".into(), BTreeSet::from(["image-id".into()])),
        ]),
        resolved_features: BTreeMap::new(),
    }
}

fn nested_fixtures() -> Vec<NestedPackage> {
    vec![
        NestedPackage {
            dir: PathBuf::from("crates/jackin-agent-status/fuzz"),
            name: "jackin-agent-status-fuzz".into(),
            member_dependencies: BTreeSet::from(["jackin-agent-status".into()]),
            depends_on_unknown: false,
            excluded: false,
        },
        NestedPackage {
            dir: PathBuf::from("vendor/arrayref"),
            name: "arrayref".into(),
            member_dependencies: BTreeSet::new(),
            depends_on_unknown: false,
            excluded: false,
        },
        NestedPackage {
            dir: PathBuf::from("crates/jackin-lints"),
            name: "jackin-lints".into(),
            member_dependencies: BTreeSet::new(),
            depends_on_unknown: false,
            excluded: true,
        },
    ]
}

fn inputs_fixture() -> InputOwners {
    InputOwners {
        members: BTreeMap::from([(
            PathBuf::from("docker/runtime/entrypoint.sh"),
            BTreeSet::from(["jackin-image".into()]),
        )]),
        nested: BTreeMap::new(),
        unknown: false,
        unknown_sources: Vec::new(),
    }
}

// Fixture sources split the macro name from its opener: the runtime scanner
// reads these test files as plain text, so a literal `include_*!(…)` here
// would poison real selections. `concat!` rejoins the fragments for the
// scanner under test.

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
        member("crates/jackin-image/src/lib.rs"),
        Some(Owner::Member(_))
    ));
    assert!(matches!(
        member("crates/jackin-agent-status/fuzz/src/main.rs"),
        Some(Owner::Nested(0))
    ));
    assert!(matches!(
        member("vendor/arrayref/src/lib.rs"),
        Some(Owner::Nested(1))
    ));
    assert!(member("crates/jackin-lints/src/lib.rs").is_none());
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
        &[PathBuf::from("crates/jackin-agent-status/src/lib.rs")],
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
        &[PathBuf::from("crates/jackin-image/src/lib.rs")],
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
            PathBuf::from("crates/jackin/README.md"),
            PathBuf::from("crates/jackin-lints/src/lib.rs"),
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
        &[PathBuf::from("crates/jackin/src/main.rs")],
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
    let file = Path::new("crates/jackin-image/src/derived_image.rs");
    let scope = Path::new("crates/jackin-image");
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

#[test]
fn normalize_resolves_dot_segments_lexically() {
    assert_eq!(
        normalize(Path::new("crates/p/src/../asset.bin")),
        PathBuf::from("crates/p/asset.bin")
    );
    assert_eq!(
        normalize(Path::new("crates/p/../../docker/x.sh")),
        PathBuf::from("docker/x.sh")
    );
    assert_eq!(
        normalize(Path::new("../../etc/x")),
        PathBuf::from("../../etc/x")
    );
}

#[test]
fn path_dependencies_read_all_sections() {
    let manifest = r#"
[dependencies]
parent = { path = ".." }
serde = "1"

[dev-dependencies]
helper = { path = "../helper" }

[build-dependencies]
gen = { path = "gen" }
"#;
    let value = toml::from_str::<toml::Value>(manifest).expect("parse");
    let mut deps = path_dependencies(&value);
    deps.sort();
    assert_eq!(
        deps,
        vec![
            PathBuf::from(".."),
            PathBuf::from("../helper"),
            PathBuf::from("gen"),
        ]
    );
}

/// Canary: the scanner must keep finding the real cross-crate includes, so
/// scanner bit-rot fails loudly instead of silently narrowing the closure.
#[test]
fn scanner_finds_real_cross_crate_includes() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root above crates/jackin-xtask");
    let source = std::fs::read_to_string(root.join("crates/jackin-image/src/derived_image.rs"))
        .expect("read derived_image.rs");
    let mut inputs = InputOwners::default();
    scan_source_inputs(
        &source,
        Path::new("crates/jackin-image/src/derived_image.rs"),
        Path::new("crates/jackin-image"),
        Some("jackin-image"),
        None,
        &mut inputs,
    );
    assert!(!inputs.unknown);
    let owners = inputs.members;
    assert!(owners.contains_key(Path::new("docker/runtime/entrypoint.sh")));
    assert!(owners.contains_key(Path::new("crates/jackin-agent-status/packs/claude.toml")));
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root above crates/jackin-xtask")
        .to_path_buf()
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let mut stack = vec![root.join("crates"), root.join("vendor")];
    while let Some(dir) = stack.pop() {
        let entries = fs_util::read_dir_sorted(&dir).expect("read dir");
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(path);
            }
        }
    }
    sources.sort();
    sources
}

/// Canary: no repository source may force the unknown-widening path. A
/// future non-literal include (or an escaping literal) must fail here so
/// the closure stays proven instead of silently degrading to workspace-wide.
#[test]
fn repo_scan_stays_within_resolvable_inputs() {
    let root = workspace_root();
    let mut inputs = InputOwners::default();
    for path in rust_sources(&root) {
        let source = std::fs::read_to_string(&path).expect("read source");
        let relative = path
            .strip_prefix(&root)
            .expect("source under root")
            .to_path_buf();
        // Empty scope: every literal resolves in-scope; only genuinely
        // unresolvable inputs set `unknown`.
        scan_source_inputs(&source, &relative, Path::new(""), None, None, &mut inputs);
    }
    assert!(
        !inputs.unknown,
        "unresolvable includes in: {:?}",
        inputs.unknown_sources
    );
}

/// Independent backstop (separate implementation from the production
/// scanner): every `include_*!` invocation in the repo must carry a plain
/// string literal. Mentions without a delimiter are skipped, mirroring the
/// production mention rule.
#[test]
fn every_include_invocation_carries_a_literal() {
    let root = workspace_root();
    for path in rust_sources(&root) {
        let source = std::fs::read_to_string(&path).expect("read source");
        for hit in find_include_macros(&source) {
            let after = skip_ascii_trivia(&source[hit..]);
            let opener = after.as_bytes().first().copied();
            if !matches!(opener, Some(b'(' | b'[' | b'{')) {
                continue;
            }
            let candidate = skip_ascii_trivia(&after[1..]);
            assert_eq!(
                candidate.as_bytes().first(),
                Some(&b'"'),
                "{}:{}: include invocation without a string literal",
                path.display(),
                source[..hit].matches('\n').count() + 1,
            );
        }
    }
}

/// Byte offsets just past each `include_str!` / `include_bytes!` /
/// `include!` occurrence (longest match wins at each position).
fn find_include_macros(source: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut index = 0;
    while index < source.len() {
        let rest = &source[index..];
        if let Some(tail) = rest.strip_prefix("include_str!") {
            hits.push(source.len() - tail.len());
            index += "include_str!".len();
        } else if let Some(tail) = rest.strip_prefix("include_bytes!") {
            hits.push(source.len() - tail.len());
            index += "include_bytes!".len();
        } else if rest.starts_with("include!") && !rest.starts_with("include_") {
            index += "include!".len();
            hits.push(index);
        } else {
            index += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    hits
}

fn skip_ascii_trivia(mut source: &str) -> &str {
    loop {
        let trimmed = source.trim_start_matches([' ', '\t', '\n', '\r']);
        if let Some(rest) = trimmed.strip_prefix("//") {
            match rest.find('\n') {
                Some(index) => source = &rest[index + 1..],
                None => return "",
            }
        } else if let Some(rest) = trimmed.strip_prefix("/*") {
            match rest.find("*/") {
                Some(index) => source = &rest[index + 2..],
                None => return "",
            }
        } else {
            return trimmed;
        }
    }
}
