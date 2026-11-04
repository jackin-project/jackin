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
    expect_with(ScreenCheck::production(), source, expected)
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
fn buffer_set_fails() -> TestResult {
    expect(
        "fn f(buffer: &mut Buffer) {\n    buffer.set_string(0, 0, 1, 2);\n}\n",
        &[("C1", 2, "buffer.set_string")],
    )
}

#[test]
fn real_tree_passes() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let violations = ScreenCheck::production().run(&Repo::open(root)?);
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}
