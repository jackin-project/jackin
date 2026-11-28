// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_xtask::{Check, Repo, RepoFile, ScreenCheck};

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
fn uncertain_macro_drawing_lint_level_is_reported() -> TestResult {
    expect(
        "macro_rules! uncertain {\n\
         ($level:ident) => { #[$level(clippy::disallowed_fields)] fn marker() {} };\n\
         }\n\
         macro_rules! ordinary { ($value:expr) => { $value }; }\n\
         macro_rules! unrelated { ($level:ident) => { #[$level(unused)] fn marker() {} }; }\n\
         macro_rules! fixed { ($reason:literal) => { #[deny(clippy::disallowed_fields, reason = $reason)] fn marker() {} }; }\n\
         macro_rules! strict { ($reason:literal) => { #[forbid(clippy::disallowed_methods, reason = $reason)] fn marker() {} }; }\n\
         macro_rules! matcher { (#[$level:ident(clippy::disallowed_fields)]) => { () }; }\n\
         macro_rules! inert { ($level:ident) => { ::core::stringify!(#[$level(clippy::disallowed_fields)]) }; }\n\
         macro_rules! inner { ($level:ident) => { mod marker { #![$level(clippy::disallowed_fields)] } }; }\n\
         ::core::stringify!(macro_rules! hidden { ($level:ident) => { #[$level(clippy::disallowed_fields)] fn marker() {} }; });\n\
         macro_rules! return_inert { ($level:ident) => { fn marker() { return ::core::stringify!(#[$level(clippy::disallowed_fields)]); } }; }\n",
        &[
            ("C1", 2, "cannot be proven"),
            ("C1", 10, "cannot be proven"),
        ],
    )
}

#[test]
fn local_wildcard_reexports_preserve_buffer_provenance() -> TestResult {
    expect(
        "mod source { #[allow(unused_imports)] pub use ratatui::buffer::Buffer; }\n\
         mod bridge { #[allow(unused_imports)] pub use super::source::*; }\n\
         use bridge::Buffer as Cells;\n\
         fn drawing(value: bridge::Buffer) { let bridge::Buffer { content, .. } = value; }\n\
         mod nominal { pub struct Buffer { pub value: u8 } }\n\
         mod local { use super::nominal::*; fn view(value: Buffer) { let Buffer { value } = value; } }\n\
         mod explicit { use super::source::*; struct Buffer { value: u8 } fn view(value: Buffer) { let Buffer { value } = value; } }\n\
         mod first { pub use super::source::*; }\n\
         mod second { pub use super::source::*; }\n\
         mod both { pub use super::first::*; pub use super::second::*; }\n\
         fn combined(value: both::Buffer) { let both::Buffer { content, .. } = value; }\n\
         mod values { pub fn helper() {} pub use self::helper as ratatui; }\n\
         mod value_scope { use super::values::*; fn view() { let _: Option<ratatui::widgets::Block> = None; } }\n",
        &[
            ("C1", 3, "Buffer"),
            ("C1", 4, "Buffer"),
            ("C1", 11, "Buffer"),
            ("C1", 13, "widgets"),
        ],
    )
}

#[test]
fn wildcard_super_obeys_file_module_policy() -> TestResult {
    let workspace = tui("mod source { pub use ratatui::buffer::Buffer; }\nmod wildcard;\n")?;
    workspace.write("crates/jackin-tui/src/wildcard.rs", "pub use super::source::*;\nuse self::Buffer as Cells;\nfn view(value: Buffer) { let Buffer { content, .. } = value; }\n")?;
    let violations = ScreenCheck::with_allowed(&[]).run(&workspace.repo()?);
    assert_eq!(violations.len(), 1, "{violations:#?}");
    assert_eq!(
        violations[0].path.as_deref(),
        Some(Path::new("crates/jackin-tui/src/wildcard.rs"))
    );
    assert_eq!(violations[0].line, Some(3));
    assert!(violations[0].message.contains("destructures"));
    Ok(())
}

#[test]
fn conditional_canonical_root_reports_uncertainty() -> TestResult {
    expect(
        "fn normal() { let _: Option<ratatui::widgets::Block> = None; }\n\
         mod uncertain {\n\
         #[cfg(windows)] mod ratatui { pub mod widgets { pub struct Block; } }\n\
         fn view() { let _: Option<ratatui::widgets::Block> = None; }\n\
         fn pattern(value: ratatui::buffer::Buffer) { let ratatui::buffer::Buffer { content, .. } = value; }\n\
         type Cells = ratatui::buffer::Buffer;\n\
         use ::{ratatui::widgets::Block};\n\
         fn allowed() { let _: Option<ratatui::Frame> = None; }\n\
         }\n\
         mod active { #[cfg(all())] mod ratatui { pub mod widgets { pub struct Block; } pub mod buffer { pub struct Buffer { pub content: u8 } } } fn view(value: ratatui::buffer::Buffer) { let ratatui::buffer::Buffer { content } = value; let _: Option<ratatui::widgets::Block> = None; } }\n",
        &[
            ("C1", 1, "widgets"),
            ("C1", 4, "uncertain"),
            ("C1", 5, "uncertain"),
            ("C1", 6, "uncertain"),
            ("C1", 7, "widgets"),
        ],
    )
}

#[test]
fn standard_matches_guard_preserves_field_provenance() -> TestResult {
    expect(
        "use ratatui::buffer::Buffer;\n\
         trait Inspect { fn checked(&self); fn shadowed(&self); fn opaque(&self); }\n\
         impl Inspect for Buffer { fn checked(&self) {\n\
         let _ = matches!((), _ if {\n\
         let Self { content, .. } = self; true\n\
         });\n\
         }\n\
         fn shadowed(&self) { macro_rules! matches { ($($t:tt)*) => { true } } let _ = matches!((), _ if { type Identity<T> = T; let Identity::<Self> { content, .. } = self; true }); }\n\
         fn opaque(&self) { let _ = matches!((), _ if { ::core::stringify!(type Identity<T> = T; let Identity::<Self> { content, .. } = self); true }); }\n\
         }\n",
        &[("C1", 5, "destructures")],
    )
}

#[test]
fn builtin_include_reports_unchecked_source() -> TestResult {
    let workspace = tui("fn load() {\n\
        include!(\"fragment.inc\");\n\
        ::core::include!(\"fragment.rs\");\n\
        let _ = include_str!(\"fragment.inc\");\n\
        let _ = include_bytes!(\"fragment.inc\");\n\
        }\n\
        fn shadow() { macro_rules! include { ($t:tt) => { () } } include!(\"fragment.inc\"); }\n\
        fn inert() { ::core::stringify!(include!(\"fragment.inc\")); }\n\
        fn disabled() { #[cfg(any())] ::core::include!(\"fragment.rs\"); }\n\
        fn guard() { let _ = matches!((), _ if { macro_rules! include { ($t:tt) => { () } } include!(\"fragment.inc\"); true }); }\n")?;
    workspace.write(
        "crates/jackin-tui/src/fragment.inc",
        "const FIRST_ITEM: u8 = 0;\n",
    )?;
    workspace.write(
        "crates/jackin-tui/src/fragment.rs",
        "const SECOND_ITEM: u8 = 0;\n",
    )?;
    expect_workspace(
        ScreenCheck::with_allowed(&["fragment.inc", "fragment.rs"]),
        &workspace,
        &[("C1", 2, "include"), ("C1", 3, "include")],
    )
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
        &"mod model { pub struct Buffer { pub value: u8 } }\n\
         trait View { fn inspect(&self); }\n\
         impl View for (ratatui::buffer::Buffer) {\n\
         \x20   fn inspect(&self) {\n\
         \x20       let _ = ::core::stringify!(Self { content, .. });\n\
         \x20       let Self { content, .. } = self;\n\
         \x20       macro_rules! read_fields {\n\
         \x20           (/// matcher documentation\n\
         \x20           ) => {{\n\
         \x20               let _ = ::core::stringify!(Self { content, .. });\n\
         \x20               let Self { content, .. } = self;\n\
         \x20           }};\n\
         \x20           (ignored) => {{\n\
         \x20               let Self { content, .. } = self;\n\
         \x20           }};\n\
         \x20           (fields) => {{\n\
         \x20               let Self { content, .. } = self;\n\
         \x20           }};\n\
         \x20           (é) => {{\n\
         \x20               let Self { content, .. } = self;\n\
         \x20           }};\n\
         \x20           (e\u{301}) => {{\n\
         \x20               let Self { content, .. } = self;\n\
         \x20           }};\n\
         \x20       }\n\
         \x20       macro_rules! LONG_MACRO_NAME { () => { () }; }\n\
         \x20       {\n\
         \x20           #![allow(unused_variables)]\n\
         \x20           read_fields!(/* ordinary comment */);\n\
         \x20           read_fields!(fields);\n\
         \x20           read_fields!(e\u{301});\n\
         \x20       }\n\
         \x20       macro_rules! disabled_read {\n\
         \x20           () => {{ let Self { content, .. } = self; }};\n\
         \x20       }\n\
         \x20       #[cfg(any())]\n\
         \x20       disabled_read!();\n\
         \x20       macro_rules! documented_read {\n\
         \x20           () => {{ let Self { content, .. } = self; }};\n\
         \x20           (#[$doc:meta]) => { () };\n\
         \x20       }\n\
         \x20       documented_read!(/** ordinary documentation */);\n\
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
         }\n".replace("LONG_MACRO_NAME", &"é".repeat(129)),
        &[
            ("C1", 6, "destructures"),
            ("C1", 11, "destructures"),
            ("C1", 17, "destructures"),
            ("C1", 20, "destructures"),
            ("C1", 50, "destructures"),
            ("C1", 57, "destructures"),
            ("C1", 61, "destructures"),
            ("C1", 62, "destructures"),
            ("C1", 77, "destructures"),
            ("C1", 87, "destructures"),
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
         \x20   let _ = matches!(&a, defaults::Fixed { content, .. } if !content.is_empty());\n\
         \x20   let _ = matches!(&b, model::Fixed { value: _ });\n\
         \x20   let _ = ::core::stringify!(defaults::Fixed { content, .. });\n\
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
            ("C1", 22, "destructures"),
            ("C1", 23, "destructures"),
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

#[test]
fn standard_matches_preserves_nominal_buffer_provenance() -> TestResult {
    expect(
        "struct Buffer { value: u8 }\n\
         fn inspect(local: Buffer, real: ratatui::buffer::Buffer) {\n\
         \x20   let _ = matches!(local, Buffer { value: 1 });\n\
         \x20   let _ = ::core::matches!(real, ratatui::buffer::Buffer { content, .. });\n\
         }\n",
        &[("C1", 4, "destructures")],
    )
}

#[test]
fn inert_attributes_preserve_inline_module_provenance() -> TestResult {
    expect(
        "#[allow(dead_code)] mod bridge { pub use ratatui::buffer::Buffer; }\n\
         use bridge::Buffer as Cells;\n\
         fn inspect(buf: bridge::Buffer) { let bridge::Buffer { content, .. } = buf; }\n",
        &[("C1", 2, "renames"), ("C1", 3, "destructures")],
    )
}

#[test]
fn generic_alias_argument_self_uses_its_implementation() -> TestResult {
    expect(
        "type Identity<T> = T;\n\
         type Forward<T> = Identity<T>;\n\
         struct Local { value: u8 }\n\
         trait Inspect { fn inspect(&self); }\n\
         impl Inspect for ratatui::buffer::Buffer {\n\
         \x20   fn inspect(&self) { let Forward::<Self> { content, .. } = self; }\n\
         }\n\
         impl Inspect for Local {\n\
         \x20   fn inspect(&self) { let Forward::<Self> { value } = self; }\n\
         }\n",
        &[("C1", 6, "destructures")],
    )
}

#[test]
fn inert_attributes_preserve_file_module_provenance() -> TestResult {
    let workspace = tui("#[allow(dead_code)] mod bridge;\n\
         fn inspect(buf: bridge::Buffer) { let bridge::Buffer { content, .. } = buf; }\n")?;
    workspace.write(
        "crates/jackin-tui/src/bridge.rs",
        "pub use ratatui::buffer::Buffer;\n",
    )?;
    expect_workspace(
        ScreenCheck::with_allowed(&[]),
        &workspace,
        &[("C1", 2, "destructures")],
    )
}

#[test]
fn cargo_aliases_preserve_ratatui_provenance() -> TestResult {
    let workspace = Workspace::empty()?;
    workspace.write(
        "Cargo.toml",
        &workspace_manifest(
            &["jackin-tui", "ratatui"],
            &["rt = { package = \"ratatui\", path = \"crates/ratatui\" }".to_owned()],
        ),
    )?;
    workspace.write(
        "crates/jackin-tui/Cargo.toml",
        &member_manifest(
            "jackin-tui",
            "[target.'cfg(unix)'.dev-dependencies]\nrt.workspace = true\n",
        ),
    )?;
    workspace.write("crates/ratatui/Cargo.toml", &member_manifest("ratatui", ""))?;
    workspace.write("crates/ratatui/src/lib.rs", "")?;
    workspace.write(
        LIB,
        "use rt::buffer::Buffer as Cells;\n\
         use rt::widgets::Paragraph;\n\
         trait Inspect { fn inspect(&self); }\n\
         impl Inspect for rt::buffer::Buffer {\n\
         \x20   fn inspect(&self) { let Self { content, .. } = self; }\n\
         }\n\
         mod local {\n\
         \x20   mod rt { pub mod buffer { pub struct Buffer { pub value: u8 } } }\n\
         \x20   fn inspect(buf: rt::buffer::Buffer) { let rt::buffer::Buffer { value } = buf; }\n\
         }\n",
    )?;
    expect_workspace(
        ScreenCheck::with_allowed(&[]),
        &workspace,
        &[
            ("C1", 1, "renames"),
            ("C1", 2, "widgets"),
            ("C1", 5, "destructures"),
        ],
    )
}

#[test]
fn unrelated_cargo_alias_does_not_gain_ratatui_provenance() -> TestResult {
    let workspace = Workspace::empty()?;
    workspace.write(
        "Cargo.toml",
        &workspace_manifest(&["jackin-tui", "other"], &[]),
    )?;
    workspace.write(
        "crates/jackin-tui/Cargo.toml",
        &member_manifest(
            "jackin-tui",
            "[dev-dependencies]\nrt = { package = \"other\", path = \"../other\" }\n",
        ),
    )?;
    workspace.write("crates/other/Cargo.toml", &member_manifest("other", ""))?;
    workspace.write("crates/other/src/lib.rs", "")?;
    workspace.write(LIB, "use rt::{widgets::Paragraph, buffer::Buffer as Cells};\ntrait Inspect { fn inspect(&self); }\nimpl Inspect for rt::buffer::Buffer { fn inspect(&self) { let Self { value } = self; } }\n")?;
    expect_workspace(ScreenCheck::with_allowed(&[]), &workspace, &[])
}

#[test]
fn cargo_alias_import_trees_keep_root_provenance() -> TestResult {
    let workspace = Workspace::empty()?;
    workspace.write(
        "Cargo.toml",
        &workspace_manifest(&["jackin-tui", "ratatui"], &[]),
    )?;
    workspace.write(
        "crates/jackin-tui/Cargo.toml",
        &member_manifest(
            "jackin-tui",
            "[dev-dependencies]\nrt = { package = \"ratatui\", path = \"../ratatui\" }\n",
        ),
    )?;
    workspace.write("crates/ratatui/Cargo.toml", &member_manifest("ratatui", ""))?;
    workspace.write("crates/ratatui/src/lib.rs", "")?;
    workspace.write(
        LIB,
        "mod grouped {\n\
         \x20   use rt::{widgets::Paragraph, buffer::Buffer as Cells};\n\
         }\n\
         mod rooted {\n\
         \x20   mod rt { pub mod widgets { pub struct Paragraph; } }\n\
         \x20   use ::{rt::widgets::Paragraph};\n\
         \x20   use {::rt::buffer::Buffer as Cells};\n\
         \x20   fn local(_: rt::widgets::Paragraph) {}\n\
         }\n\
         mod inactive {\n\
         \x20   #[cfg(any())] mod rt {}\n\
         \x20   use rt::widgets::Paragraph;\n\
         }\n\
         mod globs {\n\
         \x20   use rt::widgets::*;\n\
         \x20   use rt::style::*;\n\
         \x20   use rt::widgets::Paragraph;\n\
         }\n\
         mod attributed {\n\
         \x20   #[cfg(unix)] use rt::buffer::Buffer as Cells;\n\
         }\n\
         fn value(rt: u8) -> u8 { rt }\n\
         mod bare { use rt; }\n",
    )?;
    expect_workspace(
        ScreenCheck::with_allowed(&[]),
        &workspace,
        &[
            ("C1", 2, "widgets"),
            ("C1", 2, "renames"),
            ("C1", 6, "widgets"),
            ("C1", 7, "renames"),
            ("C1", 12, "widgets"),
            ("C1", 15, "widgets"),
            ("C1", 16, "style"),
            ("C1", 17, "widgets"),
            ("C1", 20, "renames"),
            ("C1", 23, "imports rt"),
        ],
    )
}

#[test]
fn explicit_extern_self_precedes_cargo_alias() -> TestResult {
    let workspace = Workspace::empty()?;
    workspace.write(
        "Cargo.toml",
        &workspace_manifest(&["jackin-tui", "ratatui"], &[]),
    )?;
    workspace.write(
        "crates/jackin-tui/Cargo.toml",
        &member_manifest(
            "jackin-tui",
            "[dev-dependencies]\nrt = { package = \"ratatui\", path = \"../ratatui\" }\n",
        ),
    )?;
    workspace.write("crates/ratatui/Cargo.toml", &member_manifest("ratatui", ""))?;
    workspace.write("crates/ratatui/src/lib.rs", "")?;
    workspace.write(LIB, "extern crate self as rt;\nmod widgets { pub struct Paragraph; }\nuse {::rt::widgets::Paragraph};\nmod nested { use rt::widgets::Paragraph; }\n")?;
    expect_workspace(ScreenCheck::with_allowed(&[]), &workspace, &[])
}

#[test]
fn canonical_ratatui_paths_survive_unrelated_glob_imports() -> TestResult {
    expect(
        "mod other { pub struct X; }\n\
         use other::*;\n\
         fn inspect(b: ratatui::buffer::Buffer) { let ratatui::buffer::Buffer { content, .. } = b; let _ = ratatui::widgets::Paragraph; }\n",
        &[("C1", 3, "destructures"), ("C1", 3, "widgets")],
    )
}

#[test]
fn attributed_alias_parameters_preserve_effective_positions() -> TestResult {
    expect(
        "use ratatui::buffer::Buffer;\n\
         #[derive(Clone, Copy)] struct Local { value: u8 }\n\
         struct Wrapper<T> { value: u8, marker: std::marker::PhantomData<T> }\n\
         type Inert<#[allow(unused)] T> = T;\n\
         type Active<#[cfg(all())] T> = T;\n\
         type Shift<#[cfg(any())] Hidden, T> = T;\n\
         type Maybe<#[cfg(unix)] T> = T;\n\
         type Ambiguous<#[cfg(windows)] X = Local, T = Buffer> = T;\n\
         type Concrete<#[cfg(unix)] X> = Wrapper<X>;\n\
         fn inspect(a: Buffer, b: Buffer, c: Buffer, d: Buffer, local: Local, wrapped: Wrapper<Local>) {\n\
         \x20   let Inert::<Buffer> { content, .. } = a;\n\
         \x20   let Active::<Buffer> { content, .. } = b;\n\
         \x20   let Shift::<Buffer> { content, .. } = c;\n\
         \x20   let Maybe::<Buffer> { content, .. } = d;\n\
         \x20   let Ambiguous::<Local> { value } = local;\n\
         \x20   let Maybe::<Local> { value } = local;\n\
         \x20   let Concrete::<Local> { value, .. } = wrapped;\n\
         }\n",
        &[
            ("C1", 11, "destructures"),
            ("C1", 12, "destructures"),
            ("C1", 13, "destructures"),
            ("C1", 14, "destructures"),
            ("C1", 15, "conditional"),
        ],
    )
}

#[test]
fn conditional_nominal_proof_respects_outer_alias_parameters() -> TestResult {
    expect(
        "use ratatui::buffer::Buffer;\n\
         struct T { value: u8 }\n\
         struct Local { value: u8 }\n\
         type Ambiguous<#[cfg(windows)] X = Local, U = Local> = U;\n\
         type Outer<T> = Ambiguous<T>;\n\
         type Certain<T> = T;\n\
         fn inspect(buf: Buffer, local: Local) {\n\
         \x20   let Outer::<Buffer> { content, .. } = buf;\n\
         \x20   let Certain::<Local> { value } = local;\n\
         }\n",
        &[("C1", 8, "conditional")],
    )
}

#[test]
fn generic_alias_self_in_standard_matches_keeps_receiver_scope() -> TestResult {
    expect(
        "type Identity<T> = T;\n\
         trait Inspect { fn inspect(&self) -> bool; }\n\
         impl Inspect for ratatui::buffer::Buffer {\n\
         \x20   fn inspect(&self) -> bool {\n\
         \x20       let _ = ::core::matches!(self, Identity::<Self> { content, .. } if !content.is_empty());\n\
         \x20       let _ = stringify!(Identity::<Self> { content, .. });\n\
         \x20       matches!(self, Identity::<Self> { content, .. })\n\
         \x20   }\n\
         }\n",
        &[("C1", 5, "destructures"), ("C1", 7, "destructures")],
    )
}

#[test]
fn imported_named_enum_variants_preserve_nominal_provenance() -> TestResult {
    expect(
        "mod model { #[repr(u8)] pub enum View { Other = 1 << 2, Buffer { value: u8 } } }\n\
         use model::View::Buffer;\n\
         fn inspect(local: model::View, actual: ratatui::buffer::Buffer) {\n\
         \x20   let Buffer { value } = local else { return; };\n\
         \x20   let ratatui::buffer::Buffer { content, .. } = actual;\n\
         }\n",
        &[("C1", 5, "destructures")],
    )
}

#[test]
fn nfc_equivalent_identifiers_preserve_type_provenance() -> TestResult {
    expect(
        "mod café { pub use ratatui::buffer::Buffer; }\n\
         mod cafe\u{301}two { pub use ratatui::buffer::Buffer; }\n\
         mod domain { pub struct Buffer { pub value: u8 } }\n\
         fn inspect(a: café::Buffer, b: cafe\u{301}two::Buffer, c: domain::Buffer) {\n\
         \x20   let cafe\u{301}::Buffer { content, .. } = a;\n\
         \x20   let cafétwo::Buffer { content, .. } = b;\n\
         \x20   let domain::Buffer { value } = c;\n\
         }\n\
         mod r#écran { pub struct Buffer { pub value: u8 } }\n\
         fn raw(v: r#écran::Buffer) { let r#e\u{301}cran::Buffer { value } = v; }\n",
        &[("C1", 5, "destructures"), ("C1", 6, "destructures")],
    )
}

#[test]
fn cargo_targets_resolve_their_own_crate_roots() -> TestResult {
    let workspace = Workspace::empty()?;
    workspace.write("Cargo.toml", &workspace_manifest(&["jackin-tui"], &[]))?;
    workspace.write(
        "crates/jackin-tui/Cargo.toml",
        &member_manifest("jackin-tui", "[lib]\ncrate-type = [\"rlib\"]\n[[bin]]\nname = \"custom\"\npath = \"src/custom.rs\"\n[[bin]]\nname = \"unicode\"\npath = \"src/cafe\u{301}.rs\"\n"),
    )?;
    workspace.write(
        LIB,
        "mod bridge { pub struct Buffer { pub content: Vec<u8> } }\nmod shared;\nmod special { pub use ratatui::buffer::Buffer; } fn inspect(b: special::Buffer) { let crate::special::Buffer { content, .. } = b; }\n",
    )?;
    workspace.write("crates/jackin-tui/src/main.rs", "mod bridge { pub use ratatui::buffer::Buffer; }\nmod shared;\nfn inspect(b: bridge::Buffer) { let crate::bridge::Buffer { content, .. } = b; }\n")?;
    workspace.write("crates/jackin-tui/src/bin/flat.rs", "mod bridge { pub use ratatui::buffer::Buffer; }\nfn inspect(b: bridge::Buffer) { let crate::bridge::Buffer { content, .. } = b; }\n")?;
    workspace.write("crates/jackin-tui/src/bin/folder/main.rs", "mod bridge { pub use ratatui::buffer::Buffer; }\nfn inspect(b: bridge::Buffer) { let crate::bridge::Buffer { content, .. } = b; }\n")?;
    workspace.write("crates/jackin-tui/src/custom.rs", "mod bridge { pub use ratatui::buffer::Buffer; }\nfn inspect(b: bridge::Buffer) { let crate::bridge::Buffer { content, .. } = b; }\n")?;
    workspace.write("crates/jackin-tui/src/cafe\u{301}.rs", "mod bridge { pub use ratatui::buffer::Buffer; }\nfn inspect(b: bridge::Buffer) { let crate::bridge::Buffer { content, .. } = b; }\n")?;
    workspace.write(
        "crates/jackin-tui/src/shared.rs",
        "fn inspect(b: crate::bridge::Buffer) { let crate::bridge::Buffer { content, .. } = b; }\n",
    )?;
    workspace.write(
        "crates/jackin-tui/src/orphan.rs",
        "fn inspect(b: crate::bridge::Buffer) { let crate::bridge::Buffer { content, .. } = b; }\n",
    )?;
    let repo = workspace.repo()?;
    let target = std::fs::canonicalize(
        workspace
            .root()
            .join("crates/jackin-tui/src/cafe\u{301}.rs"),
    )?;
    let unicode_paths: Vec<_> = repo
        .files()
        .iter()
        .map(RepoFile::path)
        .filter(|path| {
            std::fs::canonicalize(repo.root().join(path)).is_ok_and(|candidate| candidate == target)
        })
        .collect();
    assert_eq!(unicode_paths.len(), 1);
    let unicode_path = unicode_paths[0];
    let violations = ScreenCheck::with_allowed(&[]).run(&repo);
    let actual: Vec<_> = violations
        .iter()
        .map(|violation| {
            (
                violation.path.as_deref(),
                violation.line,
                violation.message.as_str(),
            )
        })
        .collect();
    let expected = [
        (Path::new("crates/jackin-tui/src/bin/flat.rs"), 2),
        (Path::new("crates/jackin-tui/src/bin/folder/main.rs"), 2),
        (unicode_path, 2),
        (Path::new("crates/jackin-tui/src/custom.rs"), 2),
        (Path::new(LIB), 3),
        (Path::new("crates/jackin-tui/src/main.rs"), 3),
        (Path::new("crates/jackin-tui/src/shared.rs"), 1),
    ];
    assert_eq!(actual.len(), expected.len(), "{actual:#?}");
    for ((path, line, message), (expected_path, expected_line)) in actual.iter().zip(expected) {
        assert_eq!(*path, Some(expected_path));
        assert_eq!(*line, Some(expected_line));
        assert!(message.contains("C1: destructures"), "{message}");
    }
    Ok(())
}

#[test]
fn nominal_enum_named_buffer_can_be_aliased() -> TestResult {
    expect(
        "enum Buffer { Packet { value: u8 } }\ntype PacketBuffer = Buffer;\n",
        &[],
    )
}

#[test]
fn renamed_standard_matches_preserves_receiver_scope() -> TestResult {
    expect(
        "use std::matches as is_match;\n\
         struct Local { value: u8 }\n\
         trait Inspect { fn inspect(&self) -> bool; }\n\
         impl Inspect for ratatui::buffer::Buffer {\n\
         \x20   fn inspect(&self) -> bool {\n\
         \x20       let _ = is_match!(self, Self { content, .. } if !content.is_empty());\n\
         \x20       { macro_rules! is_match { ($($tokens:tt)*) => { false }; } let _ = is_match!(self, Self { content, .. }); }\n\
         \x20       false\n\
         \x20   }\n\
         }\n\
         impl Inspect for Local { fn inspect(&self) -> bool { is_match!(self, Self { value: 1 }) } }\n",
        &[("C1", 6, "destructures")],
    )
}

#[test]
fn raw_keyword_module_path_preserves_buffer_provenance() -> TestResult {
    expect(
        "mod r#return { pub use ratatui::buffer::Buffer; }\n\
         mod domain { pub struct Buffer { pub value: u8 } }\n\
         fn inspect(buf: r#return::Buffer, local: domain::Buffer) {\n\
         \x20   let r#return::Buffer { content, .. } = buf;\n\
         \x20   let domain::Buffer { value } = local;\n\
         \x20   if false { return; }\n\
         }\n",
        &[("C1", 4, "destructures")],
    )
}

#[test]
fn grouped_standard_macro_alias_is_visible_before_its_import() -> TestResult {
    expect(
        "struct check;\n\
         trait Inspect { fn inspect(&self) -> bool; }\n\
         impl Inspect for ratatui::buffer::Buffer {\n\
         \x20   fn inspect(&self) -> bool {\n\
         \x20       let _ = check!(self, Self { content, .. });\n\
         \x20       macro_rules! check { ($($tokens:tt)*) => { false }; }\n\
         \x20       check!(self, Self { content, .. })\n\
         \x20   }\n\
         }\n\
         use core::{matches as check};\n",
        &[("C1", 5, "destructures")],
    )
}

#[test]
fn shared_file_deduplicates_by_pattern_occurrence() -> TestResult {
    let workspace = Workspace::empty()?;
    workspace.write("Cargo.toml", &workspace_manifest(&["jackin-tui"], &[]))?;
    workspace.write(
        "crates/jackin-tui/Cargo.toml",
        &member_manifest("jackin-tui", ""),
    )?;
    workspace.write(LIB, "mod left { pub use ratatui::buffer::Buffer; } mod right { pub struct Buffer { pub content: Vec<u8> } } mod both { pub use ratatui::buffer::Buffer; } mod shared;\n")?;
    workspace.write("crates/jackin-tui/src/main.rs", "mod left { pub struct Buffer { pub content: Vec<u8> } } mod right { pub use ratatui::buffer::Buffer; } mod both { pub use ratatui::buffer::Buffer; } mod shared; fn main() {}\n")?;
    workspace.write("crates/jackin-tui/src/shared.rs", "fn inspect(a: crate::left::Buffer, b: crate::right::Buffer) { let crate::left::Buffer { content, .. } = a; let crate::right::Buffer { content, .. } = b; }\nfn both(b: crate::both::Buffer) { let crate::both::Buffer { content, .. } = b; }\n")?;
    let violations = ScreenCheck::with_allowed(&[]).run(&workspace.repo()?);
    let actual: Vec<_> = violations
        .iter()
        .map(|violation| {
            (
                violation.path.as_deref(),
                violation.line,
                violation.message.as_str(),
            )
        })
        .collect();
    let shared = Some(Path::new("crates/jackin-tui/src/shared.rs"));
    assert_eq!(actual.len(), 3, "{actual:#?}");
    assert_eq!(
        actual
            .iter()
            .filter(|(path, line, message)| *path == shared
                && *line == Some(1)
                && message.contains("C1: destructures"))
            .count(),
        2,
        "{actual:#?}"
    );
    assert_eq!(
        actual
            .iter()
            .filter(|(path, line, message)| *path == shared
                && *line == Some(2)
                && message.contains("C1: destructures"))
            .count(),
        1,
        "{actual:#?}"
    );
    Ok(())
}
