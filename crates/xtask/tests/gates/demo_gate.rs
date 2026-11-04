// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use jackin_xtask::{
    Check, CommandRunner, DEMO_CANARIES, DemoGateCheck, ProductCheck, ToolOutput, Violation,
};

use crate::workspace::{TestResult, Workspace, green, member_manifest};

const CONSOLE_SOURCE: &str = "https://github.com/tailrocks/terminal-components-baseline?rev=74244972a1183db85111332ddab7eeabcbae81cb#74244972";

const NO_MATCH_CARGO_1_99_1: &str =
    "error: package ID specification `jackin-console` did not match any packages\n";

const TREE_JACKIN: &str = "-p jackin -e normal,build -f {p} {f} -i jackin-console";
const TREE_TUI: &str = "-p jackin-tui -e normal,build -f {p} {f} -i jackin-console";
const TREE_CLAP: &str = "-p jackin -e normal -i clap";
const BUILD: &str = "-p jackin-tui --message-format=json";
const RELEASE: &str = "--release -p jackin --message-format=json";

struct Script {
    replies: BTreeMap<String, io::Result<ToolOutput>>,
}

impl CommandRunner for Script {
    fn run(&self, program: &str, args: &[&str], _dir: &Path) -> io::Result<ToolOutput> {
        let key = format!("{program} {}", args.join(" "));
        match self.replies.get(&key) {
            Some(Ok(output)) => Ok(output.clone()),
            Some(Err(error)) => Err(io::Error::new(error.kind(), error.to_string())),
            None => Err(io::Error::other(format!("unscripted command: {key}"))),
        }
    }
}

fn ok(stdout: impl Into<String>) -> io::Result<ToolOutput> {
    Ok(ToolOutput {
        success: true,
        stdout: stdout.into(),
        stderr: String::new(),
    })
}

fn failed(stderr: &str) -> io::Result<ToolOutput> {
    Ok(ToolOutput {
        success: false,
        stdout: String::new(),
        stderr: stderr.to_owned(),
    })
}

fn tree_cargo_1_99_1(features: &str, below: &[&str]) -> String {
    let mut text = format!("jackin-console v0.1.0 ({CONSOLE_SOURCE}) {features}\n");
    let mut indent = String::new();
    for line in below {
        text.push_str(&format!("{indent}└── {line}\n"));
        indent.push_str("    ");
    }
    text
}

fn clean_tree() -> String {
    tree_cargo_1_99_1(
        "",
        &[
            "jackin-tui v0.1.0 (/work/crates/jackin-tui) ",
            "jackin v0.1.0 (/work/crates/jackin) ",
        ],
    )
}

fn build_json_cargo_1_99_1(rlib: &str) -> String {
    let artifact = format!(
        "{{\"reason\":\"compiler-artifact\",\"package_id\":\"git+https://github.com/tailrocks/terminal-components-baseline?rev=74244972a1183db85111332ddab7eeabcbae81cb#jackin-console@0.1.0\",\"manifest_path\":\"/cargo/git/checkouts/terminal-components-baseline/7424497/apps/jackin/Cargo.toml\",\"target\":{{\"kind\":[\"lib\"],\"crate_types\":[\"lib\"],\"name\":\"jackin_console\",\"src_path\":\"/cargo/git/checkouts/terminal-components-baseline/7424497/apps/jackin/src/lib.rs\",\"edition\":\"2024\",\"doc\":true,\"doctest\":false,\"test\":true}},\"profile\":{{\"opt_level\":\"0\",\"debuginfo\":0,\"debug_assertions\":true,\"overflow_checks\":true,\"test\":false}},\"features\":[],\"filenames\":[\"{rlib}\",\"{rlib}.rmeta\"],\"executable\":null,\"fresh\":false}}"
    );
    let other = "{\"reason\":\"compiler-artifact\",\"package_id\":\"registry+https://github.com/rust-lang/crates.io-index#ratatui@0.30.2\",\"manifest_path\":\"/cargo/registry/ratatui/Cargo.toml\",\"target\":{\"kind\":[\"lib\"],\"crate_types\":[\"lib\"],\"name\":\"ratatui\",\"src_path\":\"/cargo/registry/ratatui/src/lib.rs\",\"edition\":\"2024\",\"doc\":true,\"doctest\":true,\"test\":true},\"profile\":{\"opt_level\":\"0\",\"debuginfo\":0,\"debug_assertions\":true,\"overflow_checks\":true,\"test\":false},\"features\":[],\"filenames\":[\"/work/target/debug/deps/libratatui-0123.rlib\"],\"executable\":null,\"fresh\":false}";
    let finished = "{\"reason\":\"build-finished\",\"success\":true}";
    format!("{other}\n{artifact}\n{finished}\n")
}

fn nm_clean_llvm_nm_21() -> String {
    "0000000000000560 T <alloc::collections::btree::node::NodeRef<alloc::collections::btree::node::marker::Mut, jackin_console::domain::agent::Agent, alloc::string::String, alloc::collections::btree::node::marker::LeafOrInternal>>::fix_node_through_parent::<alloc::alloc::Global>\n0000000000001c14 T jackin_console::domain::workspace::Workspace::new\n".to_owned()
}

fn release_json_cargo_1_99_1(binary: &str, library_file: &str) -> String {
    let library = format!(
        "{{\"reason\":\"compiler-artifact\",\"package_id\":\"path+file:///work/crates/jackin#0.1.0\",\"manifest_path\":\"/work/crates/jackin/Cargo.toml\",\"target\":{{\"kind\":[\"lib\"],\"crate_types\":[\"lib\"],\"name\":\"jackin\",\"src_path\":\"/work/crates/jackin/src/lib.rs\",\"edition\":\"2024\",\"doc\":true,\"doctest\":true,\"test\":true}},\"profile\":{{\"opt_level\":\"3\",\"debuginfo\":0,\"debug_assertions\":false,\"overflow_checks\":false,\"test\":false}},\"features\":[],\"filenames\":[\"{library_file}\",\"{library_file}.rmeta\"],\"executable\":null,\"fresh\":false}}"
    );
    let bin = format!(
        "{{\"reason\":\"compiler-artifact\",\"package_id\":\"path+file:///work/crates/jackin#0.1.0\",\"manifest_path\":\"/work/crates/jackin/Cargo.toml\",\"target\":{{\"kind\":[\"bin\"],\"crate_types\":[\"bin\"],\"name\":\"jackin\",\"src_path\":\"/work/crates/jackin/src/main.rs\",\"edition\":\"2024\",\"doc\":true,\"doctest\":false,\"test\":true}},\"profile\":{{\"opt_level\":\"3\",\"debuginfo\":0,\"debug_assertions\":false,\"overflow_checks\":false,\"test\":false}},\"features\":[],\"filenames\":[\"{binary}\"],\"executable\":\"{binary}\",\"fresh\":false}}"
    );
    let finished = "{\"reason\":\"build-finished\",\"success\":true}";
    format!("{library}\n{bin}\n{finished}\n")
}

fn case_library(workspace: &Workspace) -> Result<String, Box<dyn std::error::Error>> {
    let library = workspace
        .root()
        .join("target/release/deps/libjackin-cccc.rlib");
    fs::create_dir_all(library.parent().ok_or("no parent")?)?;
    fs::write(&library, b"payments-platform")?;
    Ok(library.to_string_lossy().into_owned())
}

struct Case {
    workspace: Workspace,
    rlib: String,
    binary: String,
    library: String,
    replies: BTreeMap<String, io::Result<ToolOutput>>,
}

impl Case {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let workspace = green()?;
        let rlib = workspace
            .root()
            .join("target/debug/deps/libjackin_console-aaaa.rlib");
        let rlib = rlib.to_string_lossy().into_owned();
        let binary = workspace.root().join("target/release/jackin");
        fs::create_dir_all(binary.parent().ok_or("no parent")?)?;
        fs::write(&binary, b"jackin\0roster\0About\0")?;
        let binary = binary.to_string_lossy().into_owned();
        let library = case_library(&workspace)?;
        let mut case = Self {
            workspace,
            rlib,
            binary,
            library,
            replies: BTreeMap::new(),
        };
        case.reply(format!("cargo tree {TREE_JACKIN}"), ok(clean_tree()));
        case.reply(
            format!("cargo tree {TREE_TUI}"),
            ok(tree_cargo_1_99_1(
                "",
                &["jackin-tui v0.1.0 (/work/crates/jackin-tui) "],
            )),
        );
        case.reply(
            format!("cargo tree {TREE_CLAP}"),
            ok("clap v4.6.7\n└── jackin v0.1.0 (/work/crates/jackin)\n"),
        );
        let json = build_json_cargo_1_99_1(&case.rlib);
        case.reply(format!("cargo build {BUILD}"), ok(json));
        case.reply(format!("nm -C {}", case.rlib), ok(nm_clean_llvm_nm_21()));
        case.reply(
            format!("strings -a {}", case.rlib),
            ok("Workspace\nroster\nAbout\n"),
        );
        let json = release_json_cargo_1_99_1(&case.binary, &case.library);
        case.reply(format!("cargo build {RELEASE}"), ok(json));
        Ok(case)
    }

    fn reply(&mut self, key: String, reply: io::Result<ToolOutput>) {
        self.replies.insert(key, reply);
    }

    fn write_binary(&self, bytes: &[u8]) -> TestResult {
        fs::write(&self.binary, bytes)?;
        Ok(())
    }

    fn forget(&mut self, key: &str) {
        self.replies.remove(key);
    }

    fn run(self) -> Result<Vec<Violation>, Box<dyn std::error::Error>> {
        let repo = self.workspace.repo()?;
        let gate = DemoGateCheck::new(Script {
            replies: self.replies,
        });
        Ok(gate.run(&repo))
    }
}

impl Case {
    fn run_product(self) -> Result<Vec<Violation>, Box<dyn std::error::Error>> {
        let repo = self.workspace.repo()?;
        let gate = ProductCheck::new(Script {
            replies: self.replies,
        });
        Ok(gate.run(&repo))
    }
}

fn expect_one(violations: &[Violation], needle: &str) {
    assert_eq!(violations.len(), 1, "{violations:#?}");
    let message = &violations[0].message;
    assert!(message.contains(needle), "{needle:?} not in {message:?}");
}

#[test]
fn clean_texts_pass() -> TestResult {
    let violations = Case::new()?.run()?;
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn static_edge_fails() -> TestResult {
    let case = Case::new()?;
    case.workspace.write(
        "crates/jackin-tui/Cargo.toml",
        &member_manifest(
            "jackin-tui",
            "[dependencies]\njackin-console = { workspace = true, features = [\"demo\"] }\njunie-tui = { workspace = true }\n",
        ),
    )?;
    let violations = case.run()?;
    expect_one(&violations, "R3");
    Ok(())
}

#[test]
fn demo_feature_in_tree_fails() -> TestResult {
    let mut case = Case::new()?;
    case.reply(
        format!("cargo tree {TREE_JACKIN}"),
        ok(tree_cargo_1_99_1(
            "default,demo",
            &[
                "jackin-tui v0.1.0 (/work/crates/jackin-tui) ",
                "jackin v0.1.0 (/work/crates/jackin) ",
            ],
        )),
    );
    let violations = case.run()?;
    expect_one(
        &violations,
        "cargo tree -p jackin builds jackin-console with the feature demo",
    );
    Ok(())
}

#[test]
fn forwarded_demo_feature_fails() -> TestResult {
    let mut case = Case::new()?;
    case.reply(
        format!("cargo tree {TREE_TUI}"),
        ok(tree_cargo_1_99_1(
            "demo",
            &["jackin-tui v0.1.0 (/work/crates/jackin-tui) ui"],
        )),
    );
    let violations = case.run()?;
    expect_one(
        &violations,
        "cargo tree -p jackin-tui builds jackin-console with the feature demo",
    );
    Ok(())
}

#[test]
fn clap_below_console_fails() -> TestResult {
    let mut case = Case::new()?;
    case.reply(
        format!("cargo tree {TREE_CLAP}"),
        ok(format!(
            "clap v4.6.7 color,default,derive\n├── jackin v0.1.0 (/work/crates/jackin)\n└── junie-tui v0.1.0 ({CONSOLE_SOURCE}) cli\n    └── jackin-console v0.1.0 ({CONSOLE_SOURCE}) demo\n"
        )),
    );
    let violations = case.run()?;
    expect_one(&violations, "clap below jackin-console");
    Ok(())
}

#[test]
fn sim_symbol_fails() -> TestResult {
    let mut case = Case::new()?;
    let key = format!("nm -C {}", case.rlib);
    case.reply(
        key,
        ok(format!(
            "{}0000000000003664 T jackin_console::sim::SimBackend::perform\n",
            nm_clean_llvm_nm_21()
        )),
    );
    let violations = case.run()?;
    expect_one(&violations, "jackin_console::sim::SimBackend::perform");
    Ok(())
}

#[test]
fn trait_impl_symbol_fails() -> TestResult {
    let mut case = Case::new()?;
    let key = format!("nm -C {}", case.rlib);
    case.reply(
        key,
        ok(format!(
            "{}0000000000004a0c T <jackin_console::sim::SimBackend as jackin_console::backend::Backend>::perform::{{closure#1}}::{{closure#0}}\n",
            nm_clean_llvm_nm_21()
        )),
    );
    let violations = case.run()?;
    expect_one(&violations, "<jackin_console::sim::SimBackend as");
    Ok(())
}

#[test]
fn rlib_chosen_from_json_not_glob() -> TestResult {
    let mut case = Case::new()?;
    let named = case
        .workspace
        .root()
        .join("target/debug/deps/libjackin_console-aaaa.rlib");
    let demo = case
        .workspace
        .root()
        .join("target/debug/deps/libjackin_console-bbbb.rlib");
    fs::create_dir_all(named.parent().ok_or("no parent")?)?;
    fs::write(&named, "")?;
    fs::write(&demo, "")?;
    case.reply(
        format!("nm -C {}", demo.display()),
        ok("0000000000003664 T jackin_console::fixtures::populated::{closure#0}::{closure#0}\n"),
    );
    let violations = case.run()?;
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn no_match_is_clean() -> TestResult {
    let mut case = Case::new()?;
    case.reply(
        format!("cargo tree {TREE_JACKIN}"),
        failed(NO_MATCH_CARGO_1_99_1),
    );
    case.reply(
        format!("cargo tree {TREE_TUI}"),
        failed(NO_MATCH_CARGO_1_99_1),
    );
    case.reply(
        format!("cargo tree {TREE_CLAP}"),
        failed("error: package ID specification `clap` did not match any packages\n"),
    );
    let violations = case.run()?;
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn other_cargo_failure_is_an_error() -> TestResult {
    let mut case = Case::new()?;
    case.reply(
        format!("cargo tree {TREE_JACKIN}"),
        failed("error: failed to load manifest for workspace member\n"),
    );
    let violations = case.run()?;
    expect_one(&violations, "failed to load manifest");
    Ok(())
}

#[test]
fn canary_in_rlib_fails() -> TestResult {
    let mut case = Case::new()?;
    let key = format!("strings -a {}", case.rlib);
    case.reply(key, ok("roster\norg.jackin.workspace=payments-platform\n"));
    let violations = case.run()?;
    expect_one(&violations, "payments-platform");
    Ok(())
}

#[test]
fn product_clean_binary_passes() -> TestResult {
    let violations = Case::new()?.run_product()?;
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn canary_in_binary_fails() -> TestResult {
    let case = Case::new()?;
    case.write_binary(b"roster\0org.jackin.workspace=payments-platform\0")?;
    let binary = case.binary.clone();
    let violations = case.run_product()?;
    expect_one(&violations, "payments-platform");
    assert!(violations[0].message.contains(&binary), "{violations:#?}");
    Ok(())
}

#[test]
fn each_canary_in_binary_fails_by_name() -> TestResult {
    for canary in DEMO_CANARIES {
        let case = Case::new()?;
        case.write_binary(format!("roster\0before {canary} after\0").as_bytes())?;
        let violations = case.run_product()?;
        expect_one(&violations, canary);
    }
    Ok(())
}

#[test]
fn only_the_executable_is_scanned() -> TestResult {
    let case = Case::new()?;
    assert_eq!(fs::read(&case.library)?, b"payments-platform");
    let violations = case.run_product()?;
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn a_release_build_failure_is_an_error() -> TestResult {
    let mut case = Case::new()?;
    case.reply(
        format!("cargo build {RELEASE}"),
        failed("error[E0432]: unresolved import `crate::missing`\n"),
    );
    let violations = case.run_product()?;
    expect_one(&violations, "unresolved import");
    Ok(())
}

#[test]
fn a_build_without_an_executable_is_an_error() -> TestResult {
    let mut case = Case::new()?;
    case.reply(
        format!("cargo build {RELEASE}"),
        ok("{\"reason\":\"build-finished\",\"success\":true}\n"),
    );
    let violations = case.run_product()?;
    expect_one(&violations, "names no executable");
    Ok(())
}

#[test]
fn a_demo_feature_stops_the_product_check_before_the_release_build() -> TestResult {
    let mut case = Case::new()?;
    case.forget(&format!("cargo build {RELEASE}"));
    case.reply(
        format!("cargo tree {TREE_JACKIN}"),
        ok(tree_cargo_1_99_1(
            "default,demo",
            &[
                "jackin-tui v0.1.0 (/work/crates/jackin-tui) ",
                "jackin v0.1.0 (/work/crates/jackin) ",
            ],
        )),
    );
    let violations = case.run_product()?;
    expect_one(
        &violations,
        "cargo tree -p jackin builds jackin-console with the feature demo",
    );
    Ok(())
}

#[test]
fn a_demo_symbol_in_the_rlib_stops_the_product_check() -> TestResult {
    let mut case = Case::new()?;
    case.forget(&format!("cargo build {RELEASE}"));
    let key = format!("nm -C {}", case.rlib);
    case.reply(
        key,
        ok(format!(
            "{}0000000000003664 T jackin_console::sim::SimBackend::perform\n",
            nm_clean_llvm_nm_21()
        )),
    );
    let violations = case.run_product()?;
    expect_one(&violations, "jackin_console::sim::SimBackend::perform");
    Ok(())
}

#[test]
fn an_executable_that_cannot_be_read_is_an_error() -> TestResult {
    let case = Case::new()?;
    fs::remove_file(&case.binary)?;
    let binary = case.binary.clone();
    let violations = case.run_product()?;
    expect_one(&violations, "cannot read");
    assert!(violations[0].message.contains(&binary), "{violations:#?}");
    Ok(())
}
