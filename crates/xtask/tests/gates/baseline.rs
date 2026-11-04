// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::workspace::{
    OPENSHELL_URL, OTHER_REV, REV, TERMPANE_URL, TUISCOTTI_URL, TestResult, URL, Workspace,
    add_member, console_line, deny_text, green, junie_line, lock_source, lock_text,
    member_manifest, pin, testkit_manifest, workspace_manifest,
};
use jackin_xtask::{BaselineCheck, Check, Repo, Violation};

const TUI: &str = "crates/jackin-tui/Cargo.toml";

type Expected = (&'static str, &'static str, &'static str);

fn run(workspace: &Workspace) -> Result<Vec<Violation>, Box<dyn std::error::Error>> {
    Ok(BaselineCheck.run(&workspace.repo()?))
}

fn expect(workspace: &Workspace, expected: &[Expected]) -> TestResult {
    let violations = run(workspace)?;
    let found: Vec<(String, String, String)> = violations
        .iter()
        .map(|violation| {
            (
                violation.message.chars().take(2).collect(),
                violation
                    .path
                    .as_deref()
                    .map(Path::to_string_lossy)
                    .map(String::from)
                    .unwrap_or_default(),
                violation.message.clone(),
            )
        })
        .collect();
    let shape: Vec<(&str, &str)> = found
        .iter()
        .map(|(rule, file, _)| (rule.as_str(), file.as_str()))
        .collect();
    let wanted: Vec<(&str, &str)> = expected
        .iter()
        .map(|(rule, file, _)| (*rule, *file))
        .collect();
    assert_eq!(shape, wanted, "{found:#?}");
    for ((_, _, message), (_, _, needle)) in found.iter().zip(expected) {
        assert!(message.contains(needle), "{needle:?} not in {message:?}");
    }
    Ok(())
}

fn edit(workspace: &Workspace, relative: &str, from: &str, to: &str) -> TestResult {
    let path = workspace.root().join(relative);
    let text = std::fs::read_to_string(&path)?;
    assert!(text.contains(from), "{from:?} not in {relative}");
    workspace.write(relative, &text.replace(from, to))
}

fn root_with_console(line: &str) -> String {
    workspace_manifest(&["jackin-tui"], &[line.to_owned(), junie_line(REV)])
}

fn with_console_line(line: &str) -> Result<Workspace, Box<dyn std::error::Error>> {
    let workspace = green()?;
    workspace.write("Cargo.toml", &root_with_console(line))?;
    Ok(workspace)
}

fn with_testkit() -> Result<Workspace, Box<dyn std::error::Error>> {
    let workspace = green()?;
    add_member(&workspace, "jackin-testkit", &testkit_manifest())?;
    Ok(workspace)
}

#[test]
fn exact_pin_passes() -> TestResult {
    expect(&green()?, &[])
}

#[test]
fn testkit_exemption_passes() -> TestResult {
    expect(&with_testkit()?, &[])
}

#[test]
fn testkit_path_dependency_fails() -> TestResult {
    let workspace = with_testkit()?;
    workspace.write(
        "crates/jackin-testkit/Cargo.toml",
        &member_manifest(
            "jackin-testkit",
            "[dependencies]\njackin-console = { path = \"../../../elsewhere/jackin-console\", default-features = false, features = [\"demo\"] }\n",
        ),
    )?;
    expect(
        &workspace,
        &[("R1", "crates/jackin-testkit/Cargo.toml", "path dependency")],
    )
}

#[test]
fn testkit_exemption_covers_demo_only() -> TestResult {
    let workspace = with_testkit()?;
    workspace.write(
        "crates/jackin-testkit/Cargo.toml",
        &member_manifest(
            "jackin-testkit",
            "[dependencies]\njackin-console = { version = \"0.1.0\", git = \"https://github.com/tailrocks/terminal-components-baseline\", rev = \"74244972a1183db85111332ddab7eeabcbae81cb\", features = [\"demo\"] }\n",
        ),
    )?;
    expect(
        &workspace,
        &[("R3", "crates/jackin-testkit/Cargo.toml", "default features")],
    )
}

#[test]
fn exemption_is_limited_to_testkit() -> TestResult {
    let workspace = green()?;
    add_member(
        &workspace,
        "jackin-session",
        &member_manifest(
            "jackin-session",
            "[dependencies]\njackin-console = { workspace = true, features = [\"demo\"] }\n",
        ),
    )?;
    expect(
        &workspace,
        &[("R3", "crates/jackin-session/Cargo.toml", "demo")],
    )
}

#[test]
fn path_dependency_fails() -> TestResult {
    let workspace = with_console_line(
        "jackin-console = { version = \"0.1.0\", path = \"../elsewhere/jackin-console\", default-features = false }",
    )?;
    expect(&workspace, &[("R1", TUI, "path dependency")])
}

#[test]
fn renamed_path_fails() -> TestResult {
    let workspace = green()?;
    workspace.write(
        TUI,
        &member_manifest(
            "jackin-tui",
            "[dependencies]\njackin-console = { workspace = true }\njunie-tui = { workspace = true }\nlocal-ui = { package = \"junie-tui\", path = \"../../../elsewhere/junie-tui\", default-features = false }\n",
        ),
    )?;
    expect(&workspace, &[("R1", TUI, "path dependency")])
}

#[test]
fn other_url_fails() -> TestResult {
    let line = console_line(REV).replace(URL, "https://github.com/someone/baseline");
    expect(&with_console_line(&line)?, &[("R1", TUI, "another URL")])
}

#[test]
fn dot_git_url_fails() -> TestResult {
    let line = console_line(REV).replace(URL, &format!("{URL}.git"));
    expect(
        &with_console_line(&line)?,
        &[("R1", TUI, ".git suffix"), ("R1", TUI, ".git suffix")],
    )
}

#[test]
fn branch_fails() -> TestResult {
    let line = console_line(REV).replace(
        &format!("rev = \"{REV}\""),
        "branch = \"feature/openshell-native\"",
    );
    expect(&with_console_line(&line)?, &[("R1", TUI, "branch")])
}

#[test]
fn tag_fails() -> TestResult {
    let line = console_line(REV).replace(&format!("rev = \"{REV}\""), "tag = \"v0.1.0\"");
    expect(&with_console_line(&line)?, &[("R1", TUI, "tag")])
}

#[test]
fn no_reference_fails() -> TestResult {
    let line = console_line(REV).replace(&format!(", rev = \"{REV}\""), "");
    expect(&with_console_line(&line)?, &[("R1", TUI, "no reference")])
}

#[test]
fn short_rev_fails() -> TestResult {
    let line = console_line(REV).replace(REV, "7424497");
    expect(&with_console_line(&line)?, &[("R1", TUI, "short rev")])
}

#[test]
fn uppercase_rev_fails() -> TestResult {
    let line = console_line(REV).replace(REV, &REV.to_uppercase());
    expect(&with_console_line(&line)?, &[("R1", TUI, "uppercase")])
}

#[test]
fn rev_is_a_branch_name_fails() -> TestResult {
    let line = console_line(REV).replace(REV, "feature/openshell-native");
    expect(
        &with_console_line(&line)?,
        &[("R1", TUI, "not hexadecimal")],
    )
}

#[test]
fn two_revs_fail() -> TestResult {
    let workspace = with_console_line(&console_line(OTHER_REV))?;
    workspace.write(
        "Cargo.lock",
        &lock_text(&[
            ("jackin-console", lock_source(OTHER_REV)),
            ("junie-tui", lock_source(REV)),
        ]),
    )?;
    expect(&workspace, &[("R2", "Cargo.toml", OTHER_REV)])
}

#[test]
fn normal_edge_demo_fails() -> TestResult {
    let workspace = green()?;
    workspace.write(
        TUI,
        &member_manifest(
            "jackin-tui",
            "[dependencies]\njackin-console = { workspace = true, features = [\"demo\"] }\njunie-tui = { workspace = true }\n",
        ),
    )?;
    expect(&workspace, &[("R3", TUI, "demo")])
}

#[test]
fn dev_edge_demo_passes() -> TestResult {
    let workspace = green()?;
    workspace.write(
        TUI,
        &member_manifest(
            "jackin-tui",
            "[dependencies]\njackin-console = { workspace = true }\njunie-tui = { workspace = true }\n\n[dev-dependencies]\njackin-console = { workspace = true, features = [\"demo\"] }\n",
        ),
    )?;
    expect(&workspace, &[])
}

#[test]
fn default_features_fail() -> TestResult {
    let line = console_line(REV).replace(", default-features = false", "");
    expect(
        &with_console_line(&line)?,
        &[("R3", TUI, "default features")],
    )
}

#[test]
fn junie_cli_fails() -> TestResult {
    let workspace = green()?;
    workspace.write(
        TUI,
        &member_manifest(
            "jackin-tui",
            "[dependencies]\njackin-console = { workspace = true }\njunie-tui = { workspace = true, features = [\"cli\"] }\n",
        ),
    )?;
    expect(&workspace, &[("R3", TUI, "cli")])?;

    let line = junie_line(REV).replace(", default-features = false", "");
    let workspace = green()?;
    workspace.write(
        "Cargo.toml",
        &workspace_manifest(&["jackin-tui"], &[console_line(REV), line]),
    )?;
    expect(&workspace, &[("R3", TUI, "default features")])
}

#[test]
fn forwarded_feature_fails() -> TestResult {
    for forward in ["jackin-console/demo", "jackin-console?/demo"] {
        let workspace = green()?;
        workspace.write(
            TUI,
            &member_manifest(
                "jackin-tui",
                &format!(
                    "[features]\nui = [\"{forward}\"]\n\n[dependencies]\njackin-console = {{ workspace = true, optional = true }}\njunie-tui = {{ workspace = true }}\n"
                ),
            ),
        )?;
        expect(&workspace, &[("R3", TUI, forward)])?;
    }
    Ok(())
}

#[test]
fn renamed_forward_fails() -> TestResult {
    let workspace = green()?;
    workspace.write(
        TUI,
        &member_manifest(
            "jackin-tui",
            &format!(
                "[features]\nui = [\"console/demo\"]\n\n[dependencies]\nconsole = {{ package = \"jackin-console\", version = \"0.1.0\", {}, default-features = false, optional = true }}\njunie-tui = {{ workspace = true }}\n",
                pin(REV)
            ),
        ),
    )?;
    expect(&workspace, &[("R3", TUI, "console/demo")])
}

#[test]
fn patch_fails() -> TestResult {
    for patch in [
        format!("[patch.\"{URL}\"]\nother = {{ path = \"../patched\" }}\n"),
        "[patch.crates-io]\njunie-tui = { path = \"../patched\" }\n".to_owned(),
    ] {
        let workspace = green()?;
        edit(
            &workspace,
            "Cargo.toml",
            "[workspace.package]",
            &format!("{patch}\n[workspace.package]"),
        )?;
        expect(&workspace, &[("R4", "Cargo.toml", "patch")])?;
    }
    Ok(())
}

#[test]
fn patch_dot_git_fails() -> TestResult {
    let workspace = green()?;
    let patch = format!("[patch.\"{URL}.git\"]\nother = {{ path = \"../patched\" }}\n");
    edit(
        &workspace,
        "Cargo.toml",
        "[workspace.package]",
        &format!("{patch}\n[workspace.package]"),
    )?;
    expect(&workspace, &[("R4", "Cargo.toml", "patch")])
}

#[test]
fn patch_package_key_fails() -> TestResult {
    let workspace = green()?;
    let patch =
        "[patch.crates-io]\nsomething = { path = \"../patched\", package = \"junie-tui\" }\n";
    edit(
        &workspace,
        "Cargo.toml",
        "[workspace.package]",
        &format!("{patch}\n[workspace.package]"),
    )?;
    expect(&workspace, &[("R4", "Cargo.toml", "junie-tui")])
}

#[test]
fn replace_fails() -> TestResult {
    let workspace = green()?;
    let replace = format!("[replace]\n\"{URL}#junie-tui:0.1.0\" = {{ path = \"../patched\" }}\n");
    edit(
        &workspace,
        "Cargo.toml",
        "[workspace.package]",
        &format!("{replace}\n[workspace.package]"),
    )?;
    expect(&workspace, &[("R4", "Cargo.toml", "replace")])
}

#[test]
fn cargo_config_paths_fail() -> TestResult {
    let workspace = green()?;
    workspace.write(
        ".cargo/config.toml",
        "paths = [\"../terminal-components-baseline\"]\n",
    )?;
    expect(&workspace, &[("R4", ".cargo/config.toml", "paths")])
}

fn replacing_source(name: &str, url: &str) -> String {
    format!(
        "[source.{name}]\ngit = \"{url}\"\nrev = \"{REV}\"\nreplace-with = \"vendored\"\n\n[source.vendored]\ndirectory = \"vendor\"\n"
    )
}

#[test]
fn source_replace_fails() -> TestResult {
    let cases: [(&str, String); 8] = [
        (".cargo/config.toml", replacing_source("baseline", URL)),
        (".cargo/config", replacing_source("baseline", URL)),
        (
            "crates/jackin-tui/.cargo/config.toml",
            replacing_source("baseline", URL),
        ),
        (
            "crates/jackin-tui/.cargo/config",
            replacing_source("baseline", URL),
        ),
        (
            ".cargo/config.toml",
            "[source.crates-io]\nreplace-with = \"vendored\"\n\n[source.vendored]\ndirectory = \"vendor\"\n"
                .to_owned(),
        ),
        (
            ".cargo/config.toml",
            format!("[source.mirror]\ngit = \"{URL}.git\"\n"),
        ),
        (
            "crates/jackin-tui/.cargo/config.toml",
            format!("[source.mirror]\ngit = \"{TERMPANE_URL}\"\n"),
        ),
        (
            "crates/jackin-tui/.cargo/config.toml",
            format!("[source.mirror]\ngit = \"{TUISCOTTI_URL}\"\n"),
        ),
    ];
    for (path, text) in cases {
        let workspace = green()?;
        workspace.write(path, &text)?;
        expect(&workspace, &[("R4", path, "[source.")])?;
    }
    let workspace = green()?;
    workspace.write(
        ".cargo/config.toml",
        "[source.vendored]\ndirectory = \"vendor\"\n",
    )?;
    expect(&workspace, &[])
}

#[test]
fn lock_source_mismatch_fails() -> TestResult {
    let workspace = green()?;
    workspace.write(
        "Cargo.lock",
        &lock_text(&[
            ("jackin-console", lock_source(OTHER_REV)),
            ("junie-tui", lock_source(REV)),
        ]),
    )?;
    expect(&workspace, &[("R5", "Cargo.lock", "jackin-console")])?;

    let workspace = green()?;
    workspace.write(
        "Cargo.lock",
        "version = 4\n\n[[package]]\nname = \"jackin-console\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"junie-tui\"\nversion = \"0.1.0\"\nsource = \"git+https://github.com/tailrocks/terminal-components-baseline?rev=74244972a1183db85111332ddab7eeabcbae81cb#74244972a1183db85111332ddab7eeabcbae81cb\"\n",
    )?;
    expect(&workspace, &[("R5", "Cargo.lock", "jackin-console")])
}

#[test]
fn allow_git_exact() -> TestResult {
    let cases = [
        vec![
            OPENSHELL_URL,
            URL,
            TERMPANE_URL,
            TUISCOTTI_URL,
            "https://github.com/someone/else",
        ],
        vec![OPENSHELL_URL, URL, TERMPANE_URL],
        vec![OPENSHELL_URL, TERMPANE_URL, TUISCOTTI_URL],
        vec![OPENSHELL_URL, URL, TUISCOTTI_URL],
    ];
    for urls in cases {
        let workspace = green()?;
        workspace.write("deny.toml", &deny_text(&urls, "deny"))?;
        expect(&workspace, &[("R6", "deny.toml", "allow-git")])?;
    }
    let workspace = green()?;
    workspace.write(
        "deny.toml",
        &deny_text(&[TUISCOTTI_URL, TERMPANE_URL, URL, OPENSHELL_URL], "deny"),
    )?;
    expect(&workspace, &[])
}

#[test]
fn unknown_git_must_deny() -> TestResult {
    let workspace = green()?;
    workspace.write(
        "deny.toml",
        &deny_text(&[OPENSHELL_URL, URL, TERMPANE_URL, TUISCOTTI_URL], "warn"),
    )?;
    expect(&workspace, &[("R6", "deny.toml", "unknown-git")])
}

#[test]
fn real_repo_passes() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repo = Repo::open(root)?;
    let violations = BaselineCheck.run(&repo);
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}
