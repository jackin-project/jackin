// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_xtask::{Check, Repo, ScreenCheck};

use crate::workspace::{TestResult, Workspace, member_manifest, workspace_manifest};

const LIB: &str = "crates/jackin-tui/src/lib.rs";

type Hit = (String, Option<u32>, String);

fn tui(source: &str) -> Result<Workspace, Box<dyn std::error::Error>> {
    let workspace = Workspace::empty()?;
    workspace.write("Cargo.toml", &workspace_manifest(&["jackin-tui"], &[]))?;
    workspace.write(
        "crates/jackin-tui/Cargo.toml",
        &member_manifest("jackin-tui", ""),
    )?;
    workspace.write(LIB, source)?;
    Ok(workspace)
}

fn found(
    check: ScreenCheck,
    workspace: &Workspace,
) -> Result<Vec<Hit>, Box<dyn std::error::Error>> {
    Ok(check
        .run(&workspace.repo()?)
        .into_iter()
        .map(|violation| {
            assert_eq!(
                violation.path.as_deref(),
                Some(Path::new(LIB)),
                "{violation}"
            );
            (
                violation.message.chars().take(2).collect(),
                violation.line,
                violation.message,
            )
        })
        .collect())
}

fn expect(source: &str, expected: &[(&str, u32, &str)]) -> TestResult {
    expect_with(ScreenCheck::with_allowed(&[]), source, expected)
}

fn expect_with(check: ScreenCheck, source: &str, expected: &[(&str, u32, &str)]) -> TestResult {
    let violations = found(check, &tui(source)?)?;
    let shape: Vec<(&str, Option<u32>)> = violations
        .iter()
        .map(|(rule, line, _)| (rule.as_str(), *line))
        .collect();
    let wanted: Vec<(&str, Option<u32>)> = expected
        .iter()
        .map(|(rule, line, _)| (*rule, Some(*line)))
        .collect();
    assert_eq!(shape, wanted, "{violations:#?}");
    for ((_, _, message), (_, _, needle)) in violations.iter().zip(expected) {
        assert!(message.contains(needle), "{needle:?} not in {message:?}");
    }
    Ok(())
}

#[test]
fn literals_in_display_pass() -> TestResult {
    expect(
        "use std::fmt;\n\
         struct Row;\n\
         impl std::error::Error for Row {}\n\
         impl fmt::Display for Row {\n\
         \x20   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n\
         \x20       fn label() -> &'static str {\n\
         \x20           \"Saved\"\n\
         \x20       }\n\
         \x20       write!(f, \"{} ok\", label())\n\
         \x20   }\n\
         }\n\
         impl<T: Clone> std::fmt::Debug for Wrapper<T> {\n\
         \x20   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n\
         \x20       f.write_str(\"Wrapper\")\n\
         \x20   }\n\
         }\n\
         fn trace(count: usize) {\n\
         \x20   tracing::info!(\"opened {count} panes\");\n\
         \x20   warn!(\"slow\");\n\
         }\n\
         #[derive(Debug, thiserror::Error)]\n\
         enum Failure {\n\
         \x20   #[error(\"terminal error: {0}\")]\n\
         \x20   Terminal(String),\n\
         }\n\
         #[cfg(test)]\n\
         mod tests {\n\
         \x20   fn text() -> &'static str {\n\
         \x20       \"Saved\"\n\
         \x20   }\n\
         }\n",
        &[],
    )
}

#[test]
fn display_literal_in_non_error_fails() -> TestResult {
    expect(
        "use std::fmt;\n\
         struct Row;\n\
         impl fmt::Display for Row {\n\
         \x20   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n\
         \x20       write!(f, \"Saved\")\n\
         \x20   }\n\
         }\n\
         struct Other;\n\
         impl std::error::Error for Other {}\n",
        &[("C2", 5, "Saved")],
    )
}

#[test]
fn allowed_literals_pass() -> TestResult {
    const ALLOWED: &[&str] = &["JACKIN_NO_MOTION", "codex"];
    expect_with(
        ScreenCheck::with_allowed(ALLOWED),
        "fn motion() -> bool {\n\
         \x20   std::env::var_os(\"JACKIN_NO_MOTION\").is_some()\n\
         }\n\
         fn program() -> std::process::Command {\n\
         \x20   std::process::Command::new(\"codex\")\n\
         }\n",
        &[],
    )?;
    expect_with(
        ScreenCheck::with_allowed(ALLOWED),
        "fn motion() -> bool {\n\
         \x20   std::env::var_os(\"JACKIN_NO_MOTION\").is_some()\n\
         }\n\
         fn lookalike() -> bool {\n\
         \x20   std::env::var_os(\"JACKIN_NO_MOTION_X\").is_some()\n\
         }\n\
         fn program() -> std::process::Command {\n\
         \x20   std::process::Command::new(\"codex\")\n\
         }\n",
        &[("C2", 5, "JACKIN_NO_MOTION_X")],
    )
}

#[test]
fn production_allowed_literals_pass() -> TestResult {
    expect_with(
        ScreenCheck::production(),
        "fn motion() -> bool {\n\
         \x20   std::env::var_os(\"JACKIN_NO_MOTION\").is_some()\n\
         }\n\
         const GIT: &str = \"git\";\n",
        &[("C2", 4, "git")],
    )
}

#[test]
fn stale_allowed_literal_fails() -> TestResult {
    const ALLOWED: &[&str] = &["JACKIN_NO_MOTION", "NEVER_USED"];
    let workspace =
        tui("fn motion() -> bool {\n    std::env::var_os(\"JACKIN_NO_MOTION\").is_some()\n}\n")?;
    let violations = ScreenCheck::with_allowed(ALLOWED).run(&workspace.repo()?);
    assert_eq!(violations.len(), 1, "{violations:#?}");
    let message = &violations[0].message;
    assert!(
        message.starts_with("C2") && message.contains("NEVER_USED"),
        "{message}"
    );
    assert!(!message.contains("JACKIN_NO_MOTION"), "{message}");
    Ok(())
}

#[test]
fn literal_in_function_fails() -> TestResult {
    expect(
        "fn announce(cx: &mut Cx) {\n    cx.status(\"Saved\");\n}\n",
        &[("C2", 2, "Saved")],
    )
}

#[test]
fn format_literal_fails() -> TestResult {
    expect(
        "fn link(url: &str) -> String {\n    format!(\"Opened {url}\")\n}\n",
        &[("C2", 2, "Opened {url}")],
    )
}

#[test]
fn const_literal_fails() -> TestResult {
    expect("const NAME: &str = \"Saved\";\n", &[("C2", 1, "Saved")])
}

#[test]
fn raw_string_fails() -> TestResult {
    expect(
        "fn name() -> &'static str {\n    r\"Saved\"\n}\n",
        &[("C2", 2, "Saved")],
    )
}

#[test]
fn macro_literal_fails() -> TestResult {
    expect(
        "fn f(x: &str) -> bool {\n    matches!(x, \"a\")\n}\nfn g() -> Vec<&'static str> {\n    vec![\"b\"]\n}\n",
        &[("C2", 2, "\"a\""), ("C2", 5, "\"b\"")],
    )
}

#[test]
fn ratatui_allowed_imports_pass() -> TestResult {
    expect(
        "use ratatui::Frame;\n\
         use ratatui::layout::Rect;\n\
         use ratatui::buffer::Buffer;\n\
         use ratatui::crossterm::event::{KeyCode, KeyEvent};\n\
         use ratatui::{Frame as Surface, buffer::Buffer as Cells};\n\
         fn draw(frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {}\n",
        &[],
    )
}

#[test]
fn ratatui_widgets_fails() -> TestResult {
    expect(
        "use ratatui::widgets::Paragraph;\n",
        &[("C1", 1, "ratatui::widgets::Paragraph")],
    )
}

#[test]
fn ratatui_style_fails() -> TestResult {
    expect(
        "fn f() -> ratatui::style::Style {\n    ratatui::style::Style::default()\n}\n",
        &[
            ("C1", 1, "ratatui::style::Style"),
            ("C1", 2, "ratatui::style::Style"),
        ],
    )
}

#[test]
fn ratatui_layout_fails() -> TestResult {
    expect(
        "use ratatui::layout::{Constraint, Layout, Rect};\n",
        &[
            ("C1", 1, "ratatui::layout::Constraint"),
            ("C1", 1, "ratatui::layout::Layout"),
        ],
    )
}

#[test]
fn ratatui_group_import_fails() -> TestResult {
    expect(
        "use ratatui::{Frame, widgets::Paragraph, text::{Line, Span}};\n",
        &[
            ("C1", 1, "ratatui::text::Line"),
            ("C1", 1, "ratatui::text::Span"),
            ("C1", 1, "ratatui::widgets::Paragraph"),
        ],
    )
}

#[test]
fn ratatui_prelude_fails() -> TestResult {
    expect(
        "use ratatui::prelude::*;\n",
        &[("C1", 1, "ratatui::prelude::*")],
    )
}

#[test]
fn ratatui_paths_with_a_leading_separator_fail() -> TestResult {
    expect(
        "use ::ratatui::widgets::Paragraph;\npub use ::ratatui::style::Style;\nimpl ::ratatui::widgets::Widget for X {}\nfn f(_: &dyn ::ratatui::text::Span) {}\n",
        &[
            ("C1", 1, "ratatui::widgets::Paragraph"),
            ("C1", 2, "ratatui::style::Style"),
            ("C1", 3, "ratatui::widgets::Widget"),
            ("C1", 4, "ratatui::text::Span"),
        ],
    )
}

#[test]
fn a_module_that_is_merely_named_ratatui_is_not_the_crate() -> TestResult {
    expect("use crate::support::ratatui::widgets::Paragraph;\n", &[])
}

#[test]
fn a_source_cut_off_inside_a_tracing_macro_is_not_a_panic() -> TestResult {
    for source in [
        "fn f() {\n    info!",
        "fn f() {\n    tracing::warn!(",
        "#[error(",
    ] {
        expect(source, &[])?;
    }
    Ok(())
}

#[test]
fn indexing_a_buffer_fails() -> TestResult {
    expect(
        "fn draw(buf: &mut Buffer, pos: Position) {\n\
         \x20   buf[(0, 0)] = Cell::default();\n\
         \x20   buf[pos].set_fg(Color::Red);\n\
         \x20   let seen = &buf[(1, 1)];\n\
         \x20   self.buf[pos] = Cell::default();\n\
         \x20   frame.buffer_mut()[pos].reset();\n\
         \x20   grid[0][1] = 2;\n\
         }\n",
        &[
            ("C1", 2, "indexes"),
            ("C1", 3, "indexes"),
            ("C1", 4, "indexes"),
            ("C1", 5, "indexes"),
            ("C1", 6, "indexes"),
            ("C1", 7, "indexes"),
            ("C1", 7, "indexes"),
        ],
    )
}

#[test]
fn brackets_that_are_not_an_index_pass() -> TestResult {
    expect(
        "#[derive(Debug)]\n\
         #![deny(unused)]\n\
         struct Row {\n\
         \x20   bytes: [u8; 4],\n\
         \x20   rest: &'static [u8],\n\
         \x20   nested: Vec<[u8; 2]>,\n\
         }\n\
         fn f(x: &mut [u8]) -> [u8; 2] {\n\
         \x20   let a = [1, 2];\n\
         \x20   for item in [1, 2] {}\n\
         \x20   let v = vec![1, 2];\n\
         \x20   if let [first, ..] = x {}\n\
         \x20   return [0, 0];\n\
         \x20   if [1, 2].contains(&3) {}\n\
         \x20   while [1, 2].is_empty() {}\n\
         \x20   match [1, 2] { _ => {} }\n\
         \x20   let tail = &x[1..];\n\
         \x20   let all = &[][..];\n\
         \x20   let mid = &x[a..=b];\n\
         }\n\
         #[cfg(test)]\n\
         mod tests {\n\
         \x20   fn read(buf: &Buffer) {\n\
         \x20       let cell = &buf[(0, 0)];\n\
         \x20   }\n\
         }\n",
        &[],
    )
}

#[test]
fn silencing_the_drawing_ban_fails() -> TestResult {
    expect(
        "#![expect(clippy::disallowed_methods)]\n\
         #[allow(clippy::disallowed_methods)]\n\
         fn a() {}\n\
         #[cfg_attr(test, expect(clippy::too_many_lines, clippy::disallowed_methods))]\n\
         fn b() {}\n",
        &[
            ("C1", 1, "disallowed_methods"),
            ("C1", 2, "disallowed_methods"),
            ("C1", 4, "disallowed_methods"),
        ],
    )
}

#[test]
fn other_attributes_pass() -> TestResult {
    expect(
        "#![deny(clippy::disallowed_methods)]\n\
         #[expect(clippy::too_many_lines)]\n\
         #[allow(dead_code)]\n\
         fn a() {}\n",
        &[],
    )
}

#[test]
fn real_tree_passes() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let violations = ScreenCheck::production().run(&Repo::open(root)?);
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}
