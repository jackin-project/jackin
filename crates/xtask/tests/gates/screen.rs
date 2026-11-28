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
    expect_workspace(check, &tui(source)?, expected)
}

fn expect_workspace(
    check: ScreenCheck,
    workspace: &Workspace,
    expected: &[(&str, u32, &str)],
) -> TestResult {
    let violations = found(check, workspace)?;
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
         use ratatui::crossterm::event::{KeyCode, KeyEvent};\n\
         use ratatui::{Frame as Surface, buffer::Buffer};\n\
         fn draw(frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {}\n",
        &[],
    )
}

#[test]
fn ratatui_buffer_renames_preserve_path_provenance() -> TestResult {
    expect(
        "use ratatui::{Frame as Surface, buffer::Buffer as Cells};\n\
         use crate::model::Buffer as Domain;\n\
         use crate::model::{Buffer as Other};\n\
         type Grid = crate::model::Buffer;\n",
        &[("C1", 1, "renames")],
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
         fn b() {}\n\
         #[allow(clippy::disallowed_fields)]\n\
         fn c() {}\n\
         #![expect(clippy::disallowed_fields)]\n",
        &[
            ("C1", 1, "disallowed_methods"),
            ("C1", 2, "disallowed_methods"),
            ("C1", 4, "disallowed_methods"),
            ("C1", 6, "disallowed_fields"),
            ("C1", 8, "disallowed_fields"),
        ],
    )
}

#[test]
fn destructuring_a_buffer_fails() -> TestResult {
    expect(
        "use ratatui::buffer::Buffer;\n\
         fn a(buf: Buffer) {\n\
         \x20   let Buffer { content, .. } = buf;\n\
         }\n\
         fn b(buf: ratatui::buffer::Buffer) {\n\
         \x20   let ratatui::buffer::Buffer { area, .. } = buf;\n\
         }\n\
         fn c(buf: &mut Buffer) {\n\
         \x20   if let Buffer { content, .. } = buf {}\n\
         }\n\
         fn d() -> Buffer {\n\
         \x20   Buffer::empty(Default::default())\n\
         }\n\
         fn e(Buffer { area, .. }: Buffer) {}\n\
         fn f(x: &Buffer) -> &Buffer {\n\
         \x20   x\n\
         }\n\
         fn g<'a>(x: &'a Buffer) -> &'a Buffer {\n\
         \x20   x\n\
         }\n\
         fn h(x: &mut Buffer) -> &mut Buffer {\n\
         \x20   x\n\
         }\n\
         impl Draw for &Buffer {\n\
         \x20   fn draw(&self) {}\n\
         }\n\
         fn i(buf: Buffer) {\n\
         \x20   let Buffer { ref content, .. } = buf;\n\
         }\n\
         fn j() -> ratatui::buffer::Buffer {\n\
         \x20   Default::default()\n\
         }\n\
         fn k(x: u8, area: Rect, content: Vec<u8>) {\n\
         \x20   let _ = match x { _ => Buffer { area, content } };\n\
         }\n\
         #[cfg(test)]\n\
         mod tests {\n\
         \x20   fn t(b: Buffer) { let Buffer { content, .. } = b; }\n\
         }\n\
         fn m(b: &Buffer) { let &Buffer { ref content, .. } = b; }\n\
         fn n(bs: Vec<Buffer>) { for Buffer { content, .. } in bs {} }\n\
         fn o(b: Buffer) { let Buffer { #[allow(unused)] content, .. } = b; }\n\
         fn own(d: u8) { let crate::model::Buffer { value, .. } = d; }\n\
         fn local(d: u8) { let crate::buffer::Buffer { value, .. } = d; }\n\
         enum View { Buffer { value: usize } }\n\
         union Buffer { content: usize }\n\
         fn closure(buf: Buffer) { let _ = { enum E {} || { let Buffer { content, .. } = buf; } }; }\n\
         fn q() -> ::ratatui::buffer::Buffer {\n\
         \x20   Default::default()\n\
         }\n\
         fn ptr() -> *mut Buffer {\n\
         \x20   std::ptr::null_mut()\n\
         }\n\
         use ratatui::buffer::Buffer as Cells;\n\
         type Grid = ratatui::buffer::Buffer;\n\
         #[cfg(test)]\n\
         pub(crate) mod more_tests {\n\
         \x20   fn t(b: Buffer) { let Buffer { content, .. } = b; }\n\
         }\n",
        &[
            ("C1", 3, "destructures"),
            ("C1", 6, "destructures"),
            ("C1", 9, "destructures"),
            ("C1", 14, "destructures"),
            ("C1", 28, "destructures"),
            ("C1", 34, "destructures"),
            ("C1", 40, "destructures"),
            ("C1", 41, "destructures"),
            ("C1", 42, "destructures"),
            ("C1", 47, "destructures"),
            ("C1", 54, "renames"),
            ("C1", 55, "renames"),
        ],
    )
}

#[test]
fn bare_buffer_checks_use_provenance() -> TestResult {
    expect(
        "mod model { pub struct Buffer { pub value: u8 } }\n\
         use crate::model::Buffer;\n\
         type Local = Buffer;\n\
         fn local(b: Local) { let Buffer { value } = b; }\n\
         mod direct {\n\
         \x20   use ratatui::buffer::Buffer;\n\
         \x20   type Screen = Buffer;\n\
         \x20   fn check(b: Buffer) { let Buffer { content, .. } = b; }\n\
         }\n\
         mod bridge { pub use ratatui::buffer::Buffer; }\n\
         mod unproven {\n\
         \x20   use crate::bridge::*;\n\
         \x20   type Screen = Buffer;\n\
         \x20   fn check(b: Buffer) { let Buffer { content, .. } = b; }\n\
         }\n\
         mod explicit {\n\
         \x20   use crate::bridge::Buffer as Cells;\n\
         \x20   fn check(b: Cells) { let Cells { content, .. } = b; }\n\
         }\n\
         mod local {\n\
         \x20   mod bridge { pub use crate::model::Buffer; }\n\
         \x20   use self::bridge::Buffer as Cells;\n\
         \x20   fn check(b: Cells) { let Cells { value } = b; }\n\
         }\n\
         mod editing {\n\
         \x20   use self::Buffer as Pending;\n\
         \x20   use self::Pending as Buffer;\n\
         \x20   type Screen = Buffer;\n\
         }\n",
        &[
            ("C1", 7, "renames"),
            ("C1", 8, "destructures"),
            ("C1", 13, "renames"),
            ("C1", 14, "destructures"),
            ("C1", 17, "renames"),
            ("C1", 18, "destructures"),
            ("C1", 28, "renames"),
        ],
    )
}

#[test]
fn destructuring_an_identity_alias_of_ratatui_buffer_fails() -> TestResult {
    expect(
        "use ratatui::buffer::Buffer;\n\
         type Identity<T> = T;\n\
         struct Wrapper<T> { item: T }\n\
         mod model { pub struct Buffer { pub value: u8 } }\n\
         fn patterns(a: Buffer, b: crate::model::Buffer, c: Wrapper<Buffer>) {\n\
         \x20   let Identity::<Buffer> { content, .. } = a;\n\
         \x20   let Identity::<crate::model::Buffer> { value } = b;\n\
         \x20   let Wrapper::<Buffer> { item } = c;\n\
         }\n\
         mod nested {\n\
         \x20   use ratatui::buffer::Buffer;\n\
         \x20   type Wrapper<T> where T: Iterator<Item = u8> = T;\n\
         \x20   fn check(buf: Buffer) {\n\
         \x20       let Wrapper::<Buffer> { content, .. } = buf;\n\
         \x20       let Wrapper::<crate::model::Buffer> { value } = crate::model::Buffer { value: 0 };\n\
         \x20   }\n\
         }\n\
         fn local(buf: crate::model::Buffer) {\n\
         \x20   use crate::model::Buffer;\n\
         \x20   let Identity::<Buffer> { value } = buf;\n\
         }\n\
         fn in_macro(buf: Buffer) -> bool { matches!(buf, Identity::<Buffer> { .. }) }\n\
         mod projection { pub type Identity<T> = T; }\n\
         fn qualified(a: Buffer, b: crate::model::Buffer) {\n\
         \x20   let self::Identity::<Buffer> { content, .. } = a;\n\
         \x20   let crate::projection::Identity::<Buffer> { content, .. } = a;\n\
         \x20   let self::Identity::<crate::model::Buffer> { value } = b;\n\
         \x20   let crate::projection::Identity::<crate::model::Buffer> { value } = b;\n\
         }\n\
         mod nested_super {\n\
         \x20   fn check(a: super::Buffer) {\n\
         \x20       let super::Identity::<super::Buffer> { content, .. } = a;\n\
         \x20   }\n\
         }\n\
         mod shadowed {\n\
         \x20   use ratatui::buffer::Buffer;\n\
         \x20   type Identity<Buffer> = Buffer;\n\
         }\n",
        &[
            ("C1", 6, "destructures"),
            ("C1", 14, "destructures"),
            ("C1", 22, "destructures"),
            ("C1", 25, "destructures"),
            ("C1", 26, "destructures"),
            ("C1", 32, "destructures"),
        ],
    )
}

#[test]
fn self_patterns_follow_trait_impl_receivers() -> TestResult {
    let extern_alias = tui("extern crate self as core;\n\
         #[macro_export]\n\
         macro_rules! matches { ($($tokens:tt)*) => { false }; }\n\
         mod child {\n\
         \x20   trait View { fn inspect(&self); }\n\
         \x20   impl View for ratatui::buffer::Buffer {\n\
         \x20       fn inspect(&self) {\n\
         \x20           let _ = core::matches!(self, Self { content, .. });\n\
         \x20           let _ = ::core::matches!(self, Self { content, .. });\n\
         \x20           let _ = ::std::matches!(self, Self { content, .. });\n\
         \x20       }\n\
         \x20   }\n\
         }\n")?;
    let flat = "crates/jackin-tui/src/bin/flat.rs";
    let nested = "crates/jackin-tui/src/bin/folder/main.rs";
    let main = "crates/jackin-tui/src/main.rs";
    for binary in [flat, nested, main] {
        extern_alias.write(
            binary,
            "#[macro_use(matches)] extern crate core;\n\
             trait Inspect { fn inspect(&self); }\n\
             impl Inspect for ratatui::buffer::Buffer {\n\
             \x20   fn inspect(&self) { let _ = ::core::matches!(self, Self { content, .. }); }\n\
             }\n\
             fn main() {}\n",
        )?;
    }
    let violations = ScreenCheck::with_allowed(&[]).run(&extern_alias.repo()?);
    let locations: Vec<_> = violations
        .iter()
        .map(|violation| (violation.path.as_deref(), violation.line))
        .collect();
    assert_eq!(
        locations,
        [
            (Some(Path::new(flat)), Some(4)),
            (Some(Path::new(nested)), Some(4)),
            (Some(Path::new(LIB)), Some(10)),
            (Some(Path::new(main)), Some(4)),
        ],
        "{violations:#?}"
    );
    assert!(
        violations
            .iter()
            .all(|violation| violation.message.starts_with("C1: destructures"))
    );
    expect(
        "mod model { pub struct Buffer { pub value: u8 } }\n\
         trait View { fn inspect(&self); }\n\
         impl View for (ratatui::buffer::Buffer) {\n\
         \x20   fn inspect(&self) {\n\
         \x20       let _ = ::core::stringify!(Self { content, .. });\n\
         \x20       let Self { content, .. } = self;\n\
         \x20       macro_rules! read_fields {\n\
         \x20           () => {{\n\
         \x20               let _ = ::core::stringify!(Self { content, .. });\n\
         \x20               let Self { content, .. } = self;\n\
         \x20           }};\n\
         \x20           (fields) => {{\n\
         \x20               let Self { content, .. } = self;\n\
         \x20           }};\n\
         \x20       }\n\
         \x20       read_fields!();\n\
         \x20       macro_rules! disabled_read {\n\
         \x20           () => {{ let Self { content, .. } = self; }};\n\
         \x20       }\n\
         \x20       #[cfg(any())]\n\
         \x20       disabled_read!();\n\
         \x20       {\n\
         \x20           #[macro_use]\n\
         \x20           mod shadow { macro_rules! matches { ($($tokens:tt)*) => { false }; } }\n\
         \x20           let _ = matches!(self, Self { content, .. });\n\
         \x20       }\n\
         \x20       #[cfg(any())] macro_rules! matches { ($($tokens:tt)*) => { false }; }\n\
         \x20       #[cfg(any())] use ::core::matches;\n\
         \x20       let _ = matches!(if false | false || true { self } else { return }, r#if @ Self { content, .. } if {\n\
         \x20           let _ = ::core::stringify!(Self { content, .. });\n\
         \x20           !content.is_empty()\n\
         \x20       });\n\
         \x20       {\n\
         \x20           #[cfg(any())] use crate::model as core;\n\
         \x20           use core::matches;\n\
         \x20           let _ = matches!(self, Self { content, .. });\n\
         \x20       }\n\
         \x20       macro_rules! matches { ($($tokens:tt)*) => { false }; }\n\
         \x20       let _ = matches!(self, Self { content, .. });\n\
         \x20       let _ = loop { break ::core::matches!(self, Self { content, .. }); };\n\
         \x20       let _ = std::matches!(self, Self { content, .. });\n\
         \x20       {\n\
         \x20           mod std { macro_rules! matches { ($($tokens:tt)*) => { false }; } pub(super) use matches; }\n\
         \x20           let _ = std::matches!(self, Self { content, .. });\n\
         \x20       }\n\
         \x20   }\n\
         }\n\
         impl View for crate::model::Buffer {\n\
         \x20   fn inspect(&self) { let Self { value } = self; }\n\
         }\n\
         type Identity<T> = T;\n\
         trait AliasView { fn inspect_alias(&self); }\n\
         impl AliasView for (Identity<(ratatui::buffer::Buffer)>) {\n\
         \x20   fn inspect_alias(&self) {\n\
         \x20       let _ = ::core::stringify!(Self { content, .. });\n\
         \x20       let Self { content, .. } = self;\n\
         \x20   }\n\
         }\n\
         impl AliasView for Identity<crate::model::Buffer> {\n\
         \x20   fn inspect_alias(&self) { let Self { value } = self; }\n\
         }\n\
         type Default<T = (ratatui::buffer::Buffer)> = (T);\n\
         type Forward = Default;\n\
         trait DefaultView { fn inspect_default(&self); }\n\
         impl DefaultView for Forward {\n\
         \x20   fn inspect_default(&self) { let Self { content, .. } = self; }\n\
         }\n",
        &[
            ("C1", 6, "destructures"),
            ("C1", 10, "destructures"),
            ("C1", 29, "destructures"),
            ("C1", 36, "destructures"),
            ("C1", 40, "destructures"),
            ("C1", 41, "destructures"),
            ("C1", 56, "destructures"),
            ("C1", 66, "destructures"),
        ],
    )
}

#[test]
fn out_of_line_buffer_reexports_preserve_provenance() -> TestResult {
    let workspace = tui("mod bridge;\n\
         #[allow(dead_code)] #[macro_use] mod model;\n\
         mod relay;\n\
         mod r#mod;\n\
         fn check(a: bridge::Buffer, b: model::Buffer, c: relay::Buffer, d: r#mod::Buffer) {\n\
         \x20   let bridge::Buffer { content, .. } = a;\n\
         \x20   let model::Buffer { value } = b;\n\
         \x20   let relay::Buffer { content, .. } = c;\n\
         \x20   let r#mod::Buffer { content, .. } = d;\n\
         }\n\
         trait Inspect { fn inspect(&self); }\n\
         impl Inspect for ratatui::buffer::Buffer {\n\
         \x20   fn inspect(&self) { let _ = matches!(self, Self { content, .. }); }\n\
         }\n")?;
    workspace.write(
        "crates/jackin-tui/src/bridge.rs",
        "pub use ratatui::buffer::Buffer;\n",
    )?;
    workspace.write(
        "crates/jackin-tui/src/model.rs",
        "pub struct Buffer { pub value: u8 }\n",
    )?;
    workspace.write(
        "crates/jackin-tui/src/relay/mod.rs",
        "pub use crate::bridge::Buffer;\n",
    )?;
    workspace.write(
        "crates/jackin-tui/src/mod.rs",
        "pub use ratatui::buffer::Buffer;\n",
    )?;
    expect_workspace(
        ScreenCheck::with_allowed(&[]),
        &workspace,
        &[
            ("C1", 6, "destructures"),
            ("C1", 8, "destructures"),
            ("C1", 9, "destructures"),
            ("C1", 13, "destructures"),
        ],
    )
}

#[test]
fn alias_patterns_align_elided_lifetimes() -> TestResult {
    expect(
        "use ratatui::buffer::Buffer;\n\
         mod local { pub struct Buffer { pub value: u8 } }\n\
         type Identity<'a, T> = T;\n\
         type WithConst<'a, const C: char, T> = T;\n\
         type Default<'a, const N: usize, T = Buffer> = T;\n\
         type Mark<'℘, T> = T;\n\
         fn check(a: Buffer, b: local::Buffer) {\n\
         \x20   let Identity::<Buffer> { ref content, .. } = a;\n\
         \x20   let Identity::<'_, Buffer> { ref content, .. } = a;\n\
         \x20   let Identity::<local::Buffer> { value } = b;\n\
         \x20   let WithConst::<'x', Buffer> { ref content, .. } = a;\n\
         \x20   let Default::<'_, 7> { ref content, .. } = a;\n\
         \x20   let Default::<7> { ref content, .. } = a;\n\
         \x20   let Mark::<Buffer> { ref content, .. } = a;\n\
         }\n\
         fn raw<'r#match>(value: &'r#match Buffer) -> &'r#match Buffer { value }\n",
        &[
            ("C1", 8, "destructures"),
            ("C1", 9, "destructures"),
            ("C1", 11, "destructures"),
            ("C1", 12, "destructures"),
            ("C1", 13, "destructures"),
            ("C1", 14, "destructures"),
        ],
    )
}

#[test]
fn generic_enum_variants_are_not_buffer_patterns() -> TestResult {
    expect(
        "use ratatui::buffer::Buffer;\n\
         macro_rules! ℘ { () => { u8 }; }\n\
         enum View<T, U> { Buffer { first: T, second: U } }\n\
         enum Checked<T, U> where ℘!{}: Copy, U: Copy {\n\
         \x20   Buffer { first: T, second: U }\n\
         }\n\
         fn check(buf: Buffer) {\n\
         \x20   let Buffer { content, .. } = buf;\n\
         }\n",
        &[("C1", 8, "destructures")],
    )
}

#[test]
fn destructuring_through_composed_identity_aliases_preserves_provenance() -> TestResult {
    expect(
        "mod source { pub type Keep<const N: usize, T> = T; }\n\
         mod bridge { pub use super::source::Keep as Forward; }\n\
         mod model { pub struct Buffer { pub value: u8 } }\n\
         use ratatui::buffer::Buffer;\n\
         struct Wrapper<T> { item: T }\n\
         type Outer<T, const N: usize> = bridge::Forward<N, bridge::Forward<2, T>>;\n\
         type Wrapped<T> = bridge::Forward<1, Wrapper<T>>;\n\
         fn check(a: Buffer, b: model::Buffer, wrapped: Wrapper<Buffer>) {\n\
         \x20   let Outer::<Buffer, 7> { content, .. } = a;\n\
         \x20   let Outer::<model::Buffer, 7> { value } = b;\n\
         \x20   let Wrapped::<Buffer> { item } = wrapped;\n\
         }\n",
        &[("C1", 9, "destructures")],
    )
}

#[test]
fn omitted_alias_type_arguments_use_the_declaration_defaults() -> TestResult {
    expect(
        "mod defaults {\n\
         \x20   use ratatui::buffer::Buffer;\n\
         \x20   pub type Default<const N: usize, T = Buffer> = T;\n\
         \x20   pub type Forward<const N: usize> = Default<N>;\n\
         \x20   pub type Implicit<T = Buffer> = T;\n\
         \x20   pub type Fixed = Implicit;\n\
         }\n\
         mod model {\n\
         \x20   pub struct Buffer { pub value: u8 }\n\
         \x20   pub type Local<const N: usize, T = Buffer> = T;\n\
         \x20   pub type Implicit<T = Buffer> = T;\n\
         \x20   pub type Fixed = Implicit;\n\
         }\n\
         use model::Buffer;\n\
         fn check(a: ratatui::buffer::Buffer, b: Buffer) {\n\
         \x20   let defaults::Default::<7> { content, .. } = a;\n\
         \x20   let defaults::Default::<7, Buffer> { value } = b;\n\
         \x20   let model::Local::<7> { value } = b;\n\
         \x20   let defaults::Forward::<7> { content, .. } = a;\n\
         \x20   let defaults::Fixed { content, .. } = a;\n\
         \x20   let model::Fixed { value } = b;\n\
         }\n",
        &[
            ("C1", 16, "destructures"),
            ("C1", 19, "destructures"),
            ("C1", 20, "destructures"),
        ],
    )
}

#[test]
fn associated_type_binding_names_are_not_buffer_references() -> TestResult {
    expect(
        "trait Source { type Buffer; }\n\
         type Local<T> = Box<dyn Source<Buffer = T>>;\n\
         type UsesRatatui = Box<dyn Source<Buffer = ratatui::buffer::Buffer>>;\n",
        &[("C1", 3, "renames")],
    )
}

#[test]
fn attributed_buffer_items_preserve_provenance() -> TestResult {
    expect(
        "mod model {\n\
         \x20   #[derive(Clone)]\n\
         \x20   #[repr(C)]\n\
         \x20   pub(crate) struct Buffer { pub value: u8 }\n\
         \x20   fn check(item: Buffer) { let Buffer { value } = item; }\n\
         }\n\
         use ratatui::buffer::Buffer;\n\
         fn conditional(item: Buffer) {\n\
         \x20   #[cfg(any())]\n\
         \x20   #[derive(Clone)]\n\
         \x20   struct Buffer { value: u8 }\n\
         \x20   let Buffer { content, .. } = item;\n\
         }\n",
        &[("C1", 12, "destructures")],
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
