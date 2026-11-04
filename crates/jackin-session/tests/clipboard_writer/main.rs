// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use jackin_core::EnvSnapshot;
use jackin_session::{ClipboardTool, ClipboardWriter, HostOs};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn tool(dir: &Path, name: &str, body: &str) -> TestResult {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn recording(dir: &Path, name: &str) -> TestResult {
    let out = dir.join(format!("{name}.out"));
    tool(
        dir,
        name,
        &format!(
            "printf '%s' \"$*\" > '{args}'\n/bin/cat > '{out}'",
            args = dir.join(format!("{name}.args")).display(),
            out = out.display()
        ),
    )
}

fn writer(dir: &TempDir, tools: Vec<ClipboardTool>) -> ClipboardWriter {
    ClipboardWriter::using(tools)
        .searching(dir.path().as_os_str().to_owned())
        .within(Duration::from_secs(60))
}

#[test]
fn a_write_reaches_the_tool_on_its_stdin_unchanged() -> TestResult {
    let dir = tempfile::tempdir()?;
    recording(dir.path(), "pbcopy")?;
    let text = "line one\nline two \u{65e5}\u{672c}\u{8a9e}\ttab";
    assert!(writer(&dir, vec![ClipboardTool::Pbcopy]).write(text));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("pbcopy.out"))?,
        text
    );
    Ok(())
}

#[test]
fn a_missing_tool_falls_through_to_the_next_with_its_arguments() -> TestResult {
    let dir = tempfile::tempdir()?;
    recording(dir.path(), "xclip")?;
    let tools = vec![ClipboardTool::WlCopy, ClipboardTool::Xclip];
    assert!(writer(&dir, tools).write("Hello"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("xclip.out"))?,
        "Hello"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("xclip.args"))?,
        "-selection clipboard -in"
    );
    Ok(())
}

#[test]
fn a_tool_that_fails_or_is_absent_is_not_a_write() -> TestResult {
    let dir = tempfile::tempdir()?;
    tool(dir.path(), "pbcopy", "/bin/cat > /dev/null\nexit 1")?;
    assert!(!writer(&dir, vec![ClipboardTool::Pbcopy]).write("Hello"));
    assert!(!writer(&dir, vec![ClipboardTool::Xsel]).write("Hello"));
    assert!(!writer(&dir, Vec::new()).write("Hello"));
    Ok(())
}

#[test]
fn a_tool_that_hangs_past_the_deadline_is_not_a_write() -> TestResult {
    let dir = tempfile::tempdir()?;
    tool(dir.path(), "pbcopy", "/bin/cat > /dev/null\n/bin/sleep 3")?;
    let hung = writer(&dir, vec![ClipboardTool::Pbcopy]).within(Duration::from_millis(300));
    assert!(!hung.write("Hello"));
    Ok(())
}

#[test]
fn the_tools_follow_the_host_and_its_display() {
    let none = EnvSnapshot::from_pairs(Vec::<(String, String)>::new());
    let x11 = EnvSnapshot::from_pairs([("DISPLAY".to_owned(), ":0".to_owned())]);
    let both = EnvSnapshot::from_pairs([
        ("DISPLAY".to_owned(), ":0".to_owned()),
        ("WAYLAND_DISPLAY".to_owned(), "wayland-0".to_owned()),
    ]);
    let empty = EnvSnapshot::from_pairs([("DISPLAY".to_owned(), String::new())]);
    let tools = |os, env: &EnvSnapshot| ClipboardWriter::detect(os, env).tools().to_vec();
    assert_eq!(tools(HostOs::MacOs, &none), vec![ClipboardTool::Pbcopy]);
    assert_eq!(tools(HostOs::Unix, &none), Vec::new());
    assert_eq!(tools(HostOs::Unix, &empty), Vec::new());
    assert_eq!(
        tools(HostOs::Unix, &x11),
        vec![ClipboardTool::Xclip, ClipboardTool::Xsel]
    );
    assert_eq!(
        tools(HostOs::Unix, &both),
        vec![
            ClipboardTool::WlCopy,
            ClipboardTool::Xclip,
            ClipboardTool::Xsel
        ]
    );
    assert_eq!(tools(HostOs::Other, &both), Vec::new());
}
